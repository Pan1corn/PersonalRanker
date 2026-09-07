use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

use crate::{
    domain::project::{ProjectMetadata, CURRENT_DATA_FORMAT_VERSION},
    error::AppError,
};

const V1_BASELINE_SCHEMA: &str = include_str!("../migrations/10000_v1_0_0_baseline.sql");
const V1_BASELINE_SCHEMA_VERSION: i64 = CURRENT_DATA_FORMAT_VERSION as i64;

pub struct ProjectRepository {
    pub(super) connection: Connection,
}

impl ProjectRepository {
    pub(crate) const fn connection(&self) -> &Connection {
        &self.connection
    }

    pub(crate) const fn connection_mut(&mut self) -> &mut Connection {
        &mut self.connection
    }
    pub fn create(database_path: &Path, metadata: &ProjectMetadata) -> Result<Self, AppError> {
        let mut connection = Connection::open(database_path)?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        let transaction = connection.transaction()?;
        transaction.execute_batch(V1_BASELINE_SCHEMA)?;
        transaction.execute(
            "INSERT INTO project_info (id, name, created_at, last_opened_at, data_format_version) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![metadata.id, metadata.name, metadata.created_at.to_rfc3339(), metadata.last_opened_at.to_rfc3339(), metadata.data_format_version],
        )?;
        transaction.commit()?;
        Ok(Self { connection })
    }

    pub fn open(database_path: &Path) -> Result<Self, AppError> {
        let connection = Connection::open(database_path)?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        let repository = Self { connection };
        repository.validate_database_baseline()?;
        Ok(repository)
    }

    fn validate_database_baseline(&self) -> Result<(), AppError> {
        let versions = (|| -> rusqlite::Result<Vec<i64>> {
            let mut statement = self
                .connection
                .prepare("SELECT version FROM schema_migrations ORDER BY version")?;
            let rows = statement.query_map([], |row| row.get(0))?;
            rows.collect()
        })();

        match versions {
            Ok(versions) if versions == [V1_BASELINE_SCHEMA_VERSION] => Ok(()),
            _ => Err(AppError::UnsupportedDatabaseSchema),
        }
    }

    pub fn validate_metadata(&self, metadata: &ProjectMetadata) -> Result<(), AppError> {
        let row = self
            .connection
            .query_row(
                "SELECT id, name, data_format_version FROM project_info LIMIT 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, u32>(2)?,
                    ))
                },
            )
            .optional()?;

        match row {
            Some((id, name, version))
                if id == metadata.id
                    && name == metadata.name
                    && version == metadata.data_format_version =>
            {
                Ok(())
            }
            _ => Err(AppError::InvalidProject("元数据与项目数据库不一致".into())),
        }
    }

    pub fn update_last_opened(&mut self, metadata: &ProjectMetadata) -> Result<(), AppError> {
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "UPDATE project_info SET last_opened_at = ?1 WHERE id = ?2",
            params![metadata.last_opened_at.to_rfc3339(), metadata.id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    #[cfg(test)]
    pub fn table_names(&self) -> Result<Vec<String>, AppError> {
        let mut statement = self.connection.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")?;
        let rows = statement.query_map([], |row| row.get(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    #[cfg(test)]
    pub fn migration_versions(&self) -> Result<Vec<i64>, AppError> {
        let mut statement = self
            .connection
            .prepare("SELECT version FROM schema_migrations ORDER BY version")?;
        let rows = statement.query_map([], |row| row.get(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    #[cfg(test)]
    pub fn sort_task_column_names(&self) -> Result<Vec<String>, AppError> {
        let mut statement = self.connection.prepare("PRAGMA table_info(sort_tasks)")?;
        let rows = statement.query_map([], |row| row.get(1))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}
