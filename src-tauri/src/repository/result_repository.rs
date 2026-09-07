//! 最终结果的完整性校验、锁定和排名字段写入。
//! 结果从活动排名组派生，任何写入前都重新读取并验证数据库状态。

use std::collections::{BTreeMap, HashSet};

use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    domain::{
        comparison::ComparisonSessionState,
        result::{ExportRow, ResultIntegrity, ResultItem, ResultPreview},
        scoring::RankRule,
        sort_task::{SortDisplayField, SortTaskMode, SortTaskStatus},
    },
    error::AppError,
    repository::{project_repository::ProjectRepository, sort_task_repository::display_value},
};

impl ProjectRepository {
    pub fn load_result_preview(&self, task_id: &str) -> Result<ResultPreview, AppError> {
        let (task_name, dataset_id, mode, stored_status, rank_written_at, rank_written_field) = self
            .connection()
            .query_row(
                "SELECT name, dataset_id, mode, status, rank_written_at, rank_written_field FROM sort_tasks WHERE id = ?1",
                [task_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| AppError::InvalidInput("找不到排序任务".into()))?;
        let mode = SortTaskMode::from_storage(&mode)
            .ok_or_else(|| AppError::InvalidProject("排序任务模式无效".into()))?;
        let status = SortTaskStatus::from_storage(&stored_status)
            .ok_or_else(|| AppError::InvalidProject("排序任务状态无效".into()))?;
        let display_fields = self.load_display_fields(&dataset_id)?;
        let primary_name = display_fields
            .iter()
            .find(|(_, primary, _)| *primary)
            .map(|(name, _, _)| name.as_str())
            .ok_or_else(|| AppError::InvalidProject("数据集缺少主标识字段".into()))?;

        let mut statement = self.connection().prepare(
            "SELECT rg.id, rg.name, i.id, \
                    1 + (SELECT COUNT(*) FROM rank_groups previous \
                         JOIN rank_group_items previous_items ON previous_items.rank_group_id = previous.id \
                         WHERE previous.sort_task_id = rg.sort_task_id AND previous.is_active = 1 AND previous.position < rg.position), \
                    i.original_index, i.fields_json, i.is_valid, \
                    (SELECT COUNT(*) FROM rank_group_items group_items WHERE group_items.rank_group_id = rg.id) \
             FROM rank_groups rg \
             JOIN rank_group_items rgi ON rgi.rank_group_id = rg.id \
             JOIN items i ON i.id = rgi.item_id \
             WHERE rg.sort_task_id = ?1 AND rg.is_active = 1 ORDER BY rg.position, rgi.item_order",
        )?;
        let rows = statement.query_map([task_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, usize>(3)?,
                row.get::<_, usize>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, bool>(6)?,
                row.get::<_, usize>(7)?,
            ))
        })?;
        let mut valid_flags = Vec::new();
        let items = rows
            .map(|row| {
                let (
                    group_id,
                    group_name,
                    item_id,
                    rank,
                    original_index,
                    fields_json,
                    is_valid,
                    tie_size,
                ) = row?;
                valid_flags.push(is_valid);
                let values: BTreeMap<String, Value> = serde_json::from_str(&fields_json)?;
                let original_rank = original_index + 1;
                Ok(ResultItem {
                    group_id,
                    group_name,
                    tie_size,
                    item_id,
                    rank,
                    original_rank,
                    rank_change: original_rank as i64 - rank as i64,
                    primary_label: display_value(values.get(primary_name)),
                    fields: display_fields
                        .iter()
                        .map(|(name, _, _)| SortDisplayField {
                            name: name.clone(),
                            value: values.get(name).cloned().unwrap_or(Value::Null),
                        })
                        .collect(),
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;

        let valid_dataset_count = self.connection().query_row(
            "SELECT COUNT(*) FROM items WHERE dataset_id = ?1 AND is_valid = 1",
            [&dataset_id],
            |row| row.get::<_, usize>(0),
        )?;
        let unique_item_count = items
            .iter()
            .map(|item| item.item_id.as_str())
            .collect::<HashSet<_>>()
            .len();
        let all_items_included = items.len() == valid_dataset_count;
        let no_duplicate_items = unique_item_count == items.len();
        let no_invalid_items = valid_flags.into_iter().all(|valid| valid);
        let no_unresolved_comparisons = match mode {
            SortTaskMode::Drag => true,
            SortTaskMode::Slider => self.slider_task_is_complete(task_id)?,
            SortTaskMode::Matrix => self.matrix_is_complete(task_id)?,
            SortTaskMode::Comparison => self.comparison_is_complete(task_id)?,
        };
        let mut problems = Vec::new();
        if !all_items_included {
            problems.push(format!(
                "结果包含 {} 个条目，但数据集有 {valid_dataset_count} 个有效条目",
                items.len()
            ));
        }
        if !no_unresolved_comparisons {
            problems.push("排序尚未完成，仍有待处理条目".into());
        }
        if !no_duplicate_items {
            problems.push("结果中存在重复条目".into());
        }
        if !no_invalid_items {
            problems.push("结果中包含无效条目".into());
        }
        let integrity = ResultIntegrity {
            all_items_included,
            no_unresolved_comparisons,
            no_duplicate_items,
            no_invalid_items,
            problems,
        };
        let confirmed_at = self
            .connection()
            .query_row(
                "SELECT created_at FROM snapshots WHERE sort_task_id = ?1 AND snapshot_type = 'sort_confirmation' ORDER BY created_at DESC, rowid DESC LIMIT 1",
                [task_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(ResultPreview {
            task_id: task_id.to_owned(),
            task_name,
            can_confirm: integrity.is_valid() && status != SortTaskStatus::Confirmed,
            status,
            items,
            integrity,
            confirmed_at,
            rank_written_at,
            rank_written_field,
        })
    }

    pub fn confirm_sort_result(&mut self, task_id: &str) -> Result<ResultPreview, AppError> {
        let preview = self.load_result_preview(task_id)?;
        if !preview.integrity.is_valid() {
            return Err(AppError::InvalidInput(format!(
                "结果完整性检查未通过：{}",
                preview.integrity.problems.join("；")
            )));
        }
        if preview.status == SortTaskStatus::Confirmed {
            return Ok(preview);
        }
        let now = Utc::now().to_rfc3339();
        let state = json!({
            "taskId": task_id,
            "groupIds": preview.items.iter().map(|item| item.group_id.as_str()).collect::<Vec<_>>(),
            "itemIds": preview.items.iter().map(|item| item.item_id.as_str()).collect::<Vec<_>>(),
            "confirmedAt": now,
        });
        // 确认快照与锁定状态同事务提交，避免出现已锁定但没有可追溯版本的状态。
        let transaction = self.connection_mut().transaction()?;
        transaction.execute(
            "INSERT INTO snapshots (id, sort_task_id, snapshot_type, state_json, created_at) VALUES (?1, ?2, 'sort_confirmation', ?3, ?4)",
            params![Uuid::new_v4().to_string(), task_id, serde_json::to_string(&state)?, now],
        )?;
        transaction.execute(
            "UPDATE sort_tasks SET status = 'confirmed', rank_written_at = NULL, rank_written_field = NULL, updated_at = ?1 WHERE id = ?2",
            params![now, task_id],
        )?;
        transaction.commit()?;
        self.load_result_preview(task_id)
    }

    pub fn unlock_sort_result(&mut self, task_id: &str) -> Result<ResultPreview, AppError> {
        let exists = self.connection().query_row(
            "SELECT EXISTS(SELECT 1 FROM sort_tasks WHERE id = ?1)",
            [task_id],
            |row| row.get::<_, bool>(0),
        )?;
        if !exists {
            return Err(AppError::InvalidInput("找不到排序任务".into()));
        }
        self.connection().execute(
            "UPDATE sort_tasks SET status = 'sorting', rank_written_at = NULL, rank_written_field = NULL, updated_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), task_id],
        )?;
        self.load_result_preview(task_id)
    }

    pub fn write_rank_field_with_rule(
        &mut self,
        task_id: &str,
        field_name: &str,
        overwrite: bool,
        rank_rule: RankRule,
    ) -> Result<ResultPreview, AppError> {
        let field_name = validate_field_name(field_name)?;
        let preview = self.load_result_preview(task_id)?;
        if preview.status != SortTaskStatus::Confirmed {
            return Err(AppError::InvalidInput(
                "请先确认排序结果，再写入排名字段".into(),
            ));
        }
        let dataset_id = self.connection().query_row(
            "SELECT dataset_id FROM sort_tasks WHERE id = ?1",
            [task_id],
            |row| row.get::<_, String>(0),
        )?;
        let existing_type = self
            .connection()
            .query_row(
                "SELECT field_type FROM field_definitions WHERE dataset_id = ?1 AND name = ?2",
                params![dataset_id, field_name],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if existing_type.is_some() && !overwrite {
            return Err(AppError::ConfirmationRequired(format!(
                "字段“{field_name}”已存在，写入排名会覆盖其当前值"
            )));
        }
        if existing_type
            .as_deref()
            .is_some_and(|kind| kind != "number")
        {
            return Err(AppError::InvalidInput(format!(
                "已有字段“{field_name}”不是数字类型，不能写入排名"
            )));
        }
        let assignments = self.rank_assignments(task_id, rank_rule)?;
        let transaction = self.connection_mut().transaction()?;
        if existing_type.is_none() {
            let display_order = transaction.query_row(
                "SELECT COALESCE(MAX(display_order), -1) + 1 FROM field_definitions WHERE dataset_id = ?1",
                [&dataset_id],
                |row| row.get::<_, usize>(0),
            )?;
            transaction.execute(
                "INSERT INTO field_definitions (id, dataset_id, name, field_type, display_order, is_primary_identifier, is_auxiliary_identifier) VALUES (?1, ?2, ?3, 'number', ?4, 0, 0)",
                params![Uuid::new_v4().to_string(), dataset_id, field_name, display_order],
            )?;
        }
        for (item_id, rank) in assignments {
            let json_text = transaction.query_row(
                "SELECT fields_json FROM items WHERE id = ?1",
                [&item_id],
                |row| row.get::<_, String>(0),
            )?;
            let mut fields: serde_json::Map<String, Value> = serde_json::from_str(&json_text)?;
            fields.insert(field_name.clone(), json!(rank));
            transaction.execute(
                "UPDATE items SET fields_json = ?1 WHERE id = ?2",
                params![serde_json::to_string(&fields)?, item_id],
            )?;
        }
        let now = Utc::now().to_rfc3339();
        transaction.execute(
            "UPDATE datasets SET updated_at = ?1 WHERE id = ?2",
            params![now, dataset_id],
        )?;
        transaction.execute(
            "UPDATE sort_tasks SET rank_written_at = COALESCE(rank_written_at, ?1), rank_written_field = COALESCE(rank_written_field, ?2), updated_at = ?1 WHERE id = ?3",
            params![now, field_name, task_id],
        )?;
        transaction.commit()?;
        self.load_result_preview(task_id)
    }

    pub(crate) fn load_export_rows(&self, task_id: &str) -> Result<Vec<ExportRow>, AppError> {
        let status = self
            .connection()
            .query_row(
                "SELECT status FROM sort_tasks WHERE id = ?1",
                [task_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| AppError::InvalidInput("找不到排序任务".into()))?;
        if status != "confirmed" {
            return Err(AppError::InvalidInput(
                "请先确认排序结果，再导出文件".into(),
            ));
        }
        let mut statement = self.connection().prepare(
            "SELECT 1 + (SELECT COUNT(*) FROM rank_groups previous \
                         JOIN rank_group_items previous_items ON previous_items.rank_group_id = previous.id \
                         WHERE previous.sort_task_id = rg.sort_task_id AND previous.is_active = 1 AND previous.position < rg.position), \
                    i.original_index, i.fields_json FROM rank_groups rg \
             JOIN rank_group_items rgi ON rgi.rank_group_id = rg.id \
             JOIN items i ON i.id = rgi.item_id \
             WHERE rg.sort_task_id = ?1 AND rg.is_active = 1 ORDER BY rg.position, rgi.item_order",
        )?;
        let rows = statement.query_map([task_id], |row| {
            Ok((
                row.get::<_, usize>(0)?,
                row.get::<_, usize>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        rows.map(|row| {
            let (position, original_index, fields_json) = row?;
            Ok(ExportRow {
                final_rank: position,
                original_index,
                fields: serde_json::from_str(&fields_json)?,
            })
        })
        .collect()
    }

    pub(crate) fn dataset_field_names(&self, task_id: &str) -> Result<Vec<String>, AppError> {
        let mut statement = self.connection().prepare(
            "SELECT f.name FROM field_definitions f JOIN sort_tasks t ON t.dataset_id = f.dataset_id WHERE t.id = ?1 ORDER BY f.display_order",
        )?;
        let rows = statement.query_map([task_id], |row| row.get(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    fn comparison_is_complete(&self, task_id: &str) -> Result<bool, AppError> {
        let state_json = self
            .connection()
            .query_row(
                "SELECT state_json FROM comparison_sessions WHERE sort_task_id = ?1",
                [task_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(state_json) = state_json else {
            return Ok(false);
        };
        let state: ComparisonSessionState = serde_json::from_str(&state_json)?;
        Ok(state.queue.is_empty())
    }

    fn matrix_is_complete(&self, task_id: &str) -> Result<bool, AppError> {
        let state_json = self
            .connection()
            .query_row(
                "SELECT state_json FROM matrix_sessions WHERE sort_task_id = ?1",
                [task_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(state_json) = state_json else {
            return Ok(false);
        };
        let state: crate::domain::matrix::MatrixSessionState = serde_json::from_str(&state_json)?;
        Ok(state.completed())
    }

    fn rank_assignments(
        &self,
        task_id: &str,
        rank_rule: RankRule,
    ) -> Result<Vec<(String, usize)>, AppError> {
        let mut statement = self.connection().prepare(
            "SELECT rg.id, i.id FROM rank_groups rg JOIN rank_group_items rgi ON rgi.rank_group_id = rg.id JOIN items i ON i.id = rgi.item_id WHERE rg.sort_task_id = ?1 AND rg.is_active = 1 ORDER BY rg.position, rgi.item_order",
        )?;
        let rows = statement.query_map([task_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        // competition 跳过并列占用的名次，dense 每组递增 1，ordinal 则忽略并列逐条编号。
        let mut assignments = Vec::new();
        let mut current_group = String::new();
        let mut dense_rank = 0;
        let mut group_rank = 0;
        for (index, row) in rows.enumerate() {
            let (group_id, item_id) = row?;
            if group_id != current_group {
                current_group = group_id;
                dense_rank += 1;
                group_rank = match rank_rule {
                    RankRule::Competition => index + 1,
                    RankRule::Dense => dense_rank,
                    RankRule::Ordinal => 0,
                };
            }
            assignments.push((
                item_id,
                if rank_rule == RankRule::Ordinal {
                    index + 1
                } else {
                    group_rank
                },
            ));
        }
        Ok(assignments)
    }
}

fn validate_field_name(field_name: &str) -> Result<String, AppError> {
    let trimmed = field_name.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidInput("排名字段名称不能为空".into()));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(AppError::InvalidInput(
            "排名字段名称不能包含控制字符".into(),
        ));
    }
    Ok(trimmed.to_owned())
}
