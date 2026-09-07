use std::{
    collections::HashSet,
    fs,
    path::{Component, Path},
};

use uuid::Uuid;

use crate::{
    domain::{
        dataset::{CommitDatasetImport, DatasetImportSource, DatasetOverview},
        import::ImportResult,
    },
    error::AppError,
    repository::project_repository::ProjectRepository,
    services::{
        import_service::{ImportOptions, ImportService},
        media_service::MediaService,
        project_service::ProjectService,
    },
};

pub struct DatasetService;

impl DatasetService {
    pub fn commit_import(request: CommitDatasetImport) -> Result<DatasetOverview, AppError> {
        let dataset_name = request.dataset_name.trim();
        if dataset_name.is_empty() {
            return Err(AppError::InvalidInput("数据集名称不能为空".into()));
        }
        ProjectService::open(&request.project_path)?;
        let import = match &request.source {
            DatasetImportSource::File(path) => ImportService::import_file(
                path,
                ImportOptions {
                    deduplicate: request.deduplicate,
                    json_pointer: request.json_pointer.as_deref(),
                },
            )?,
            DatasetImportSource::PastedText(content) => {
                ImportService::import_text(content, request.deduplicate)?
            }
        };
        validate_identifiers(
            &import,
            &request.primary_identifier,
            &request.auxiliary_identifiers,
        )?;

        let (source_path, source_filename) = save_source_copy(
            &request.project_path,
            &request.source,
            import.source_type.as_str(),
        )?;
        let mut repository = ProjectRepository::open(&request.project_path.join("project.sqlite"))?;
        let result = repository.save_imported_dataset(
            dataset_name,
            Some(&source_filename),
            &import,
            &request.primary_identifier,
            &request.auxiliary_identifiers,
        );
        let dataset = match result {
            Ok(dataset) => dataset,
            Err(error) => {
                remove_readonly_file(&source_path);
                return Err(error);
            }
        };
        MediaService::index_imported_media(
            &request.project_path,
            &dataset.id,
            &request.source,
            &import,
        )?;
        Ok(dataset)
    }

    pub fn list_datasets(project_path: &Path) -> Result<Vec<DatasetOverview>, AppError> {
        ProjectService::open(project_path)?;
        ProjectRepository::open(&project_path.join("project.sqlite"))?.list_datasets()
    }

    pub fn delete_dataset(project_path: &Path, dataset_id: &str) -> Result<(), AppError> {
        ProjectService::open(project_path)?;
        let database_path = project_path.join("project.sqlite");
        let mut repository = ProjectRepository::open(&database_path)?;
        let (source_filename, media_paths) = repository.dataset_file_references(dataset_id)?;
        repository.delete_dataset(dataset_id)?;

        if let Some(filename) = source_filename {
            if Path::new(&filename)
                .file_name()
                .and_then(|value| value.to_str())
                == Some(&filename)
            {
                remove_readonly_file(&project_path.join("source").join(filename));
            }
        }
        let assets_root = project_path.join("assets");
        for relative_path in media_paths {
            let relative = Path::new(&relative_path);
            let safe_relative = relative
                .components()
                .all(|component| matches!(component, Component::Normal(_) | Component::CurDir));
            let candidate = project_path.join(relative);
            if safe_relative && candidate.starts_with(&assets_root) {
                remove_readonly_file(&candidate);
            }
        }
        Ok(())
    }
}

fn validate_identifiers(
    import: &ImportResult,
    primary_identifier: &str,
    auxiliary_identifiers: &[String],
) -> Result<(), AppError> {
    let field_names: HashSet<&str> = import
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .collect();
    if !field_names.contains(primary_identifier) {
        return Err(AppError::InvalidInput("必须选择有效的主标识字段".into()));
    }
    if auxiliary_identifiers
        .iter()
        .any(|name| name == primary_identifier || !field_names.contains(name.as_str()))
    {
        return Err(AppError::InvalidInput(
            "辅助标识字段无效，或与主标识字段重复".into(),
        ));
    }
    let unique_auxiliary_count = auxiliary_identifiers.iter().collect::<HashSet<_>>().len();
    if unique_auxiliary_count != auxiliary_identifiers.len() {
        return Err(AppError::InvalidInput("辅助标识字段不能重复".into()));
    }
    Ok(())
}

fn save_source_copy(
    project_path: &Path,
    source: &DatasetImportSource,
    source_type: &str,
) -> Result<(std::path::PathBuf, String), AppError> {
    let source_directory = project_path.join("source");
    let prefix = Uuid::new_v4().simple().to_string();
    let filename = match source {
        DatasetImportSource::File(path) => {
            let original_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("imported-data");
            format!("{prefix}_{original_name}")
        }
        DatasetImportSource::PastedText(_) => {
            let extension = if source_type == "text" {
                "txt"
            } else {
                source_type
            };
            format!("{prefix}_pasted.{extension}")
        }
    };
    let destination = source_directory.join(&filename);
    match source {
        DatasetImportSource::File(path) => {
            fs::copy(path, &destination)
                .map_err(|error| AppError::from_io(error, &source_directory))?;
        }
        DatasetImportSource::PastedText(content) => {
            fs::write(&destination, content)
                .map_err(|error| AppError::from_io(error, &source_directory))?;
        }
    }
    let mut permissions = fs::metadata(&destination)
        .map_err(|error| AppError::from_io(error, &destination))?
        .permissions();
    permissions.set_readonly(true);
    if let Err(error) = fs::set_permissions(&destination, permissions) {
        remove_readonly_file(&destination);
        return Err(AppError::from_io(error, &destination));
    }
    Ok((destination, filename))
}

fn remove_readonly_file(path: &Path) {
    if let Ok(metadata) = fs::metadata(path) {
        let mut permissions = metadata.permissions();
        // 产品目标仅为 Windows；这里用于回滚刚创建的只读源副本。
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        let _ = fs::set_permissions(path, permissions);
    }
    let _ = fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commits_and_reloads_dataset_with_field_configuration() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("导入测试", directory.path()).unwrap();
        let source = directory.path().join("tasks.csv");
        fs::write(&source, "name,priority,note\n任务A,1,甲\n任务B,2,乙").unwrap();

        let saved = DatasetService::commit_import(CommitDatasetImport {
            project_path: project.project_path.clone(),
            dataset_name: "任务清单".into(),
            source: DatasetImportSource::File(source),
            deduplicate: false,
            json_pointer: None,
            primary_identifier: "name".into(),
            auxiliary_identifiers: vec!["note".into()],
        })
        .unwrap();

        assert_eq!(saved.item_count, 2);
        assert!(saved
            .fields
            .iter()
            .any(|field| field.name == "name" && field.is_primary_identifier));
        assert!(saved
            .fields
            .iter()
            .any(|field| field.name == "note" && field.is_auxiliary_identifier));
        let copied_source = project
            .project_path
            .join("source")
            .join(saved.source_filename.unwrap());
        assert!(copied_source.is_file());
        assert!(fs::metadata(copied_source)
            .unwrap()
            .permissions()
            .readonly());

        let reopened = DatasetService::list_datasets(&project.project_path).unwrap();
        assert_eq!(reopened.len(), 1);
        assert_eq!(reopened[0].name, "任务清单");
        assert_eq!(reopened[0].fields.len(), 3);
    }

    #[test]
    fn rejects_overlapping_identifiers_without_writing() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("配置测试", directory.path()).unwrap();
        let result = DatasetService::commit_import(CommitDatasetImport {
            project_path: project.project_path.clone(),
            dataset_name: "数据".into(),
            source: DatasetImportSource::PastedText("A\nB".into()),
            deduplicate: false,
            json_pointer: None,
            primary_identifier: "内容".into(),
            auxiliary_identifiers: vec!["内容".into()],
        });
        assert!(matches!(result, Err(AppError::InvalidInput(_))));
        assert!(DatasetService::list_datasets(&project.project_path)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn deletes_dataset_and_its_readonly_source_copy() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("删除数据", directory.path()).unwrap();
        let dataset = DatasetService::commit_import(CommitDatasetImport {
            project_path: project.project_path.clone(),
            dataset_name: "待删除".into(),
            source: DatasetImportSource::PastedText("A\nB".into()),
            deduplicate: false,
            json_pointer: None,
            primary_identifier: "内容".into(),
            auxiliary_identifiers: vec![],
        })
        .unwrap();
        let source_path = project
            .project_path
            .join("source")
            .join(dataset.source_filename.as_deref().unwrap());
        assert!(source_path.exists());

        DatasetService::delete_dataset(&project.project_path, &dataset.id).unwrap();

        assert!(DatasetService::list_datasets(&project.project_path)
            .unwrap()
            .is_empty());
        assert!(!source_path.exists());
    }
}
