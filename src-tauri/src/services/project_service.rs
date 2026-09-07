use std::{fs, path::Path};

use chrono::Utc;
use uuid::Uuid;

use crate::{
    domain::project::{ProjectMetadata, CURRENT_DATA_FORMAT_VERSION, PROJECT_EXTENSION},
    error::AppError,
    repository::project_repository::ProjectRepository,
};

pub struct ProjectService;

impl ProjectService {
    pub fn create(name: &str, parent_directory: &Path) -> Result<ProjectMetadata, AppError> {
        let normalized_name = validate_name(name)?;
        if !parent_directory.is_dir() {
            return Err(AppError::InvalidInput("保存位置不是有效文件夹".into()));
        }
        let project_path = parent_directory.join(format!("{normalized_name}.{PROJECT_EXTENSION}"));
        if project_path.exists() {
            return Err(AppError::AlreadyExists(project_path.display().to_string()));
        }

        fs::create_dir(&project_path)
            .map_err(|error| AppError::from_io(error, parent_directory))?;
        let result = Self::initialize_project(&project_path, &normalized_name);
        if result.is_err() {
            let _ = fs::remove_dir_all(&project_path);
        }
        result
    }

    fn initialize_project(project_path: &Path, name: &str) -> Result<ProjectMetadata, AppError> {
        fs::create_dir(project_path.join("source"))
            .map_err(|error| AppError::from_io(error, project_path))?;
        fs::create_dir(project_path.join("assets"))
            .map_err(|error| AppError::from_io(error, project_path))?;

        let now = Utc::now();
        let metadata = ProjectMetadata {
            id: Uuid::new_v4().to_string(),
            name: name.to_owned(),
            created_at: now,
            last_opened_at: now,
            data_format_version: CURRENT_DATA_FORMAT_VERSION,
            project_path: project_path.to_path_buf(),
        };

        ProjectRepository::create(&project_path.join("project.sqlite"), &metadata)?;
        write_metadata(project_path, &metadata)?;
        Ok(metadata)
    }

    pub fn open(project_path: &Path) -> Result<ProjectMetadata, AppError> {
        validate_project_layout(project_path)?;
        let metadata_contents = fs::read_to_string(project_path.join("metadata.json"))
            .map_err(|error| AppError::from_io(error, project_path))?;
        let mut metadata: ProjectMetadata = serde_json::from_str(&metadata_contents)?;
        if metadata.data_format_version != CURRENT_DATA_FORMAT_VERSION {
            return Err(AppError::UnsupportedVersion {
                found: metadata.data_format_version,
                supported: CURRENT_DATA_FORMAT_VERSION,
            });
        }

        let mut repository = ProjectRepository::open(&project_path.join("project.sqlite"))?;
        repository.validate_metadata(&metadata)?;
        metadata.last_opened_at = Utc::now();
        metadata.project_path = project_path.to_path_buf();
        repository.update_last_opened(&metadata)?;
        write_metadata(project_path, &metadata)?;
        Ok(metadata)
    }
}

fn validate_name(name: &str) -> Result<String, AppError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidInput("项目名称不能为空".into()));
    }
    if trimmed.ends_with('.')
        || trimmed.ends_with(' ')
        || trimmed.chars().any(|character| {
            character < '\u{20}'
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
        })
    {
        return Err(AppError::InvalidInput(
            "项目名称包含 Windows 文件名不允许的字符".into(),
        ));
    }
    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if reserved
        .iter()
        .any(|reserved_name| trimmed.eq_ignore_ascii_case(reserved_name))
    {
        return Err(AppError::InvalidInput("项目名称是 Windows 保留名称".into()));
    }
    Ok(trimmed.to_owned())
}

fn validate_project_layout(project_path: &Path) -> Result<(), AppError> {
    let has_extension =
        project_path.extension().and_then(|value| value.to_str()) == Some(PROJECT_EXTENSION);
    let required = ["metadata.json", "project.sqlite", "source", "assets"];
    if !project_path.is_dir()
        || !has_extension
        || required
            .iter()
            .any(|entry| !project_path.join(entry).exists())
    {
        return Err(AppError::InvalidProject(project_path.display().to_string()));
    }
    Ok(())
}

fn write_metadata(project_path: &Path, metadata: &ProjectMetadata) -> Result<(), AppError> {
    let destination = project_path.join("metadata.json");
    let temporary = project_path.join("metadata.json.tmp");
    let content = serde_json::to_vec_pretty(metadata)?;
    fs::write(&temporary, content).map_err(|error| AppError::from_io(error, project_path))?;
    // Windows 的 rename 不会覆盖已有文件。元数据体积很小，先删除旧文件再提交临时文件；
    // 任何一步失败都会作为显式保存错误返回前端。
    if destination.exists() {
        fs::remove_file(&destination).map_err(|error| AppError::from_io(error, project_path))?;
    }
    fs::rename(&temporary, &destination).map_err(|error| AppError::from_io(error, project_path))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_reopen_empty_project() {
        let directory = tempfile::tempdir().unwrap();
        let created = ProjectService::create("测试项目", directory.path()).unwrap();
        assert!(created.project_path.join("project.sqlite").is_file());
        assert!(created.project_path.join("source").is_dir());
        let reopened = ProjectService::open(&created.project_path).unwrap();
        assert_eq!(created.id, reopened.id);
        assert_eq!(created.name, reopened.name);
        assert!(reopened.last_opened_at >= created.last_opened_at);
    }

    #[test]
    fn rejects_invalid_project_name() {
        let directory = tempfile::tempdir().unwrap();
        assert!(matches!(
            ProjectService::create("bad/name", directory.path()),
            Err(AppError::InvalidInput(_))
        ));
    }

    #[test]
    fn accepts_chinese_underscores_and_date_digits() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("测试_20260803", directory.path()).unwrap();
        assert_eq!(project.name, "测试_20260803");
        assert!(project.project_path.is_dir());
    }

    #[test]
    fn creates_all_initial_tables() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("schema", directory.path()).unwrap();
        let repository =
            ProjectRepository::open(&project.project_path.join("project.sqlite")).unwrap();
        let tables = repository.table_names().unwrap();
        for required in [
            "comparisons",
            "datasets",
            "field_definitions",
            "items",
            "media_assets",
            "matrix_sessions",
            "operation_logs",
            "project_runtime_state",
            "project_snapshots",
            "project_info",
            "rank_group_items",
            "rank_groups",
            "schema_migrations",
            "slider_ratings",
            "snapshots",
            "sort_tasks",
        ] {
            assert!(
                tables.contains(&required.to_owned()),
                "missing table {required}"
            );
        }
        assert_eq!(repository.migration_versions().unwrap(), [10_000]);
        let sort_task_columns = repository.sort_task_column_names().unwrap();
        assert!(sort_task_columns.contains(&"mode".to_owned()));
        assert!(sort_task_columns.contains(&"matrix_comparison_percent".to_owned()));
        assert!(!sort_task_columns.contains(&"mode_variant".to_owned()));
        assert!(!sort_task_columns.contains(&"sort_method".to_owned()));
    }

    #[test]
    fn rejects_pre_release_project_format() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("旧格式", directory.path()).unwrap();
        let metadata_path = project.project_path.join("metadata.json");
        let mut metadata: serde_json::Value =
            serde_json::from_slice(&fs::read(&metadata_path).unwrap()).unwrap();
        metadata["dataFormatVersion"] = serde_json::json!(1);
        fs::write(
            &metadata_path,
            serde_json::to_vec_pretty(&metadata).unwrap(),
        )
        .unwrap();

        assert!(matches!(
            ProjectService::open(&project.project_path),
            Err(AppError::UnsupportedVersion {
                found: 1,
                supported: 10_000
            })
        ));
    }

    #[test]
    fn rejects_non_baseline_database_schema() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("错误基线", directory.path()).unwrap();
        let database_path = project.project_path.join("project.sqlite");
        let connection = rusqlite::Connection::open(&database_path).unwrap();
        connection
            .execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (10001, CURRENT_TIMESTAMP)",
                [],
            )
            .unwrap();
        drop(connection);

        assert!(matches!(
            ProjectService::open(&project.project_path),
            Err(AppError::UnsupportedDatabaseSchema)
        ));
    }
}
