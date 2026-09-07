use std::path::PathBuf;

use serde::Deserialize;

use crate::{
    domain::history::{AutosaveStatus, ProjectSnapshot, RestoreSnapshotResult},
    error::{AppError, AppErrorPayload},
    services::history_service::{AutosaveService, SnapshotService},
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectHistoryInput {
    pub project_path: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordAutosaveInput {
    pub project_path: PathBuf,
    pub action: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreSnapshotInput {
    pub project_path: PathBuf,
    pub snapshot_id: String,
}

#[tauri::command]
pub async fn get_autosave_status(
    input: ProjectHistoryInput,
) -> Result<AutosaveStatus, AppErrorPayload> {
    run_history_operation("读取自动保存状态", move || {
        AutosaveService::status(&input.project_path)
    })
    .await
}

#[tauri::command]
pub async fn record_autosave(
    input: RecordAutosaveInput,
) -> Result<AutosaveStatus, AppErrorPayload> {
    run_history_operation("自动保存", move || {
        AutosaveService::record(&input.project_path, &input.action)
    })
    .await
}

#[tauri::command]
pub async fn list_project_snapshots(
    input: ProjectHistoryInput,
) -> Result<Vec<ProjectSnapshot>, AppErrorPayload> {
    run_history_operation("读取历史快照", move || {
        SnapshotService::list(&input.project_path)
    })
    .await
}

#[tauri::command]
pub async fn restore_project_snapshot(
    input: RestoreSnapshotInput,
) -> Result<RestoreSnapshotResult, AppErrorPayload> {
    run_history_operation("恢复历史快照", move || {
        SnapshotService::restore(&input.project_path, &input.snapshot_id)
    })
    .await
}

async fn run_history_operation<T, F>(
    action: &'static str,
    operation: F,
) -> Result<T, AppErrorPayload>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|error| AppErrorPayload {
            code: "BACKGROUND_TASK",
            message: format!("{action}的后台操作失败。"),
            detail: Some(error.to_string()),
        })?
        .map_err(Into::into)
}
