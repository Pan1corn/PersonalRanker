use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};

use crate::{
    domain::history::AutosaveStatus, error::AppError,
    repository::project_repository::ProjectRepository,
};

#[derive(Debug, Clone)]
pub(crate) struct SnapshotRecord {
    pub id: String,
    pub snapshot_type: String,
    pub label: String,
    pub file_name: String,
    pub source_task_id: Option<String>,
    pub source_dataset_id: Option<String>,
    pub created_at: String,
}

impl ProjectRepository {
    pub(crate) fn begin_autosave_session(&self) -> Result<AutosaveStatus, AppError> {
        let previous = self.runtime_status()?;
        let now = Utc::now().to_rfc3339();
        self.connection().execute(
            "UPDATE project_runtime_state SET session_id = ?1, session_open = 1, session_started_at = ?2 WHERE singleton_id = 1",
            params![uuid::Uuid::new_v4().to_string(), now],
        )?;
        Ok(AutosaveStatus {
            session_open: true,
            recovered_unclean_session: previous.session_open,
            last_autosave_at: previous.last_autosave_at,
            last_autosave_action: previous.last_autosave_action,
        })
    }

    pub(crate) fn runtime_status(&self) -> Result<AutosaveStatus, AppError> {
        let (session_open, saved_at, action) = self.connection().query_row(
            "SELECT session_open, last_autosave_at, last_autosave_action FROM project_runtime_state WHERE singleton_id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, bool>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )?;
        Ok(AutosaveStatus {
            session_open,
            recovered_unclean_session: false,
            last_autosave_at: saved_at.map(|value| parse_timestamp(&value)).transpose()?,
            last_autosave_action: action,
        })
    }

    pub(crate) fn record_autosave(&self, action: &str) -> Result<AutosaveStatus, AppError> {
        self.connection().execute(
            "UPDATE project_runtime_state SET last_autosave_at = ?1, last_autosave_action = ?2 WHERE singleton_id = 1",
            params![Utc::now().to_rfc3339(), action],
        )?;
        self.runtime_status()
    }

    pub(crate) fn end_autosave_session(&self) -> Result<(), AppError> {
        self.connection().execute(
            "UPDATE project_runtime_state SET session_open = 0, last_clean_exit_at = ?1 WHERE singleton_id = 1",
            [Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub(crate) fn insert_snapshot_record(&self, record: &SnapshotRecord) -> Result<(), AppError> {
        self.connection().execute(
            "INSERT INTO project_snapshots (id, snapshot_type, label, file_name, source_task_id, source_dataset_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                record.id,
                record.snapshot_type,
                record.label,
                record.file_name,
                record.source_task_id,
                record.source_dataset_id,
                record.created_at
            ],
        )?;
        Ok(())
    }

    pub(crate) fn merge_snapshot_records(
        &mut self,
        records: &[SnapshotRecord],
    ) -> Result<(), AppError> {
        let transaction = self.connection_mut().transaction()?;
        for record in records {
            transaction.execute(
                "INSERT OR IGNORE INTO project_snapshots (id, snapshot_type, label, file_name, source_task_id, source_dataset_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![record.id, record.snapshot_type, record.label, record.file_name, record.source_task_id, record.source_dataset_id, record.created_at],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn snapshot_records(&self) -> Result<Vec<SnapshotRecord>, AppError> {
        let mut statement = self.connection().prepare(
            "SELECT id, snapshot_type, label, file_name, source_task_id, source_dataset_id, created_at FROM project_snapshots ORDER BY created_at DESC, rowid DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(SnapshotRecord {
                id: row.get(0)?,
                snapshot_type: row.get(1)?,
                label: row.get(2)?,
                file_name: row.get(3)?,
                source_task_id: row.get(4)?,
                source_dataset_id: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub(crate) fn snapshot_record(&self, id: &str) -> Result<SnapshotRecord, AppError> {
        self.connection()
            .query_row(
                "SELECT id, snapshot_type, label, file_name, source_task_id, source_dataset_id, created_at FROM project_snapshots WHERE id = ?1",
                [id],
                |row| {
                    Ok(SnapshotRecord {
                        id: row.get(0)?,
                        snapshot_type: row.get(1)?,
                        label: row.get(2)?,
                        file_name: row.get(3)?,
                        source_task_id: row.get(4)?,
                        source_dataset_id: row.get(5)?,
                        created_at: row.get(6)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| AppError::InvalidInput("找不到所选快照".into()))
    }

    pub(crate) fn delete_snapshot_record(&self, id: &str) -> Result<(), AppError> {
        self.connection()
            .execute("DELETE FROM project_snapshots WHERE id = ?1", [id])?;
        Ok(())
    }

    pub(crate) fn vacuum_into(&self, destination: &Path) -> Result<(), AppError> {
        self.connection()
            .execute_batch("PRAGMA wal_checkpoint(FULL);")?;
        self.connection().execute(
            "VACUUM main INTO ?1",
            [destination.to_string_lossy().as_ref()],
        )?;
        Ok(())
    }
}

pub(crate) fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, AppError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| AppError::InvalidProject("项目历史记录的时间格式无效".into()))
}
