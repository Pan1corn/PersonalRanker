use std::collections::HashMap;

use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    domain::scoring::{ScoreConfig, ScoringGroup, ScoringItem},
    error::AppError,
    repository::{project_repository::ProjectRepository, sort_task_repository::display_value},
};

impl ProjectRepository {
    pub(crate) fn load_score_config(&self, task_id: &str) -> Result<Option<ScoreConfig>, AppError> {
        let config = self
            .connection()
            .query_row(
                "SELECT config_json FROM score_configs WHERE sort_task_id = ?1",
                [task_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        config
            .map(|value| serde_json::from_str(&value).map_err(AppError::from))
            .transpose()
    }

    pub(crate) fn save_score_config(
        &self,
        task_id: &str,
        config: &ScoreConfig,
    ) -> Result<(), AppError> {
        let exists = self.connection().query_row(
            "SELECT EXISTS(SELECT 1 FROM sort_tasks WHERE id = ?1)",
            [task_id],
            |row| row.get::<_, bool>(0),
        )?;
        if !exists {
            return Err(AppError::InvalidInput("找不到排序任务".into()));
        }
        self.connection().execute(
            "INSERT INTO score_configs (sort_task_id, config_json, updated_at) VALUES (?1, ?2, ?3) ON CONFLICT(sort_task_id) DO UPDATE SET config_json = excluded.config_json, updated_at = excluded.updated_at",
            params![task_id, serde_json::to_string(config)?, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub(crate) fn load_scoring_groups(&self, task_id: &str) -> Result<Vec<ScoringGroup>, AppError> {
        let (dataset_id, status) = self
            .connection()
            .query_row(
                "SELECT dataset_id, status FROM sort_tasks WHERE id = ?1",
                [task_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?
            .ok_or_else(|| AppError::InvalidInput("找不到排序任务".into()))?;
        if status != "confirmed" {
            return Err(AppError::InvalidInput(
                "请先确认排序结果，再配置评分".into(),
            ));
        }
        let primary_name = self
            .load_display_fields(&dataset_id)?
            .into_iter()
            .find(|(_, primary, _)| *primary)
            .map(|(name, _, _)| name)
            .ok_or_else(|| AppError::InvalidProject("数据集缺少主标识字段".into()))?;
        let mut statement = self.connection().prepare(
            "SELECT rg.id, rg.name, i.id, i.fields_json FROM rank_groups rg \
             JOIN rank_group_items rgi ON rgi.rank_group_id = rg.id \
             JOIN items i ON i.id = rgi.item_id \
             WHERE rg.sort_task_id = ?1 AND rg.is_active = 1 \
             ORDER BY rg.position, rgi.item_order",
        )?;
        let rows = statement.query_map([task_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        let mut order = Vec::new();
        let mut groups: HashMap<String, ScoringGroup> = HashMap::new();
        for row in rows {
            let (group_id, group_name, item_id, fields_json) = row?;
            let fields: serde_json::Map<String, Value> = serde_json::from_str(&fields_json)?;
            if !groups.contains_key(&group_id) {
                order.push(group_id.clone());
                groups.insert(
                    group_id.clone(),
                    ScoringGroup {
                        group_id: group_id.clone(),
                        group_name,
                        items: Vec::new(),
                    },
                );
            }
            groups
                .get_mut(&group_id)
                .expect("group inserted above")
                .items
                .push(ScoringItem {
                    item_id,
                    primary_label: display_value(fields.get(&primary_name)),
                    fields,
                });
        }
        Ok(order
            .into_iter()
            .filter_map(|group_id| groups.remove(&group_id))
            .collect())
    }

    pub(crate) fn numeric_field_type(
        &self,
        task_id: &str,
        field_name: &str,
    ) -> Result<Option<String>, AppError> {
        self.connection()
            .query_row(
                "SELECT f.field_type FROM field_definitions f JOIN sort_tasks t ON t.dataset_id = f.dataset_id WHERE t.id = ?1 AND f.name = ?2",
                params![task_id, field_name],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub(crate) fn write_scores(
        &mut self,
        task_id: &str,
        config: &ScoreConfig,
        values: &[(String, f64)],
        overwrite: bool,
    ) -> Result<(), AppError> {
        let dataset_id = self.connection().query_row(
            "SELECT dataset_id FROM sort_tasks WHERE id = ?1 AND status = 'confirmed'",
            [task_id],
            |row| row.get::<_, String>(0),
        )?;
        let existing_type = self.numeric_field_type(task_id, &config.field_name)?;
        if existing_type.is_some() && !overwrite {
            return Err(AppError::ConfirmationRequired(format!(
                "字段“{}”已存在，写入评分会覆盖当前值",
                config.field_name
            )));
        }
        if existing_type
            .as_deref()
            .is_some_and(|kind| kind != "number")
        {
            return Err(AppError::InvalidInput(format!(
                "已有字段“{}”不是数字类型，不能写入评分",
                config.field_name
            )));
        }
        let now = Utc::now().to_rfc3339();
        let transaction = self.connection_mut().transaction()?;
        if existing_type.is_none() {
            let display_order = transaction.query_row(
                "SELECT COALESCE(MAX(display_order), -1) + 1 FROM field_definitions WHERE dataset_id = ?1",
                [&dataset_id],
                |row| row.get::<_, usize>(0),
            )?;
            transaction.execute(
                "INSERT INTO field_definitions (id, dataset_id, name, field_type, display_order, is_primary_identifier, is_auxiliary_identifier) VALUES (?1, ?2, ?3, 'number', ?4, 0, 0)",
                params![Uuid::new_v4().to_string(), dataset_id, config.field_name, display_order],
            )?;
        }
        for (item_id, score) in values {
            let fields_json = transaction.query_row(
                "SELECT fields_json FROM items WHERE id = ?1",
                [item_id],
                |row| row.get::<_, String>(0),
            )?;
            let mut fields: serde_json::Map<String, Value> = serde_json::from_str(&fields_json)?;
            fields.insert(config.field_name.clone(), Value::from(*score));
            transaction.execute(
                "UPDATE items SET fields_json = ?1 WHERE id = ?2",
                params![serde_json::to_string(&fields)?, item_id],
            )?;
        }
        transaction.execute(
            "INSERT INTO score_configs (sort_task_id, config_json, updated_at) VALUES (?1, ?2, ?3) ON CONFLICT(sort_task_id) DO UPDATE SET config_json = excluded.config_json, updated_at = excluded.updated_at",
            params![task_id, serde_json::to_string(config)?, now],
        )?;
        transaction.execute(
            "UPDATE datasets SET updated_at = ?1 WHERE id = ?2",
            params![now, dataset_id],
        )?;
        transaction.commit()?;
        Ok(())
    }
}
