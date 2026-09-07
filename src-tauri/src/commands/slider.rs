use std::path::PathBuf;

use serde::Deserialize;

use crate::{
    domain::{history::SnapshotType, slider::SliderWorkspace},
    error::{AppError, AppErrorPayload},
    services::{
        history_service::{AutosaveService, SnapshotService},
        slider_service::SliderService,
    },
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SliderWorkspaceInput {
    pub project_path: PathBuf,
    pub task_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmSliderValueInput {
    pub project_path: PathBuf,
    pub task_id: String,
    pub value: f64,
}

#[tauri::command]
pub async fn load_slider_workspace(
    input: SliderWorkspaceInput,
) -> Result<SliderWorkspace, AppErrorPayload> {
    run_slider_operation("加载滑杆排序工作台", move || {
        SliderService::load(&input.project_path, &input.task_id)
    })
    .await
}

#[tauri::command]
pub async fn confirm_slider_value(
    input: ConfirmSliderValueInput,
) -> Result<SliderWorkspace, AppErrorPayload> {
    run_slider_operation("保存滑杆位置", move || {
        let was_completed = SliderService::load(&input.project_path, &input.task_id)?.completed;
        let workspace = SliderService::confirm(&input.project_path, &input.task_id, input.value)?;
        AutosaveService::record(&input.project_path, "保存滑杆排序评分")?;
        if !was_completed && workspace.completed {
            SnapshotService::create(
                &input.project_path,
                SnapshotType::SliderCompleted,
                Some(&input.task_id),
                None,
            )?;
        }
        Ok(workspace)
    })
    .await
}

async fn run_slider_operation<F>(
    action: &'static str,
    operation: F,
) -> Result<SliderWorkspace, AppErrorPayload>
where
    F: FnOnce() -> Result<SliderWorkspace, AppError> + Send + 'static,
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
