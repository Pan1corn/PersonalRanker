use std::path::PathBuf;

use serde::Deserialize;

use crate::{
    domain::history::SnapshotType,
    domain::scoring::{ScoreConfig, ScorePreview, ScorePreviewRequest},
    error::{AppError, AppErrorPayload},
    services::{
        history_service::{AutosaveService, SnapshotService},
        scoring_service::ScoringService,
    },
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreInput {
    pub project_path: PathBuf,
    pub task_id: String,
    pub config: ScoreConfig,
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreConfigInput {
    pub project_path: PathBuf,
    pub task_id: String,
}

#[tauri::command]
pub async fn load_score_config(
    input: ScoreConfigInput,
) -> Result<Option<ScoreConfig>, AppErrorPayload> {
    run_score_config_operation("读取评分配置", move || {
        ScoringService::load_config(&input.project_path, &input.task_id)
    })
    .await
}

#[tauri::command]
pub async fn save_score_config(input: ScoreInput) -> Result<ScoreConfig, AppErrorPayload> {
    run_score_config_operation("自动保存评分配置", move || {
        let config =
            ScoringService::save_config(&input.project_path, &input.task_id, &input.config)?;
        AutosaveService::record(&input.project_path, "保存评分配置")?;
        Ok(config)
    })
    .await
}

#[tauri::command]
pub async fn preview_scores(input: ScoreInput) -> Result<ScorePreview, AppErrorPayload> {
    run_score_operation("生成评分预览", move || {
        ScoringService::preview(&ScorePreviewRequest {
            project_path: input.project_path,
            task_id: input.task_id,
            config: input.config,
        })
    })
    .await
}

#[tauri::command]
pub async fn write_scores(input: ScoreInput) -> Result<ScorePreview, AppErrorPayload> {
    run_score_operation("写入评分字段", move || {
        let preview = ScoringService::write(
            &ScorePreviewRequest {
                project_path: input.project_path.clone(),
                task_id: input.task_id.clone(),
                config: input.config,
            },
            input.overwrite,
        )?;
        AutosaveService::record(&input.project_path, "确认并写入评分")?;
        SnapshotService::create(
            &input.project_path,
            SnapshotType::ScoreConfirmed,
            Some(&input.task_id),
            None,
        )?;
        Ok(preview)
    })
    .await
}

async fn run_score_config_operation<T, F>(
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

async fn run_score_operation<F>(
    action: &'static str,
    operation: F,
) -> Result<ScorePreview, AppErrorPayload>
where
    F: FnOnce() -> Result<ScorePreview, AppError> + Send + 'static,
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
