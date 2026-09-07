//! 1v1 矩阵排序会话。
//! 首次打开任务时生成并固化待比较的无序对象对；完成后按胜场降序持久化，
//! 胜场相同的对象自动合并为并列组。

use chrono::Utc;
use rusqlite::{params, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    domain::{
        comparison::{ComparisonDecision, ComparisonWorkspace, MatrixStanding},
        matrix::{MatrixPair, MatrixSessionState},
        sort_task::{SortTaskMode, SortTaskStatus},
    },
    error::AppError,
    repository::{
        comparison_repository::{capture_group_topology, restore_group_topology, GroupTopology},
        group_repository::merge_groups_in_transaction,
        project_repository::ProjectRepository,
        sort_task_repository::persist_group_order,
    },
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MatrixComparisonHistory {
    current_index: usize,
    pair: MatrixPair,
}

impl ProjectRepository {
    pub fn load_matrix_workspace(
        &mut self,
        task_id: &str,
    ) -> Result<ComparisonWorkspace, AppError> {
        self.ensure_matrix_task(task_id)?;
        let state = self.ensure_matrix_session(task_id)?;
        self.build_matrix_workspace(task_id, state)
    }

    pub fn answer_matrix_comparison(
        &mut self,
        task_id: &str,
        decision: ComparisonDecision,
    ) -> Result<ComparisonWorkspace, AppError> {
        self.ensure_matrix_task(task_id)?;
        self.ensure_matrix_editable(task_id)?;
        let mut state = self.ensure_matrix_session(task_id)?;
        let pair = state
            .current_pair()
            .cloned()
            .ok_or_else(|| AppError::InvalidInput("矩阵排序已经完成".into()))?;
        let history_json = serde_json::to_string(&MatrixComparisonHistory {
            current_index: state.current_index,
            pair: pair.clone(),
        })?;
        match decision {
            ComparisonDecision::LeftBefore => {
                state.contestants[pair.left].wins += 1;
                state.contestants[pair.right].losses += 1;
            }
            ComparisonDecision::RightBefore => {
                state.contestants[pair.right].wins += 1;
                state.contestants[pair.left].losses += 1;
            }
            ComparisonDecision::Tie => {
                state.contestants[pair.left].ties += 1;
                state.contestants[pair.right].ties += 1;
            }
        }
        state.current_index += 1;
        let completed = state.completed();
        let topology_before = completed
            .then(|| capture_group_topology(self.connection(), task_id))
            .transpose()?;
        let now = Utc::now().to_rfc3339();
        let left_group_id = state.contestants[pair.left].item.group_id.clone();
        let right_group_id = state.contestants[pair.right].item.group_id.clone();
        let transaction = self.connection_mut().transaction()?;
        transaction.execute(
            "INSERT INTO comparisons (id, sort_task_id, left_group_id, right_group_id, result, source, is_undone, created_at, session_state_before_json, session_state_after_json, group_state_before_json, group_state_after_json) \
             VALUES (?1, ?2, ?3, ?4, ?5, 'user', 0, ?6, ?7, ?8, ?9, NULL)",
            params![
                Uuid::new_v4().to_string(),
                task_id,
                left_group_id,
                right_group_id,
                decision.as_str(),
                now,
                history_json,
                "{}",
                topology_before
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?,
            ],
        )?;
        if completed {
            finalize_matrix(&transaction, task_id, &state)?;
        }
        upsert_matrix_session(&transaction, task_id, &state, &now)?;
        transaction.execute(
            "UPDATE sort_tasks SET status = 'sorting', updated_at = ?1 WHERE id = ?2",
            params![now, task_id],
        )?;
        transaction.commit()?;
        self.build_matrix_workspace(task_id, state)
    }

    pub fn skip_matrix_comparison(
        &mut self,
        task_id: &str,
    ) -> Result<ComparisonWorkspace, AppError> {
        self.ensure_matrix_task(task_id)?;
        self.ensure_matrix_editable(task_id)?;
        let mut state = self.ensure_matrix_session(task_id)?;
        if state.current_index + 1 < state.pairs.len() {
            let pair = state.pairs.remove(state.current_index);
            state.pairs.push(pair);
            self.save_matrix_session(task_id, &state)?;
        }
        self.build_matrix_workspace(task_id, state)
    }

    pub fn undo_matrix_comparison(
        &mut self,
        task_id: &str,
    ) -> Result<ComparisonWorkspace, AppError> {
        self.ensure_matrix_task(task_id)?;
        self.ensure_matrix_editable(task_id)?;
        let comparison = self
            .connection()
            .query_row(
                "SELECT id, result, session_state_before_json, group_state_before_json FROM comparisons \
                 WHERE sort_task_id = ?1 AND is_undone = 0 AND source = 'user' \
                 ORDER BY created_at DESC, rowid DESC LIMIT 1",
                [task_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                },
            )
            .optional()?;
        let Some((comparison_id, result, history_json, topology_before_json)) = comparison else {
            return self.load_matrix_workspace(task_id);
        };
        let history: MatrixComparisonHistory = serde_json::from_str(&history_json)?;
        let state_json = self.connection().query_row(
            "SELECT state_json FROM matrix_sessions WHERE sort_task_id = ?1",
            [task_id],
            |row| row.get::<_, String>(0),
        )?;
        let mut state: MatrixSessionState = serde_json::from_str(&state_json)?;
        if state.current_index != history.current_index + 1
            || state.pairs.get(history.current_index) != Some(&history.pair)
        {
            return Err(AppError::InvalidProject(
                "矩阵排序撤销记录与当前会话不一致".into(),
            ));
        }
        match result.as_str() {
            "left_before" => {
                state.contestants[history.pair.left].wins = state.contestants[history.pair.left]
                    .wins
                    .checked_sub(1)
                    .ok_or_else(|| AppError::InvalidProject("矩阵胜场记录无效".into()))?;
                state.contestants[history.pair.right].losses = state.contestants
                    [history.pair.right]
                    .losses
                    .checked_sub(1)
                    .ok_or_else(|| AppError::InvalidProject("矩阵负场记录无效".into()))?;
            }
            "right_before" => {
                state.contestants[history.pair.right].wins = state.contestants[history.pair.right]
                    .wins
                    .checked_sub(1)
                    .ok_or_else(|| AppError::InvalidProject("矩阵胜场记录无效".into()))?;
                state.contestants[history.pair.left].losses = state.contestants[history.pair.left]
                    .losses
                    .checked_sub(1)
                    .ok_or_else(|| AppError::InvalidProject("矩阵负场记录无效".into()))?;
            }
            "tie" => {
                state.contestants[history.pair.left].ties = state.contestants[history.pair.left]
                    .ties
                    .checked_sub(1)
                    .ok_or_else(|| AppError::InvalidProject("矩阵平局记录无效".into()))?;
                state.contestants[history.pair.right].ties = state.contestants[history.pair.right]
                    .ties
                    .checked_sub(1)
                    .ok_or_else(|| AppError::InvalidProject("矩阵平局记录无效".into()))?;
            }
            _ => return Err(AppError::InvalidProject("矩阵比较结果无效".into())),
        }
        state.current_index = history.current_index;
        let now = Utc::now().to_rfc3339();
        let transaction = self.connection_mut().transaction()?;
        if let Some(topology_json) = topology_before_json {
            let topology: GroupTopology = serde_json::from_str(&topology_json)?;
            restore_group_topology(&transaction, task_id, &topology)?;
        }
        transaction.execute(
            "UPDATE comparisons SET is_undone = 1 WHERE id = ?1",
            [comparison_id],
        )?;
        upsert_matrix_session(&transaction, task_id, &state, &now)?;
        transaction.execute(
            "UPDATE sort_tasks SET status = 'sorting', updated_at = ?1 WHERE id = ?2",
            params![now, task_id],
        )?;
        transaction.commit()?;
        self.build_matrix_workspace(task_id, state)
    }

    fn ensure_matrix_session(&mut self, task_id: &str) -> Result<MatrixSessionState, AppError> {
        let saved = self
            .connection()
            .query_row(
                "SELECT state_json FROM matrix_sessions WHERE sort_task_id = ?1",
                [task_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if let Some(json) = saved {
            return Ok(serde_json::from_str(&json)?);
        }
        let group_ids = self.ordered_group_ids(task_id)?;
        let items = self.load_comparison_items(task_id)?;
        let ordered_items = group_ids
            .into_iter()
            .filter_map(|group_id| items.get(&group_id).cloned())
            .collect::<Vec<_>>();
        let percent = self.connection().query_row(
            "SELECT COALESCE(matrix_comparison_percent, 100) FROM sort_tasks WHERE id = ?1",
            [task_id],
            |row| row.get::<_, u8>(0),
        )?;
        let state = MatrixSessionState::initialize(ordered_items, percent);
        self.save_matrix_session(task_id, &state)?;
        self.connection().execute(
            "UPDATE sort_tasks SET status = 'sorting', updated_at = ?1 WHERE id = ?2 AND status = 'draft'",
            params![Utc::now().to_rfc3339(), task_id],
        )?;
        Ok(state)
    }

    fn save_matrix_session(
        &mut self,
        task_id: &str,
        state: &MatrixSessionState,
    ) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        let transaction = self.connection_mut().transaction()?;
        upsert_matrix_session(&transaction, task_id, state, &now)?;
        transaction.commit()?;
        Ok(())
    }

    fn build_matrix_workspace(
        &self,
        task_id: &str,
        state: MatrixSessionState,
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
        let current = state.current_pair();
        let left = current.map(|pair| state.contestants[pair.left].item.clone());
        let right = current.map(|pair| state.contestants[pair.right].item.clone());
        let mut order = (0..state.contestants.len()).collect::<Vec<_>>();
        order.sort_by(|left, right| {
            state.contestants[*right]
                .wins
                .cmp(&state.contestants[*left].wins)
                .then_with(|| left.cmp(right))
        });
        let standings = order
            .iter()
            .map(|index| {
                let contestant = &state.contestants[*index];
                MatrixStanding {
                    item: contestant.item.clone(),
                    wins: contestant.wins,
                    losses: contestant.losses,
                    ties: contestant.ties,
                    total: contestant.wins + contestant.losses + contestant.ties,
                }
            })
            .collect::<Vec<_>>();
        let ordered_items = standings
            .iter()
            .map(|standing| standing.item.clone())
            .collect();
        let total_pairs = state.pairs.len();
        let comparison_count = state.current_index.min(total_pairs);
        let remaining = total_pairs.saturating_sub(comparison_count);
        let completed = state.completed();
        Ok(ComparisonWorkspace {
            task_id: task_id.to_owned(),
            task_name,
            criteria,
            status,
            mode: SortTaskMode::Matrix,
            left,
            right,
            located_count: if completed {
                state.contestants.len()
            } else {
                0
            },
            total_count: state.contestants.len(),
            comparison_count,
            estimated_remaining: remaining,
            pending_count: remaining,
            progress_percent: comparison_count
                .saturating_mul(100)
                .checked_div(total_pairs)
                .unwrap_or(100),
            can_undo: comparison_count > 0,
            completed,
            ordered_items,
            planned_comparison_count: Some(total_pairs),
            matrix_comparison_percent: Some(state.comparison_percent),
            standings,
        })
    }

    fn ensure_matrix_task(&self, task_id: &str) -> Result<(), AppError> {
        if self.sort_task_mode(task_id)? != SortTaskMode::Matrix {
            return Err(AppError::InvalidInput("该任务不是 1v1 矩阵排序任务".into()));
        }
        Ok(())
    }

    fn ensure_matrix_editable(&self, task_id: &str) -> Result<(), AppError> {
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
}

fn finalize_matrix(
    transaction: &Transaction<'_>,
    task_id: &str,
    state: &MatrixSessionState,
) -> Result<(), AppError> {
    let mut order = (0..state.contestants.len()).collect::<Vec<_>>();
    order.sort_by(|left, right| {
        state.contestants[*right]
            .wins
            .cmp(&state.contestants[*left].wins)
            .then_with(|| left.cmp(right))
    });
    let mut active_order = Vec::new();
    let mut cursor = 0;
    while cursor < order.len() {
        let wins = state.contestants[order[cursor]].wins;
        let target = state.contestants[order[cursor]].item.group_id.clone();
        active_order.push(target.clone());
        cursor += 1;
        while cursor < order.len() && state.contestants[order[cursor]].wins == wins {
            let source = &state.contestants[order[cursor]].item.group_id;
            merge_groups_in_transaction(transaction, task_id, source, &target)?;
            cursor += 1;
        }
    }
    persist_group_order(transaction, task_id, &active_order)
}

fn upsert_matrix_session(
    transaction: &Transaction<'_>,
    task_id: &str,
    state: &MatrixSessionState,
    now: &str,
) -> Result<(), AppError> {
    transaction.execute(
        "INSERT INTO matrix_sessions (sort_task_id, state_json, updated_at) VALUES (?1, ?2, ?3) \
         ON CONFLICT(sort_task_id) DO UPDATE SET state_json = excluded.state_json, updated_at = excluded.updated_at",
        params![task_id, serde_json::to_string(state)?, now],
    )?;
    Ok(())
}
