//! 排名组的结构编辑与混合排序冲突处理。
//! 合并/拆分会同时影响组拓扑、比较会话和撤销历史，因此统一在事务内维护。

use std::collections::{HashMap, HashSet};

use chrono::Utc;
use rusqlite::{params, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    domain::{
        comparison::ComparisonSessionState,
        sort_task::{DragWorkspace, MoveConflictPreview, MoveConflictResolution},
    },
    error::AppError,
    repository::{
        comparison_repository::{capture_group_topology, restore_group_topology, GroupTopology},
        project_repository::ProjectRepository,
        sort_task_repository::persist_group_order,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StructuralHistoryPayload {
    // 结构操作不能只保存顺序：撤销时还要恢复组成员、会话游标和比较有效状态。
    topology: GroupTopology,
    session_state_json: Option<String>,
    comparisons: Vec<ComparisonHistoryState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ComparisonHistoryState {
    id: String,
    is_undone: bool,
}

impl ProjectRepository {
    pub fn merge_rank_groups(
        &mut self,
        task_id: &str,
        source_group_id: &str,
        target_group_id: &str,
    ) -> Result<DragWorkspace, AppError> {
        self.ensure_editable_active_groups(task_id, &[source_group_id, target_group_id])?;
        if source_group_id == target_group_id {
            return Err(AppError::InvalidInput("不能将排序组与自身合并".into()));
        }
        let history_before = capture_structural_history(self.connection(), task_id)?;
        let mut active_order = self.ordered_group_ids(task_id)?;
        active_order.retain(|group_id| group_id != source_group_id);
        let now = Utc::now().to_rfc3339();
        let transaction = self.connection_mut().transaction()?;
        merge_groups_in_transaction(&transaction, task_id, source_group_id, target_group_id)?;
        persist_group_order(&transaction, task_id, &active_order)?;
        update_session_after_merge(
            &transaction,
            task_id,
            source_group_id,
            target_group_id,
            &now,
        )?;
        let history_after = capture_structural_history(&transaction, task_id)?;
        insert_structural_history(
            &transaction,
            task_id,
            "merge_rank_groups",
            &history_before,
            &history_after,
            &now,
        )?;
        transaction.execute(
            "UPDATE sort_tasks SET status = 'sorting', updated_at = ?1 WHERE id = ?2",
            params![now, task_id],
        )?;
        transaction.commit()?;
        self.load_drag_workspace(task_id)
    }

    pub fn split_rank_group_item(
        &mut self,
        task_id: &str,
        group_id: &str,
        item_id: &str,
    ) -> Result<DragWorkspace, AppError> {
        self.ensure_editable_active_groups(task_id, &[group_id])?;
        let item_count = self.connection().query_row(
            "SELECT COUNT(*) FROM rank_group_items WHERE rank_group_id = ?1",
            [group_id],
            |row| row.get::<_, usize>(0),
        )?;
        if item_count < 2 {
            return Err(AppError::InvalidInput(
                "该排序组没有可拆分的并列条目".into(),
            ));
        }
        let contains_item = self.connection().query_row(
            "SELECT EXISTS(SELECT 1 FROM rank_group_items WHERE rank_group_id = ?1 AND item_id = ?2)",
            params![group_id, item_id],
            |row| row.get::<_, bool>(0),
        )?;
        if !contains_item {
            return Err(AppError::InvalidInput("并列组中找不到要拆分的条目".into()));
        }
        let history_before = capture_structural_history(self.connection(), task_id)?;
        let mut active_order = self.ordered_group_ids(task_id)?;
        let group_position = active_order
            .iter()
            .position(|id| id == group_id)
            .ok_or_else(|| AppError::InvalidInput("找不到并列组".into()))?;
        let new_group_id = Uuid::new_v4().to_string();
        active_order.insert(group_position + 1, new_group_id.clone());
        let now = Utc::now().to_rfc3339();
        let transaction = self.connection_mut().transaction()?;
        let temporary_position = transaction.query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM rank_groups WHERE sort_task_id = ?1",
            [task_id],
            |row| row.get::<_, usize>(0),
        )?;
        let initial_position = transaction.query_row(
            "SELECT original_index FROM items WHERE id = ?1",
            [item_id],
            |row| row.get::<_, usize>(0),
        )?;
        transaction.execute(
            "INSERT INTO rank_groups (id, sort_task_id, position, initial_position, name, created_at, is_active, merged_into_group_id) VALUES (?1, ?2, ?3, ?4, NULL, ?5, 1, NULL)",
            params![new_group_id, task_id, temporary_position, initial_position, now],
        )?;
        transaction.execute(
            "UPDATE rank_group_items SET rank_group_id = ?1, item_order = 0 WHERE rank_group_id = ?2 AND item_id = ?3",
            params![new_group_id, group_id, item_id],
        )?;
        normalize_item_order(&transaction, group_id)?;
        persist_group_order(&transaction, task_id, &active_order)?;
        update_completed_session_order(&transaction, task_id, &active_order, &now)?;
        mark_group_relations_undone(&transaction, task_id, group_id)?;
        let history_after = capture_structural_history(&transaction, task_id)?;
        insert_structural_history(
            &transaction,
            task_id,
            "split_rank_group_item",
            &history_before,
            &history_after,
            &now,
        )?;
        transaction.execute(
            "UPDATE sort_tasks SET status = 'sorting', updated_at = ?1 WHERE id = ?2",
            params![now, task_id],
        )?;
        transaction.commit()?;
        self.load_drag_workspace(task_id)
    }

    pub fn rename_rank_group(
        &mut self,
        task_id: &str,
        group_id: &str,
        name: &str,
    ) -> Result<DragWorkspace, AppError> {
        self.rename_rank_group_name(task_id, group_id, name)?;
        self.load_drag_workspace(task_id)
    }

    pub fn rename_rank_group_name(
        &mut self,
        task_id: &str,
        group_id: &str,
        name: &str,
    ) -> Result<(), AppError> {
        self.ensure_editable_active_groups(task_id, &[group_id])?;
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 60 {
            return Err(AppError::InvalidInput("并列组名称应为 1—60 个字符".into()));
        }
        self.connection().execute(
            "UPDATE rank_groups SET name = ?1 WHERE id = ?2 AND sort_task_id = ?3 AND is_active = 1",
            params![name, group_id, task_id],
        )?;
        self.connection().execute(
            "UPDATE sort_tasks SET updated_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), task_id],
        )?;
        Ok(())
    }

    pub fn preview_rank_group_move(
        &self,
        task_id: &str,
        group_id: &str,
        to_position: usize,
    ) -> Result<MoveConflictPreview, AppError> {
        let mut order = self.ordered_group_ids(task_id)?;
        let from = order
            .iter()
            .position(|id| id == group_id)
            .ok_or_else(|| AppError::InvalidInput("找不到要移动的排序组".into()))?;
        if to_position >= order.len() {
            return Err(AppError::InvalidInput("目标位置超出排序组范围".into()));
        }
        let moved = order.remove(from);
        order.insert(to_position, moved);
        // 先在内存构造候选顺序，仅报告会被反转的有效直接判断，不修改数据库。
        let conflicts = self.conflicting_comparisons(task_id, &order)?;
        Ok(MoveConflictPreview {
            conflict_count: conflicts.len(),
            descriptions: conflicts
                .iter()
                .take(5)
                .map(|(_, description)| description.clone())
                .collect(),
        })
    }

    pub fn resolve_move_conflicts(
        &mut self,
        task_id: &str,
        group_id: &str,
        to_position: usize,
        resolution: MoveConflictResolution,
    ) -> Result<(), AppError> {
        // 临时移动保留原判断；“以拖拽为准”则把冲突判断标记为已撤销。
        if resolution == MoveConflictResolution::Temporary {
            return Ok(());
        }
        let mut order = self.ordered_group_ids(task_id)?;
        let from = order
            .iter()
            .position(|id| id == group_id)
            .ok_or_else(|| AppError::InvalidInput("找不到要移动的排序组".into()))?;
        let moved = order.remove(from);
        order.insert(to_position, moved);
        let conflicts = self.conflicting_comparisons(task_id, &order)?;
        let transaction = self.connection_mut().transaction()?;
        for (comparison_id, _) in conflicts {
            transaction.execute(
                "UPDATE comparisons SET is_undone = 1 WHERE id = ?1",
                [comparison_id],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    fn ensure_editable_active_groups(
        &self,
        task_id: &str,
        group_ids: &[&str],
    ) -> Result<(), AppError> {
        let status = self
            .connection()
            .query_row(
                "SELECT status FROM sort_tasks WHERE id = ?1",
                [task_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| AppError::InvalidInput("找不到排序任务".into()))?;
        if status == "confirmed" {
            return Err(AppError::InvalidInput(
                "结果已确认并锁定，请先解锁后再编辑".into(),
            ));
        }
        for group_id in group_ids {
            let valid = self.connection().query_row(
                "SELECT EXISTS(SELECT 1 FROM rank_groups WHERE id = ?1 AND sort_task_id = ?2 AND is_active = 1)",
                params![group_id, task_id],
                |row| row.get::<_, bool>(0),
            )?;
            if !valid {
                return Err(AppError::InvalidInput("找不到有效的排序组".into()));
            }
        }
        Ok(())
    }

    fn conflicting_comparisons(
        &self,
        task_id: &str,
        proposed_order: &[String],
    ) -> Result<Vec<(String, String)>, AppError> {
        let positions = proposed_order
            .iter()
            .enumerate()
            .map(|(position, id)| (id.as_str(), position))
            .collect::<HashMap<_, _>>();
        let labels = self
            .load_drag_workspace(task_id)?
            .groups
            .into_iter()
            .map(|group| {
                let label = group.name.unwrap_or_else(|| {
                    group
                        .items
                        .iter()
                        .map(|item| item.primary_label.as_str())
                        .collect::<Vec<_>>()
                        .join("、")
                });
                (group.group_id, label)
            })
            .collect::<HashMap<_, _>>();
        let mut statement = self.connection().prepare(
            "SELECT id, left_group_id, right_group_id, result FROM comparisons WHERE sort_task_id = ?1 AND is_undone = 0",
        )?;
        let rows = statement.query_map([task_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        let mut conflicts = Vec::new();
        for row in rows {
            let (id, left, right, result) = row?;
            let left = self.resolve_active_group_id(&left)?;
            let right = self.resolve_active_group_id(&right)?;
            if left == right {
                continue;
            }
            let (Some(left_position), Some(right_position)) =
                (positions.get(left.as_str()), positions.get(right.as_str()))
            else {
                continue;
            };
            let valid = match result.as_str() {
                "left_before" => left_position < right_position,
                "right_before" => right_position < left_position,
                "tie" => false,
                _ => true,
            };
            if !valid {
                let left_label = labels.get(&left).map(String::as_str).unwrap_or("左侧组");
                let right_label = labels.get(&right).map(String::as_str).unwrap_or("右侧组");
                conflicts.push((id, format!("{left_label} ↔ {right_label}")));
            }
        }
        Ok(conflicts)
    }

    pub(super) fn resolve_active_group_id(&self, group_id: &str) -> Result<String, AppError> {
        let mut current = group_id.to_owned();
        for _ in 0..100 {
            let next = self
                .connection()
                .query_row(
                    "SELECT merged_into_group_id FROM rank_groups WHERE id = ?1",
                    [&current],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()?
                .flatten();
            let Some(next) = next else {
                return Ok(current);
            };
            current = next;
        }
        Err(AppError::InvalidProject(
            "并列组归并关系形成异常循环".into(),
        ))
    }
}

pub(super) fn merge_groups_in_transaction(
    transaction: &Transaction<'_>,
    task_id: &str,
    source_group_id: &str,
    target_group_id: &str,
) -> Result<(), AppError> {
    let target_count = transaction.query_row(
        "SELECT COUNT(*) FROM rank_group_items WHERE rank_group_id = ?1",
        [target_group_id],
        |row| row.get::<_, usize>(0),
    )?;
    let source_items = {
        let mut statement = transaction.prepare(
            "SELECT item_id FROM rank_group_items WHERE rank_group_id = ?1 ORDER BY item_order",
        )?;
        let rows = statement.query_map([source_group_id], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    for (offset, item_id) in source_items.iter().enumerate() {
        transaction.execute(
            "UPDATE rank_group_items SET rank_group_id = ?1, item_order = ?2 WHERE rank_group_id = ?3 AND item_id = ?4",
            params![target_group_id, target_count + offset, source_group_id, item_id],
        )?;
    }
    let inactive_position = transaction.query_row(
        "SELECT COALESCE(MAX(position), -1) + 1 FROM rank_groups WHERE sort_task_id = ?1",
        [task_id],
        |row| row.get::<_, usize>(0),
    )?;
    transaction.execute(
        "UPDATE rank_groups SET is_active = 0, merged_into_group_id = ?1, position = ?2 WHERE id = ?3 AND sort_task_id = ?4",
        params![target_group_id, inactive_position, source_group_id, task_id],
    )?;
    let existing_name = transaction.query_row(
        "SELECT name FROM rank_groups WHERE id = ?1",
        [target_group_id],
        |row| row.get::<_, Option<String>>(0),
    )?;
    if existing_name.is_none() {
        let tie_count = transaction.query_row(
            "SELECT COUNT(*) FROM rank_groups rg WHERE rg.sort_task_id = ?1 AND rg.is_active = 1 AND rg.id <> ?2 AND (SELECT COUNT(*) FROM rank_group_items WHERE rank_group_id = rg.id) > 1",
            params![task_id, target_group_id],
            |row| row.get::<_, usize>(0),
        )?;
        transaction.execute(
            "UPDATE rank_groups SET name = ?1 WHERE id = ?2",
            params![format!("并列组{}", tie_count + 1), target_group_id],
        )?;
    }
    Ok(())
}

fn update_session_after_merge(
    transaction: &Transaction<'_>,
    task_id: &str,
    source_group_id: &str,
    target_group_id: &str,
    now: &str,
) -> Result<(), AppError> {
    let state_json = transaction
        .query_row(
            "SELECT state_json FROM comparison_sessions WHERE sort_task_id = ?1",
            [task_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(state_json) = state_json else {
        return Ok(());
    };
    let mut state: ComparisonSessionState = serde_json::from_str(&state_json)?;
    for group_id in &mut state.sorted_group_ids {
        if group_id == source_group_id {
            *group_id = target_group_id.to_owned();
        }
    }
    let mut seen = HashSet::new();
    state.sorted_group_ids.retain(|id| seen.insert(id.clone()));
    state
        .queue
        .retain(|candidate| candidate.group_id != source_group_id);
    transaction.execute(
        "UPDATE comparison_sessions SET state_json = ?1, updated_at = ?2 WHERE sort_task_id = ?3",
        params![serde_json::to_string(&state)?, now, task_id],
    )?;
    Ok(())
}

fn mark_group_relations_undone(
    transaction: &Transaction<'_>,
    task_id: &str,
    group_id: &str,
) -> Result<(), AppError> {
    transaction.execute(
        "UPDATE comparisons SET is_undone = 1 WHERE sort_task_id = ?1 AND (left_group_id = ?2 OR right_group_id = ?2)",
        params![task_id, group_id],
    )?;
    Ok(())
}

fn normalize_item_order(transaction: &Transaction<'_>, group_id: &str) -> Result<(), AppError> {
    let item_ids = {
        let mut statement = transaction.prepare(
            "SELECT item_id FROM rank_group_items WHERE rank_group_id = ?1 ORDER BY item_order",
        )?;
        let rows = statement.query_map([group_id], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    for (order, item_id) in item_ids.iter().enumerate() {
        transaction.execute(
            "UPDATE rank_group_items SET item_order = ?1 WHERE rank_group_id = ?2 AND item_id = ?3",
            params![order, group_id, item_id],
        )?;
    }
    Ok(())
}

fn capture_structural_history(
    connection: &rusqlite::Connection,
    task_id: &str,
) -> Result<StructuralHistoryPayload, AppError> {
    let topology = capture_group_topology(connection, task_id)?;
    let session_state_json = connection
        .query_row(
            "SELECT state_json FROM comparison_sessions WHERE sort_task_id = ?1",
            [task_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let comparisons = {
        let mut statement = connection.prepare(
            "SELECT id, is_undone FROM comparisons WHERE sort_task_id = ?1 ORDER BY created_at, rowid",
        )?;
        let rows = statement.query_map([task_id], |row| {
            Ok(ComparisonHistoryState {
                id: row.get(0)?,
                is_undone: row.get(1)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    Ok(StructuralHistoryPayload {
        topology,
        session_state_json,
        comparisons,
    })
}

fn insert_structural_history(
    transaction: &Transaction<'_>,
    task_id: &str,
    operation_type: &str,
    before: &StructuralHistoryPayload,
    after: &StructuralHistoryPayload,
    now: &str,
) -> Result<(), AppError> {
    transaction.execute(
        "DELETE FROM operation_logs WHERE sort_task_id = ?1 AND is_undone = 1",
        [task_id],
    )?;
    let sequence = transaction.query_row(
        "SELECT COALESCE(MAX(sequence), 0) + 1 FROM operation_logs WHERE sort_task_id = ?1",
        [task_id],
        |row| row.get::<_, i64>(0),
    )?;
    transaction.execute(
        "INSERT INTO operation_logs (id, sort_task_id, operation_type, forward_payload_json, inverse_payload_json, sequence, is_undone, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7)",
        params![Uuid::new_v4().to_string(), task_id, operation_type, serde_json::to_string(after)?, serde_json::to_string(before)?, sequence, now],
    )?;
    Ok(())
}

pub(crate) fn restore_structural_history(
    transaction: &Transaction<'_>,
    task_id: &str,
    payload: &StructuralHistoryPayload,
    now: &str,
) -> Result<(), AppError> {
    restore_group_topology(transaction, task_id, &payload.topology)?;
    if let Some(state_json) = &payload.session_state_json {
        transaction.execute(
            "INSERT INTO comparison_sessions (sort_task_id, state_json, updated_at) VALUES (?1, ?2, ?3) ON CONFLICT(sort_task_id) DO UPDATE SET state_json = excluded.state_json, updated_at = excluded.updated_at",
            params![task_id, state_json, now],
        )?;
    } else {
        transaction.execute(
            "DELETE FROM comparison_sessions WHERE sort_task_id = ?1",
            [task_id],
        )?;
    }
    for comparison in &payload.comparisons {
        transaction.execute(
            "UPDATE comparisons SET is_undone = ?1 WHERE id = ?2 AND sort_task_id = ?3",
            params![comparison.is_undone, comparison.id, task_id],
        )?;
    }
    transaction.execute(
        "UPDATE sort_tasks SET status = 'sorting', updated_at = ?1 WHERE id = ?2",
        params![now, task_id],
    )?;
    Ok(())
}

fn update_completed_session_order(
    transaction: &Transaction<'_>,
    task_id: &str,
    active_order: &[String],
    now: &str,
) -> Result<(), AppError> {
    let state_json = transaction
        .query_row(
            "SELECT state_json FROM comparison_sessions WHERE sort_task_id = ?1",
            [task_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(state_json) = state_json else {
        return Ok(());
    };
    let mut state: ComparisonSessionState = serde_json::from_str(&state_json)?;
    if state.queue.is_empty() {
        state.sorted_group_ids = active_order.to_vec();
        transaction.execute(
            "UPDATE comparison_sessions SET state_json = ?1, updated_at = ?2 WHERE sort_task_id = ?3",
            params![serde_json::to_string(&state)?, now, task_id],
        )?;
    }
    Ok(())
}
