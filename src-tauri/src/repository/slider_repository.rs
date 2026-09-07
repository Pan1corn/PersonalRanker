//! 滑杆排序的逐项评分持久化与最终并列组结算。

use std::collections::BTreeMap;

use chrono::Utc;
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::Value;

use crate::{
    domain::{
        comparison::ComparisonItem,
        slider::{SliderRating, SliderWorkspace},
        sort_task::{SortDisplayField, SortTaskMode, SortTaskStatus},
    },
    error::AppError,
    repository::{
        group_repository::merge_groups_in_transaction,
        project_repository::ProjectRepository,
        sort_task_repository::{display_value, persist_group_order},
    },
};

impl ProjectRepository {
    pub fn load_slider_workspace(&self, task_id: &str) -> Result<SliderWorkspace, AppError> {
        self.ensure_slider_task(task_id)?;
        self.build_slider_workspace(task_id)
    }

    pub fn confirm_slider_rating(
        &mut self,
        task_id: &str,
        value_hundredths: u16,
    ) -> Result<SliderWorkspace, AppError> {
        self.ensure_slider_task(task_id)?;
        self.ensure_slider_editable(task_id)?;
        if value_hundredths > 10_000 {
            return Err(AppError::InvalidInput(
                "滑杆位置必须在 0.00 到 100.00 之间".into(),
            ));
        }
        let next = self.next_unrated_slider_item(task_id)?;
        let Some((group_id, item_id)) = next else {
            return Err(AppError::InvalidInput("滑杆排序已经完成".into()));
        };
        let now = Utc::now().to_rfc3339();
        let transaction = self.connection_mut().transaction()?;
        let completed_order = transaction.query_row(
            "SELECT COUNT(*) FROM slider_ratings WHERE sort_task_id = ?1",
            [task_id],
            |row| row.get::<_, usize>(0),
        )?;
        transaction.execute(
            "INSERT INTO slider_ratings (sort_task_id, group_id, item_id, value_hundredths, completed_order, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![
                task_id,
                group_id,
                item_id,
                value_hundredths,
                completed_order,
                now,
            ],
        )?;
        let total_count = valid_item_count(&transaction, task_id)?;
        let completed = completed_order + 1 == total_count;
        if completed {
            finalize_slider(&transaction, task_id)?;
        }
        transaction.execute(
            "UPDATE sort_tasks SET status = 'sorting', updated_at = ?1 WHERE id = ?2",
            params![now, task_id],
        )?;
        transaction.commit()?;
        self.build_slider_workspace(task_id)
    }

    pub(crate) fn slider_task_is_complete(&self, task_id: &str) -> Result<bool, AppError> {
        let total_count = self.connection().query_row(
            "SELECT COUNT(*) FROM items i JOIN sort_tasks t ON t.dataset_id = i.dataset_id \
             WHERE t.id = ?1 AND i.is_valid = 1",
            [task_id],
            |row| row.get::<_, usize>(0),
        )?;
        let completed_count = self.connection().query_row(
            "SELECT COUNT(*) FROM slider_ratings WHERE sort_task_id = ?1",
            [task_id],
            |row| row.get::<_, usize>(0),
        )?;
        Ok(total_count > 0 && completed_count == total_count)
    }

    fn build_slider_workspace(&self, task_id: &str) -> Result<SliderWorkspace, AppError> {
        let (task_name, criteria, stored_status, dataset_id) = self.connection().query_row(
            "SELECT name, criteria, status, dataset_id FROM sort_tasks WHERE id = ?1",
            [task_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )?;
        let status = SortTaskStatus::from_storage(&stored_status)
            .ok_or_else(|| AppError::InvalidProject("排序任务状态无效".into()))?;
        let definitions = self.load_display_fields(&dataset_id)?;
        let primary_name = definitions
            .iter()
            .find(|(_, primary, _)| *primary)
            .map(|(name, _, _)| name.as_str())
            .ok_or_else(|| AppError::InvalidProject("数据集缺少主标识字段".into()))?;

        let current = self
            .connection()
            .query_row(
                "SELECT rg.id, i.id, i.fields_json FROM rank_groups rg \
                 JOIN rank_group_items rgi ON rgi.rank_group_id = rg.id \
                 JOIN items i ON i.id = rgi.item_id \
                 WHERE rg.sort_task_id = ?1 AND rg.is_active = 1 \
                   AND NOT EXISTS (SELECT 1 FROM slider_ratings sr WHERE sr.sort_task_id = ?1 AND sr.item_id = i.id) \
                 ORDER BY rg.position, rgi.item_order LIMIT 1",
                [task_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?
            .map(|(group_id, item_id, fields_json)| {
                slider_item(group_id, item_id, &fields_json, &definitions, primary_name)
            })
            .transpose()?;

        let mut statement = self.connection().prepare(
            "SELECT sr.group_id, sr.item_id, sr.value_hundredths, i.fields_json \
             FROM slider_ratings sr JOIN items i ON i.id = sr.item_id \
             WHERE sr.sort_task_id = ?1 ORDER BY sr.completed_order",
        )?;
        let rows = statement.query_map([task_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u16>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        let ratings = rows
            .map(|row| {
                let (group_id, item_id, value_hundredths, fields_json) = row?;
                Ok(SliderRating {
                    item: slider_item(group_id, item_id, &fields_json, &definitions, primary_name)?,
                    value: value_hundredths as f64 / 100.0,
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        let total_count = self.connection().query_row(
            "SELECT COUNT(*) FROM items WHERE dataset_id = ?1 AND is_valid = 1",
            [&dataset_id],
            |row| row.get::<_, usize>(0),
        )?;
        let completed_count = ratings.len();
        let completed = completed_count == total_count;
        Ok(SliderWorkspace {
            task_id: task_id.to_owned(),
            task_name,
            criteria,
            status,
            current,
            completed_count,
            total_count,
            progress_percent: completed_count
                .saturating_mul(100)
                .checked_div(total_count)
                .unwrap_or(100),
            completed,
            ratings,
        })
    }

    fn next_unrated_slider_item(
        &self,
        task_id: &str,
    ) -> Result<Option<(String, String)>, AppError> {
        self.connection()
            .query_row(
                "SELECT rg.id, i.id FROM rank_groups rg \
                 JOIN rank_group_items rgi ON rgi.rank_group_id = rg.id \
                 JOIN items i ON i.id = rgi.item_id \
                 WHERE rg.sort_task_id = ?1 AND rg.is_active = 1 \
                   AND NOT EXISTS (SELECT 1 FROM slider_ratings sr WHERE sr.sort_task_id = ?1 AND sr.item_id = i.id) \
                 ORDER BY rg.position, rgi.item_order LIMIT 1",
                [task_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(Into::into)
    }

    fn ensure_slider_task(&self, task_id: &str) -> Result<(), AppError> {
        if self.sort_task_mode(task_id)? != SortTaskMode::Slider {
            return Err(AppError::InvalidInput("该任务不是滑杆排序任务".into()));
        }
        Ok(())
    }

    fn ensure_slider_editable(&self, task_id: &str) -> Result<(), AppError> {
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

fn slider_item(
    group_id: String,
    item_id: String,
    fields_json: &str,
    definitions: &[(String, bool, bool)],
    primary_name: &str,
) -> Result<ComparisonItem, AppError> {
    let values: BTreeMap<String, Value> = serde_json::from_str(fields_json)?;
    let primary_label = display_value(values.get(primary_name));
    let fields = definitions
        .iter()
        .map(|(name, _, _)| SortDisplayField {
            name: name.clone(),
            value: values.get(name).cloned().unwrap_or(Value::Null),
        })
        .collect();
    let auxiliary_fields = definitions
        .iter()
        .filter(|(_, _, auxiliary)| *auxiliary)
        .map(|(name, _, _)| SortDisplayField {
            name: name.clone(),
            value: values.get(name).cloned().unwrap_or(Value::Null),
        })
        .collect();
    Ok(ComparisonItem {
        group_id,
        item_id: item_id.clone(),
        item_ids: vec![item_id],
        member_labels: vec![primary_label.clone()],
        group_name: None,
        primary_label,
        auxiliary_fields,
        fields,
    })
}

fn valid_item_count(transaction: &Transaction<'_>, task_id: &str) -> Result<usize, AppError> {
    Ok(transaction.query_row(
        "SELECT COUNT(*) FROM items i JOIN sort_tasks t ON t.dataset_id = i.dataset_id \
         WHERE t.id = ?1 AND i.is_valid = 1",
        [task_id],
        |row| row.get::<_, usize>(0),
    )?)
}

fn finalize_slider(transaction: &Transaction<'_>, task_id: &str) -> Result<(), AppError> {
    let ratings = {
        let mut statement = transaction.prepare(
            "SELECT group_id, value_hundredths FROM slider_ratings \
             WHERE sort_task_id = ?1 ORDER BY value_hundredths DESC, completed_order",
        )?;
        let rows = statement.query_map([task_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u16>(1)?))
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    let mut active_order = Vec::new();
    let mut cursor = 0;
    while cursor < ratings.len() {
        let value = ratings[cursor].1;
        let target = ratings[cursor].0.clone();
        active_order.push(target.clone());
        cursor += 1;
        while cursor < ratings.len() && ratings[cursor].1 == value {
            merge_groups_in_transaction(transaction, task_id, &ratings[cursor].0, &target)?;
            cursor += 1;
        }
    }
    persist_group_order(transaction, task_id, &active_order)
}
