//! 自动保存会话与完整数据库快照。
//! 自动保存记录“最近成功事务”，快照则使用独立 SQLite 文件支持跨操作恢复。

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use chrono::Utc;
use rusqlite::{Connection, OpenFlags};
use uuid::Uuid;

use crate::{
    domain::history::{AutosaveStatus, ProjectSnapshot, RestoreSnapshotResult, SnapshotType},
    error::AppError,
    repository::{
        history_repository::{parse_timestamp, SnapshotRecord},
        project_repository::ProjectRepository,
    },
};

const SNAPSHOT_LIMIT: usize = 50;
const DATABASE_FILE: &str = "project.sqlite";
const SNAPSHOT_DIRECTORY: &str = "snapshots";

#[derive(Default)]
/// 记录本进程打开的项目，用于窗口关闭时清除“未正常退出”会话标记。
pub struct OpenProjectRegistry(Mutex<HashSet<PathBuf>>);

impl OpenProjectRegistry {
    pub fn register(&self, project_path: PathBuf) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(project_path);
    }

    pub fn unregister(&self, project_path: &Path) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(project_path);
    }

    pub fn paths(&self) -> Vec<PathBuf> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .cloned()
            .collect()
    }
}

pub struct AutosaveService;

impl AutosaveService {
    pub fn begin_session(project_path: &Path) -> Result<AutosaveStatus, AppError> {
        let repository = open_repository(project_path)?;
        repository.begin_autosave_session()
    }

    pub fn status(project_path: &Path) -> Result<AutosaveStatus, AppError> {
        let repository = open_repository(project_path)?;
        repository.runtime_status()
    }

    pub fn record(project_path: &Path, action: &str) -> Result<AutosaveStatus, AppError> {
        let repository = open_repository(project_path)?;
        repository.record_autosave(action)
    }

    pub fn end_session(project_path: &Path) -> Result<(), AppError> {
        let repository = open_repository(project_path)?;
        repository.end_autosave_session()
    }
}

pub struct SnapshotService;

impl SnapshotService {
    pub fn create(
        project_path: &Path,
        snapshot_type: SnapshotType,
        source_task_id: Option<&str>,
        source_dataset_id: Option<&str>,
    ) -> Result<ProjectSnapshot, AppError> {
        Self::create_inner(
            project_path,
            snapshot_type,
            source_task_id,
            source_dataset_id,
            true,
        )
    }

    fn create_inner(
        project_path: &Path,
        snapshot_type: SnapshotType,
        source_task_id: Option<&str>,
        source_dataset_id: Option<&str>,
        prune: bool,
    ) -> Result<ProjectSnapshot, AppError> {
        let snapshot_directory = project_path.join(SNAPSHOT_DIRECTORY);
        fs::create_dir_all(&snapshot_directory)
            .map_err(|error| AppError::from_io(error, project_path))?;
        let id = Uuid::new_v4().to_string();
        let file_name = format!("{id}.sqlite");
        let destination = snapshot_directory.join(&file_name);
        let record = SnapshotRecord {
            id,
            snapshot_type: snapshot_type.as_str().to_owned(),
            label: snapshot_type.default_label().to_owned(),
            file_name,
            source_task_id: source_task_id.map(str::to_owned),
            source_dataset_id: source_dataset_id.map(str::to_owned),
            created_at: Utc::now().to_rfc3339(),
        };
        // 先落元数据，再用 SQLite VACUUM INTO 生成一致性快照；任一步失败都会回滚记录和文件。
        let repository = open_repository(project_path)?;
        repository.insert_snapshot_record(&record)?;
        if let Err(error) = repository.vacuum_into(&destination) {
            let _ = repository.delete_snapshot_record(&record.id);
            let _ = fs::remove_file(&destination);
            return Err(error);
        }
        drop(repository);
        if prune {
            Self::prune(project_path)?;
        }
        Self::record_to_domain(project_path, &record)
    }

    pub fn list(project_path: &Path) -> Result<Vec<ProjectSnapshot>, AppError> {
        let repository = open_repository(project_path)?;
        repository
            .snapshot_records()?
            .iter()
            .map(|record| Self::record_to_domain(project_path, record))
            .collect()
    }

    pub fn restore(
        project_path: &Path,
        snapshot_id: &str,
    ) -> Result<RestoreSnapshotResult, AppError> {
        // 恢复前始终保留当前状态，避免误恢复后无法返回。
        let repository = open_repository(project_path)?;
        let selected = repository.snapshot_record(snapshot_id)?;
        let source = project_path
            .join(SNAPSHOT_DIRECTORY)
            .join(&selected.file_name);
        validate_snapshot_database(project_path, &source, repository.connection())?;
        let selected_domain = Self::record_to_domain(project_path, &selected)?;
        drop(repository);
        Self::create_inner(project_path, SnapshotType::BeforeRestore, None, None, false)?;
        let repository = open_repository(project_path)?;
        let all_records = repository.snapshot_records()?;
        drop(repository);

        let database = project_path.join(DATABASE_FILE);
        let token = Uuid::new_v4();
        let replacement = project_path.join(format!("project.sqlite.restore-{token}"));
        let rollback = project_path.join(format!("project.sqlite.rollback-{token}"));
        // replacement 先完整复制，原库改名为 rollback；新库验证失败时可立即换回。
        fs::copy(&source, &replacement).map_err(|error| AppError::from_io(error, &source))?;
        remove_sqlite_sidecars(&database);
        fs::rename(&database, &rollback).map_err(|error| AppError::from_io(error, &database))?;
        if let Err(error) = fs::rename(&replacement, &database) {
            let _ = fs::rename(&rollback, &database);
            return Err(AppError::from_io(error, &database));
        }

        let restored = ProjectRepository::open(&database).and_then(|mut repository| {
            repository.merge_snapshot_records(&all_records)?;
            repository.begin_autosave_session()?;
            repository.record_autosave("恢复历史快照")
        });
        let autosave = match restored {
            Ok(status) => status,
            Err(error) => {
                let _ = fs::remove_file(&database);
                let _ = fs::rename(&rollback, &database);
                return Err(error);
            }
        };
        fs::remove_file(&rollback).map_err(|error| AppError::from_io(error, project_path))?;
        Self::prune(project_path)?;
        Ok(RestoreSnapshotResult {
            snapshot: selected_domain,
            autosave,
        })
    }

    fn prune(project_path: &Path) -> Result<(), AppError> {
        let repository = open_repository(project_path)?;
        let records = repository.snapshot_records()?;
        // snapshot_records 按时间倒序返回，因此只删除第 51 个及更旧的快照。
        for record in records.iter().skip(SNAPSHOT_LIMIT) {
            repository.delete_snapshot_record(&record.id)?;
            let file = project_path
                .join(SNAPSHOT_DIRECTORY)
                .join(&record.file_name);
            if file.exists() {
                fs::remove_file(&file).map_err(|error| AppError::from_io(error, &file))?;
            }
        }
        Ok(())
    }

    fn record_to_domain(
        project_path: &Path,
        record: &SnapshotRecord,
    ) -> Result<ProjectSnapshot, AppError> {
        let file = project_path
            .join(SNAPSHOT_DIRECTORY)
            .join(&record.file_name);
        let size_bytes = fs::metadata(&file)
            .map_err(|error| AppError::from_io(error, &file))?
            .len();
        Ok(ProjectSnapshot {
            id: record.id.clone(),
            snapshot_type: record.snapshot_type.clone(),
            label: record.label.clone(),
            source_task_id: record.source_task_id.clone(),
            source_dataset_id: record.source_dataset_id.clone(),
            created_at: parse_timestamp(&record.created_at)?,
            size_bytes,
        })
    }
}

fn open_repository(project_path: &Path) -> Result<ProjectRepository, AppError> {
    if project_path.extension().and_then(|value| value.to_str()) != Some("subject-sort") {
        return Err(AppError::InvalidProject(project_path.display().to_string()));
    }
    ProjectRepository::open(&project_path.join(DATABASE_FILE))
}

fn validate_snapshot_database(
    project_path: &Path,
    snapshot_path: &Path,
    current: &Connection,
) -> Result<(), AppError> {
    let snapshot = Connection::open_with_flags(snapshot_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let integrity =
        snapshot.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))?;
    if integrity != "ok" {
        return Err(AppError::InvalidProject(
            "所选快照数据库未通过完整性检查".into(),
        ));
    }
    let current_id = current.query_row("SELECT id FROM project_info LIMIT 1", [], |row| {
        row.get::<_, String>(0)
    })?;
    let snapshot_id = snapshot.query_row("SELECT id FROM project_info LIMIT 1", [], |row| {
        row.get::<_, String>(0)
    })?;
    if current_id != snapshot_id {
        return Err(AppError::InvalidProject(format!(
            "快照不属于当前项目：{}",
            project_path.display()
        )));
    }
    Ok(())
}

fn remove_sqlite_sidecars(database: &Path) {
    for suffix in ["-wal", "-shm"] {
        let sidecar = PathBuf::from(format!("{}{suffix}", database.display()));
        if sidecar.exists() {
            let _ = fs::remove_file(sidecar);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        domain::{
            dataset::{CommitDatasetImport, DatasetImportSource},
            history::SnapshotType,
        },
        services::{dataset_service::DatasetService, project_service::ProjectService},
    };

    use super::*;

    #[test]
    fn detects_unclean_session_and_clears_it_after_normal_close() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("自动保存", directory.path()).unwrap();

        let first = AutosaveService::begin_session(&project.project_path).unwrap();
        assert!(!first.recovered_unclean_session);
        AutosaveService::record(&project.project_path, "移动条目").unwrap();
        let recovered = AutosaveService::begin_session(&project.project_path).unwrap();
        assert!(recovered.recovered_unclean_session);
        assert_eq!(recovered.last_autosave_action.as_deref(), Some("移动条目"));

        AutosaveService::end_session(&project.project_path).unwrap();
        let reopened = AutosaveService::begin_session(&project.project_path).unwrap();
        assert!(!reopened.recovered_unclean_session);
    }

    #[test]
    fn restores_full_project_and_preserves_snapshot_history() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("快照恢复", directory.path()).unwrap();
        let first = import_dataset(&project.project_path, "第一批", "A\nB");
        let snapshot = SnapshotService::create(
            &project.project_path,
            SnapshotType::ImportCompleted,
            None,
            Some(&first.id),
        )
        .unwrap();
        import_dataset(&project.project_path, "第二批", "C\nD");
        assert_eq!(
            DatasetService::list_datasets(&project.project_path)
                .unwrap()
                .len(),
            2
        );

        SnapshotService::restore(&project.project_path, &snapshot.id).unwrap();
        let datasets = DatasetService::list_datasets(&project.project_path).unwrap();
        assert_eq!(datasets.len(), 1);
        assert_eq!(datasets[0].name, "第一批");
        let history = SnapshotService::list(&project.project_path).unwrap();
        assert!(history.iter().any(|entry| entry.id == snapshot.id));
        assert!(history
            .iter()
            .any(|entry| entry.snapshot_type == "before_restore"));
    }

    #[test]
    fn retains_only_the_latest_fifty_snapshots() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("快照上限", directory.path()).unwrap();
        for _ in 0..51 {
            SnapshotService::create(
                &project.project_path,
                SnapshotType::ExportCompleted,
                None,
                None,
            )
            .unwrap();
        }
        assert_eq!(
            SnapshotService::list(&project.project_path).unwrap().len(),
            50
        );
        assert_eq!(
            fs::read_dir(project.project_path.join(SNAPSHOT_DIRECTORY))
                .unwrap()
                .count(),
            50
        );
    }

    fn import_dataset(
        project_path: &Path,
        name: &str,
        content: &str,
    ) -> crate::domain::dataset::DatasetOverview {
        DatasetService::commit_import(CommitDatasetImport {
            project_path: project_path.to_path_buf(),
            dataset_name: name.into(),
            source: DatasetImportSource::PastedText(content.into()),
            deduplicate: false,
            json_pointer: None,
            primary_identifier: "内容".into(),
            auxiliary_identifiers: vec![],
        })
        .unwrap()
    }
}
