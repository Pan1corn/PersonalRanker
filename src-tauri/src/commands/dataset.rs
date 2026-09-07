use std::path::PathBuf;

use serde::Deserialize;

use crate::{
    domain::dataset::{CommitDatasetImport, DatasetImportSource, DatasetOverview},
    domain::history::SnapshotType,
    error::AppErrorPayload,
    services::{
        dataset_service::DatasetService,
        history_service::{AutosaveService, SnapshotService},
    },
};

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CommitSourceInput {
    File { path: PathBuf },
    PastedText { content: String },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitImportInput {
    pub project_path: PathBuf,
    pub dataset_name: String,
    pub source: CommitSourceInput,
    #[serde(default)]
    pub deduplicate: bool,
    pub json_pointer: Option<String>,
    pub primary_identifier: String,
    #[serde(default)]
    pub auxiliary_identifiers: Vec<String>,
}

impl From<CommitSourceInput> for DatasetImportSource {
    fn from(value: CommitSourceInput) -> Self {
        match value {
            CommitSourceInput::File { path } => Self::File(path),
            CommitSourceInput::PastedText { content } => Self::PastedText(content),
        }
    }
}

#[tauri::command]
pub async fn commit_dataset_import(
    input: CommitImportInput,
) -> Result<DatasetOverview, AppErrorPayload> {
    tauri::async_runtime::spawn_blocking(move || {
        let project_path = input.project_path.clone();
        let dataset = DatasetService::commit_import(CommitDatasetImport {
            project_path: input.project_path,
            dataset_name: input.dataset_name,
            source: input.source.into(),
            deduplicate: input.deduplicate,
            json_pointer: input.json_pointer,
            primary_identifier: input.primary_identifier,
            auxiliary_identifiers: input.auxiliary_identifiers,
        })?;
        AutosaveService::record(&project_path, "导入数据并保存字段配置")?;
        SnapshotService::create(
            &project_path,
            SnapshotType::ImportCompleted,
            None,
            Some(&dataset.id),
        )?;
        Ok::<_, crate::error::AppError>(dataset)
    })
    .await
    .map_err(|error| AppErrorPayload {
        code: "BACKGROUND_TASK",
        message: "保存导入数据的后台任务失败。".into(),
        detail: Some(error.to_string()),
    })?
    .map_err(Into::into)
}

#[tauri::command]
pub async fn list_datasets(project_path: PathBuf) -> Result<Vec<DatasetOverview>, AppErrorPayload> {
    tauri::async_runtime::spawn_blocking(move || DatasetService::list_datasets(&project_path))
        .await
        .map_err(|error| AppErrorPayload {
            code: "BACKGROUND_TASK",
            message: "加载项目数据的后台任务失败。".into(),
            detail: Some(error.to_string()),
        })?
        .map_err(Into::into)
}

#[tauri::command]
pub async fn delete_dataset(
    project_path: PathBuf,
    dataset_id: String,
) -> Result<(), AppErrorPayload> {
    tauri::async_runtime::spawn_blocking(move || {
        DatasetService::delete_dataset(&project_path, &dataset_id)?;
        AutosaveService::record(&project_path, "删除数据表")?;
        Ok::<_, crate::error::AppError>(())
    })
    .await
    .map_err(|error| AppErrorPayload {
        code: "BACKGROUND_TASK",
        message: "删除数据表的后台操作失败。".into(),
        detail: Some(error.to_string()),
    })?
    .map_err(Into::into)
}
