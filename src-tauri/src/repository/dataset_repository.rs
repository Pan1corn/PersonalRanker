use chrono::Utc;
use rusqlite::{params, OptionalExtension, Transaction};
use uuid::Uuid;

use crate::{
    domain::{
        dataset::{DatasetOverview, SavedFieldDefinition},
        import::{ImportResult, ImportSourceType, ImportedFieldType},
    },
    error::AppError,
    repository::project_repository::ProjectRepository,
};

impl ProjectRepository {
    pub fn dataset_file_references(
        &self,
        dataset_id: &str,
    ) -> Result<(Option<String>, Vec<String>), AppError> {
        let source_filename = self
            .connection()
            .query_row(
                "SELECT source_filename FROM datasets WHERE id = ?1",
                [dataset_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .ok_or_else(|| AppError::InvalidInput("找不到要删除的数据表".into()))?;
        let mut statement = self.connection().prepare(
            "SELECT DISTINCT local_path FROM media_assets WHERE dataset_id = ?1 AND local_path IS NOT NULL",
        )?;
        let rows = statement.query_map([dataset_id], |row| row.get::<_, String>(0))?;
        Ok((source_filename, rows.collect::<Result<Vec<_>, _>>()?))
    }

    pub fn delete_dataset(&mut self, dataset_id: &str) -> Result<(), AppError> {
        let transaction = self.connection_mut().transaction()?;
        let deleted = transaction.execute("DELETE FROM datasets WHERE id = ?1", [dataset_id])?;
        if deleted == 0 {
            return Err(AppError::InvalidInput("找不到要删除的数据表".into()));
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn save_imported_dataset(
        &mut self,
        dataset_name: &str,
        source_filename: Option<&str>,
        import: &ImportResult,
        primary_identifier: &str,
        auxiliary_identifiers: &[String],
    ) -> Result<DatasetOverview, AppError> {
        let transaction = self.connection_mut().transaction()?;
        let dataset_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        transaction.execute(
            "INSERT INTO datasets (id, name, source_type, source_filename, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![dataset_id, dataset_name, import.source_type.as_str(), source_filename, now],
        )?;
        insert_fields(
            &transaction,
            &dataset_id,
            import,
            primary_identifier,
            auxiliary_identifiers,
        )?;
        for item in &import.items {
            transaction.execute(
                "INSERT INTO items (id, dataset_id, original_index, fields_json, is_valid, created_at) VALUES (?1, ?2, ?3, ?4, 1, ?5)",
                params![item.id, dataset_id, item.original_index, serde_json::to_string(&item.fields)?, now],
            )?;
        }
        transaction.commit()?;
        self.load_dataset(&dataset_id)
    }

    pub fn list_datasets(&self) -> Result<Vec<DatasetOverview>, AppError> {
        let mut statement = self.connection().prepare(
            "SELECT d.id, d.name, d.source_type, d.source_filename, COUNT(i.id) \
             FROM datasets d LEFT JOIN items i ON i.dataset_id = d.id AND i.is_valid = 1 \
             GROUP BY d.id ORDER BY d.created_at",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, usize>(4)?,
            ))
        })?;
        let records = rows.collect::<Result<Vec<_>, _>>()?;
        records
            .into_iter()
            .map(|(id, name, source_type, source_filename, item_count)| {
                Ok(DatasetOverview {
                    fields: self.load_fields(&id)?,
                    id,
                    name,
                    source_type: ImportSourceType::from_storage(&source_type).ok_or_else(|| {
                        AppError::InvalidProject(format!("未知的数据来源类型：{source_type}"))
                    })?,
                    source_filename,
                    item_count,
                })
            })
            .collect()
    }

    fn load_dataset(&self, dataset_id: &str) -> Result<DatasetOverview, AppError> {
        self.list_datasets()?
            .into_iter()
            .find(|dataset| dataset.id == dataset_id)
            .ok_or_else(|| AppError::InvalidProject("保存后无法重新读取数据集".into()))
    }

    fn load_fields(&self, dataset_id: &str) -> Result<Vec<SavedFieldDefinition>, AppError> {
        let mut statement = self.connection().prepare(
            "SELECT id, name, field_type, display_order, is_primary_identifier, is_auxiliary_identifier \
             FROM field_definitions WHERE dataset_id = ?1 ORDER BY display_order",
        )?;
        let rows = statement.query_map([dataset_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, usize>(3)?,
                row.get::<_, bool>(4)?,
                row.get::<_, bool>(5)?,
            ))
        })?;
        rows.map(|row| {
            let (id, name, field_type, display_order, is_primary, is_auxiliary) = row?;
            Ok(SavedFieldDefinition {
                id,
                name,
                field_type: ImportedFieldType::from_storage(&field_type).ok_or_else(|| {
                    AppError::InvalidProject(format!("未知的字段类型：{field_type}"))
                })?,
                display_order,
                is_primary_identifier: is_primary,
                is_auxiliary_identifier: is_auxiliary,
            })
        })
        .collect()
    }
}

fn insert_fields(
    transaction: &Transaction<'_>,
    dataset_id: &str,
    import: &ImportResult,
    primary_identifier: &str,
    auxiliary_identifiers: &[String],
) -> Result<(), AppError> {
    for field in &import.fields {
        transaction.execute(
            "INSERT INTO field_definitions (id, dataset_id, name, field_type, display_order, is_primary_identifier, is_auxiliary_identifier) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                Uuid::new_v4().to_string(),
                dataset_id,
                field.name,
                field.field_type.as_str(),
                field.display_order,
                field.name == primary_identifier,
                auxiliary_identifiers.contains(&field.name),
            ],
        )?;
    }
    Ok(())
}
