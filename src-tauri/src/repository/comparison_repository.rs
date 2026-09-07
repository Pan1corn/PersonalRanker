//! 1v1 传递排序会话的持久化二分插入实现。
//! `sorted_group_ids` 保存已定位组，队首候选用 `[low, high)` 搜索区间逐步插入。

use std::collections::{BTreeMap, HashMap};

use chrono::Utc;
use rusqlite::{params, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    domain::{
        comparison::{
            ComparisonDecision, ComparisonItem, ComparisonSessionState, ComparisonWorkspace,
        },
        sort_task::{SortDisplayField, SortTaskMode, SortTaskStatus},
    },
    error::AppError,
    repository::{
        group_repository::merge_groups_in_transaction,
        project_repository::ProjectRepository,
        sort_task_repository::{display_value, persist_group_order},
    },
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GroupTopology {
    groups: Vec<GroupTopologyEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GroupTopologyEntry {
    id: String,
    position: usize,
    name: Option<String>,
    is_active: bool,
    merged_into_group_id: Option<String>,
    items: Vec<String>,
}

type ComparisonGroupMembers =
    HashMap<String, (Option<String>, Vec<(String, BTreeMap<String, Value>)>)>;

impl ProjectRepository {
    pub fn load_comparison_workspace(
        &mut self,
        task_id: &str,
    ) -> Result<ComparisonWorkspace, AppError> {
        self.ensure_comparison_task(task_id)?;
        let mut state = self.ensure_comparison_session(task_id)?;
        state.prepare_active();
        self.save_comparison_session(task_id, &state)?;
        self.build_comparison_workspace(task_id, state)
    }

    pub fn answer_comparison(
        &mut self,
        task_id: &str,
        decision: ComparisonDecision,
    ) -> Result<ComparisonWorkspace, AppError> {
        self.ensure_comparison_task(task_id)?;
        self.ensure_comparison_editable(task_id)?;
        let mut state = self.ensure_comparison_session(task_id)?;
        state.prepare_active();
        // 当前候选只与搜索区间中点比较，每次判断约缩小一半候选位置。
        let midpoint = state
            .midpoint()
            .ok_or_else(|| AppError::InvalidInput("当前没有需要判断的比较".into()))?;
        let active_group_id = state.queue[0].group_id.clone();
        let right_group_id = state.sorted_group_ids[midpoint].clone();
        let before_json = serde_json::to_string(&state)?;
        // 并列会改变组成员拓扑；额外保存拓扑才能让一次撤销完整拆回原组。
        let topology_before = (decision == ComparisonDecision::Tie)
            .then(|| self.capture_group_topology(task_id))
            .transpose()?;

        if decision == ComparisonDecision::Tie {
            state.queue.remove(0);
            state.prepare_active();
        } else {
            let active = &mut state.queue[0];
            match decision {
                ComparisonDecision::LeftBefore => active.high = Some(midpoint),
                ComparisonDecision::RightBefore => active.low = Some(midpoint + 1),
                ComparisonDecision::Tie => unreachable!(),
            }
            if active.low == active.high {
                let insertion_position = active.low.expect("prepared candidate has a lower bound");
                let completed = state.queue.remove(0);
                state
                    .sorted_group_ids
                    .insert(insertion_position, completed.group_id);
                adjust_deferred_ranges(&mut state, insertion_position);
                state.prepare_active();
            }
        }
        let after_json = serde_json::to_string(&state)?;
        let now = Utc::now().to_rfc3339();
        // 比较记录、可能的并列合并、会话状态和最终顺序必须作为一个原子事务提交。
        let transaction = self.connection_mut().transaction()?;
        if decision == ComparisonDecision::Tie {
            merge_groups_in_transaction(&transaction, task_id, &active_group_id, &right_group_id)?;
        }
        transaction.execute(
            "INSERT INTO comparisons (id, sort_task_id, left_group_id, right_group_id, result, source, is_undone, created_at, session_state_before_json, session_state_after_json, group_state_before_json, group_state_after_json) \
             VALUES (?1, ?2, ?3, ?4, ?5, 'user', 0, ?6, ?7, ?8, ?9, NULL)",
            params![
                Uuid::new_v4().to_string(),
                task_id,
                active_group_id,
                right_group_id,
                decision.as_str(),
                now,
                before_json,
                after_json,
                topology_before
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?,
            ],
        )?;
        upsert_session(&transaction, task_id, &state, &now)?;
        transaction.execute(
            "UPDATE sort_tasks SET status = 'sorting', updated_at = ?1 WHERE id = ?2",
            params![now, task_id],
        )?;
        if state.queue.is_empty() {
            persist_group_order(&transaction, task_id, &state.sorted_group_ids)?;
        }
        transaction.commit()?;
        self.build_comparison_workspace(task_id, state)
    }

    pub fn skip_comparison(&mut self, task_id: &str) -> Result<ComparisonWorkspace, AppError> {
        self.ensure_comparison_task(task_id)?;
        self.ensure_comparison_editable(task_id)?;
        let mut state = self.ensure_comparison_session(task_id)?;
        state.prepare_active();
        if state.queue.len() > 1 {
            let skipped = state.queue.remove(0);
            state.queue.push(skipped);
            state.prepare_active();
            self.save_comparison_session(task_id, &state)?;
        }
        self.build_comparison_workspace(task_id, state)
    }

    pub fn undo_comparison(&mut self, task_id: &str) -> Result<ComparisonWorkspace, AppError> {
        self.ensure_comparison_task(task_id)?;
        self.ensure_comparison_editable(task_id)?;
        let comparison = self
            .connection()
            .query_row(
                "SELECT id, session_state_before_json, group_state_before_json FROM comparisons \
                 WHERE sort_task_id = ?1 AND is_undone = 0 AND source = 'user' \
                 ORDER BY created_at DESC, rowid DESC LIMIT 1",
                [task_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()?;
        let Some((comparison_id, before_json, topology_before_json)) = comparison else {
            return self.load_comparison_workspace(task_id);
        };
        let mut state: ComparisonSessionState = serde_json::from_str(&before_json)?;
        state.prepare_active();
        let now = Utc::now().to_rfc3339();
        let transaction = self.connection_mut().transaction()?;
        // 普通判断只恢复会话 JSON；并列判断还需恢复活动组及成员关系。
        if let Some(topology_json) = topology_before_json {
            let topology: GroupTopology = serde_json::from_str(&topology_json)?;
            restore_group_topology(&transaction, task_id, &topology)?;
        }
        transaction.execute(
            "UPDATE comparisons SET is_undone = 1 WHERE id = ?1",
            [comparison_id],
        )?;
        upsert_session(&transaction, task_id, &state, &now)?;
        transaction.execute(
            "UPDATE sort_tasks SET status = 'sorting', updated_at = ?1 WHERE id = ?2",
            params![now, task_id],
        )?;
        transaction.commit()?;
        self.build_comparison_workspace(task_id, state)
    }

    fn ensure_comparison_task(&self, task_id: &str) -> Result<(), AppError> {
        let mode = self
            .connection()
            .query_row(
                "SELECT mode FROM sort_tasks WHERE id = ?1",
                [task_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| AppError::InvalidInput("找不到排序任务".into()))?;
        if mode != "comparison" {
            return Err(AppError::InvalidInput("该任务不是 1v1 传递排序任务".into()));
        }
        Ok(())
    }

    fn ensure_comparison_editable(&self, task_id: &str) -> Result<(), AppError> {
        let status = self.connection().query_row(
            "SELECT status FROM sort_tasks WHERE id = ?1",
            [task_id],
            |row| row.get::<_, String>(0),
        )?;
        if status == "confirmed" {
            return Err(AppError::InvalidInput(
                "结果已确认并锁定，请先在结果页解锁后再编辑".into(),
            ));
        }
        Ok(())
    }

    fn ensure_comparison_session(
        &mut self,
        task_id: &str,
    ) -> Result<ComparisonSessionState, AppError> {
        let saved = self
            .connection()
            .query_row(
                "SELECT state_json FROM comparison_sessions WHERE sort_task_id = ?1",
                [task_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if let Some(json) = saved {
            return Ok(serde_json::from_str(&json)?);
        }
        let mut state = ComparisonSessionState::initialize(self.ordered_group_ids(task_id)?);
        state.prepare_active();
        self.save_comparison_session(task_id, &state)?;
        self.connection().execute(
            "UPDATE sort_tasks SET status = 'sorting', updated_at = ?1 WHERE id = ?2 AND status = 'draft'",
            params![Utc::now().to_rfc3339(), task_id],
        )?;
        Ok(state)
    }

    fn save_comparison_session(
        &mut self,
        task_id: &str,
        state: &ComparisonSessionState,
    ) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        let transaction = self.connection_mut().transaction()?;
        upsert_session(&transaction, task_id, state, &now)?;
        transaction.commit()?;
        Ok(())
    }

    fn build_comparison_workspace(
        &self,
        task_id: &str,
        state: ComparisonSessionState,
    ) -> Result<ComparisonWorkspace, AppError> {
        let (task_name, criteria, stored_status) = self.connection().query_row(
            "SELECT name, criteria, status FROM sort_tasks WHERE id = ?1",
            [task_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )?;
        let status = SortTaskStatus::from_storage(&stored_status)
            .ok_or_else(|| AppError::InvalidProject("排序任务状态无效".into()))?;
        let items = self.load_comparison_items(task_id)?;
        let midpoint = state.midpoint();
        let left = state
            .queue
            .first()
            .and_then(|candidate| items.get(&candidate.group_id))
            .cloned();
        let right = midpoint
            .and_then(|index| state.sorted_group_ids.get(index))
            .and_then(|group_id| items.get(group_id))
            .cloned();
        let ordered_items = state
            .sorted_group_ids
            .iter()
            .filter_map(|group_id| items.get(group_id).cloned())
            .collect::<Vec<_>>();
        let group_sizes = items
            .iter()
            .map(|(group_id, item)| (group_id.as_str(), item.item_ids.len()))
            .collect::<HashMap<_, _>>();
        let located_count = state
            .sorted_group_ids
            .iter()
            .map(|id| group_sizes.get(id.as_str()).copied().unwrap_or(0))
            .sum::<usize>();
        let pending_count = state
            .queue
            .iter()
            .map(|candidate| {
                group_sizes
                    .get(candidate.group_id.as_str())
                    .copied()
                    .unwrap_or(0)
            })
            .sum::<usize>();
        let total_count = located_count + pending_count;
        let comparison_count = self.connection().query_row(
            "SELECT COUNT(*) FROM comparisons WHERE sort_task_id = ?1 AND is_undone = 0 AND source = 'user'",
            [task_id],
            |row| row.get::<_, usize>(0),
        )?;
        let estimated_remaining = estimate_remaining(&state);
        // 传递排序的路径会随判断动态变化，以已判断数加当前预计剩余数作为实时预计总数。
        // 这样界面中的“已判断 / 约总次数”和“预计剩余”可以彼此准确换算。
        let planned_comparison_count = comparison_count.saturating_add(estimated_remaining);
        Ok(ComparisonWorkspace {
            task_id: task_id.to_owned(),
            task_name,
            criteria,
            status,
            mode: SortTaskMode::Comparison,
            left,
            right,
            located_count,
            total_count,
            comparison_count,
            estimated_remaining,
            pending_count,
            progress_percent: located_count
                .saturating_mul(100)
                .checked_div(total_count)
                .unwrap_or(100),
            can_undo: comparison_count > 0,
            completed: state.queue.is_empty(),
            ordered_items,
            planned_comparison_count: Some(planned_comparison_count),
            matrix_comparison_percent: None,
            standings: Vec::new(),
        })
    }

    pub(super) fn load_comparison_items(
        &self,
        task_id: &str,
    ) -> Result<HashMap<String, ComparisonItem>, AppError> {
        let dataset_id = self.connection().query_row(
            "SELECT dataset_id FROM sort_tasks WHERE id = ?1",
            [task_id],
            |row| row.get::<_, String>(0),
        )?;
        let definitions = self.load_display_fields(&dataset_id)?;
        let primary_name = definitions
            .iter()
            .find(|(_, primary, _)| *primary)
            .map(|(name, _, _)| name.as_str())
            .ok_or_else(|| AppError::InvalidProject("数据集缺少主标识字段".into()))?;
        let mut statement = self.connection().prepare(
            "SELECT rg.id, rg.name, i.id, i.fields_json FROM rank_groups rg \
             JOIN rank_group_items rgi ON rgi.rank_group_id = rg.id \
             JOIN items i ON i.id = rgi.item_id \
             WHERE rg.sort_task_id = ?1 AND rg.is_active = 1 ORDER BY rg.position, rgi.item_order",
        )?;
        let rows = statement.query_map([task_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        let mut grouped: ComparisonGroupMembers = HashMap::new();
        for row in rows {
            let (group_id, group_name, item_id, fields_json) = row?;
            let fields: BTreeMap<String, Value> = serde_json::from_str(&fields_json)?;
            grouped
                .entry(group_id)
                .or_insert_with(|| (group_name, Vec::new()))
                .1
                .push((item_id, fields));
        }
        grouped
            .into_iter()
            .map(|(group_id, (group_name, members))| {
                let (_, representative_fields) = members
                    .first()
                    .ok_or_else(|| AppError::InvalidProject("活动排序组没有条目".into()))?;
                let all_fields = definitions
                    .iter()
                    .map(|(name, _, _)| SortDisplayField {
                        name: name.clone(),
                        value: representative_fields
                            .get(name)
                            .cloned()
                            .unwrap_or(Value::Null),
                    })
                    .collect();
                let auxiliary_fields = definitions
                    .iter()
                    .filter(|(_, _, auxiliary)| *auxiliary)
                    .map(|(name, _, _)| SortDisplayField {
                        name: name.clone(),
                        value: representative_fields
                            .get(name)
                            .cloned()
                            .unwrap_or(Value::Null),
                    })
                    .collect();
                let member_labels = members
                    .iter()
                    .map(|(_, fields)| display_value(fields.get(primary_name)))
                    .collect::<Vec<_>>();
                let item_ids = members
                    .iter()
                    .map(|(item_id, _)| item_id.clone())
                    .collect::<Vec<_>>();
                Ok((
                    group_id.clone(),
                    ComparisonItem {
                        group_id,
                        item_id: item_ids.first().cloned().unwrap_or_default(),
                        item_ids,
                        primary_label: group_name
                            .clone()
                            .unwrap_or_else(|| member_labels.join("、")),
                        member_labels,
                        group_name,
                        auxiliary_fields,
                        fields: all_fields,
                    },
                ))
            })
            .collect()
    }

    fn capture_group_topology(&self, task_id: &str) -> Result<GroupTopology, AppError> {
        capture_group_topology(self.connection(), task_id)
    }
}

pub(crate) fn capture_group_topology(
    connection: &rusqlite::Connection,
    task_id: &str,
) -> Result<GroupTopology, AppError> {
    let mut statement = connection.prepare(
        "SELECT id, position, name, is_active, merged_into_group_id FROM rank_groups WHERE sort_task_id = ?1 ORDER BY position",
    )?;
    let rows = statement.query_map([task_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, usize>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, bool>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    })?;
    let mut groups = Vec::new();
    for row in rows {
        let (id, position, name, is_active, merged_into_group_id) = row?;
        let mut item_statement = connection.prepare(
            "SELECT item_id FROM rank_group_items WHERE rank_group_id = ?1 ORDER BY item_order",
        )?;
        let item_rows = item_statement.query_map([&id], |row| row.get(0))?;
        groups.push(GroupTopologyEntry {
            id,
            position,
            name,
            is_active,
            merged_into_group_id,
            items: item_rows.collect::<Result<Vec<_>, _>>()?,
        });
    }
    Ok(GroupTopology { groups })
}

pub(crate) fn restore_group_topology(
    transaction: &Transaction<'_>,
    task_id: &str,
    topology: &GroupTopology,
) -> Result<(), AppError> {
    let snapshot_group_ids = topology
        .groups
        .iter()
        .map(|group| group.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let current_group_ids = {
        let mut statement =
            transaction.prepare("SELECT id FROM rank_groups WHERE sort_task_id = ?1")?;
        let rows = statement.query_map([task_id], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    transaction.execute(
        "DELETE FROM rank_group_items WHERE rank_group_id IN (SELECT id FROM rank_groups WHERE sort_task_id = ?1)",
        [task_id],
    )?;
    for group_id in current_group_ids {
        if !snapshot_group_ids.contains(group_id.as_str()) {
            transaction.execute(
                "UPDATE rank_groups SET is_active = 0, merged_into_group_id = NULL, name = NULL WHERE id = ?1 AND sort_task_id = ?2",
                params![group_id, task_id],
            )?;
        }
    }
    let offset = transaction.query_row(
        "SELECT COALESCE(MAX(position), -1) + ?1 + 1 FROM rank_groups WHERE sort_task_id = ?2",
        params![topology.groups.len(), task_id],
        |row| row.get::<_, usize>(0),
    )?;
    transaction.execute(
        "UPDATE rank_groups SET position = position + ?1 WHERE sort_task_id = ?2",
        params![offset, task_id],
    )?;
    for group in &topology.groups {
        transaction.execute(
            "UPDATE rank_groups SET position = ?1, name = ?2, is_active = ?3, merged_into_group_id = ?4 WHERE id = ?5 AND sort_task_id = ?6",
            params![group.position, group.name, group.is_active, group.merged_into_group_id, group.id, task_id],
        )?;
        for (item_order, item_id) in group.items.iter().enumerate() {
            transaction.execute(
                "INSERT INTO rank_group_items (rank_group_id, item_id, item_order) VALUES (?1, ?2, ?3)",
                params![group.id, item_id, item_order],
            )?;
        }
    }
    Ok(())
}

fn upsert_session(
    transaction: &Transaction<'_>,
    task_id: &str,
    state: &ComparisonSessionState,
    now: &str,
) -> Result<(), AppError> {
    transaction.execute(
        "INSERT INTO comparison_sessions (sort_task_id, state_json, updated_at) VALUES (?1, ?2, ?3) \
         ON CONFLICT(sort_task_id) DO UPDATE SET state_json = excluded.state_json, updated_at = excluded.updated_at",
        params![task_id, serde_json::to_string(state)?, now],
    )?;
    Ok(())
}

fn adjust_deferred_ranges(state: &mut ComparisonSessionState, insertion_position: usize) {
    // 插入已定位数组后，尚未处理候选的边界索引也要同步右移。
    for candidate in &mut state.queue {
        let (Some(low), Some(high)) = (candidate.low, candidate.high) else {
            continue;
        };
        if insertion_position <= low {
            candidate.low = Some(low + 1);
            candidate.high = Some(high + 1);
        } else if insertion_position < high {
            candidate.high = Some(high + 1);
        }
    }
}

fn estimate_remaining(state: &ComparisonSessionState) -> usize {
    // 对每个待定位组估算其当前搜索区间所需的二分层数，再求和得到动态剩余次数。
    state
        .queue
        .iter()
        .enumerate()
        .map(|(index, candidate)| match (candidate.low, candidate.high) {
            (Some(low), Some(high)) => ceil_log2(high.saturating_sub(low) + 1),
            _ => ceil_log2(state.sorted_group_ids.len() + index + 1),
        })
        .sum()
}

fn ceil_log2(value: usize) -> usize {
    if value <= 1 {
        0
    } else {
        usize::BITS as usize - (value - 1).leading_zeros() as usize
    }
}
