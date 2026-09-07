use std::path::PathBuf;

use serde::Deserialize;

use crate::{
    domain::comparison::{ComparisonDecision, ComparisonWorkspace},
    domain::history::SnapshotType,
    domain::sort_task::SortTaskMode,
    error::{AppError, AppErrorPayload},
    services::{
        comparison_service::ComparisonService,
        history_service::{AutosaveService, SnapshotService},
    },
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComparisonWorkspaceInput {
    pub project_path: PathBuf,
    pub task_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerComparisonInput {
    pub project_path: PathBuf,
    pub task_id: String,
    pub decision: ComparisonDecision,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameComparisonRankGroupInput {
    pub project_path: PathBuf,
    pub task_id: String,
    pub group_id: String,
    pub name: String,
}

#[tauri::command]
pub async fn load_comparison_workspace(
    input: ComparisonWorkspaceInput,
) -> Result<ComparisonWorkspace, AppErrorPayload> {
    run_comparison_operation("加载 1v1 工作台", move || {
        ComparisonService::load(&input.project_path, &input.task_id)
    })
    .await
}

#[tauri::command]
pub async fn answer_comparison(
    input: AnswerComparisonInput,
) -> Result<ComparisonWorkspace, AppErrorPayload> {
    run_comparison_operation("保存比较判断", move || {
        let before = ComparisonService::load(&input.project_path, &input.task_id)?;
        let was_completed = before.completed;
        let workspace =
            ComparisonService::answer(&input.project_path, &input.task_id, input.decision)?;
        let history_label = comparison_history_label(workspace.mode, "保存");
        AutosaveService::record(&input.project_path, &history_label)?;
        if !was_completed && workspace.completed {
            SnapshotService::create(
                &input.project_path,
                SnapshotType::ComparisonCompleted,
                Some(&input.task_id),
                None,
            )?;
        }
        Ok(workspace)
    })
    .await
}

#[tauri::command]
pub async fn skip_comparison(
    input: ComparisonWorkspaceInput,
) -> Result<ComparisonWorkspace, AppErrorPayload> {
    run_comparison_operation("跳过当前比较", move || {
        let workspace = ComparisonService::skip(&input.project_path, &input.task_id)?;
        let history_label = comparison_history_label(workspace.mode, "跳过");
        AutosaveService::record(&input.project_path, &history_label)?;
        Ok(workspace)
    })
    .await
}

#[tauri::command]
pub async fn undo_comparison(
    input: ComparisonWorkspaceInput,
) -> Result<ComparisonWorkspace, AppErrorPayload> {
    run_comparison_operation("撤销比较判断", move || {
        let workspace = ComparisonService::undo(&input.project_path, &input.task_id)?;
        let history_label = comparison_history_label(workspace.mode, "撤销");
        AutosaveService::record(&input.project_path, &history_label)?;
        Ok(workspace)
    })
    .await
}

#[tauri::command]
pub async fn rename_comparison_rank_group(
    input: RenameComparisonRankGroupInput,
) -> Result<ComparisonWorkspace, AppErrorPayload> {
    run_comparison_operation("重命名 1v1 并列组", move || {
        let workspace = ComparisonService::rename_group(
            &input.project_path,
            &input.task_id,
            &input.group_id,
            &input.name,
        )?;
        AutosaveService::record(&input.project_path, "重命名 1v1 并列组")?;
        Ok(workspace)
    })
    .await
}

fn comparison_history_label(mode: SortTaskMode, action: &str) -> String {
    let method = if mode == SortTaskMode::Matrix {
        "1v1 矩阵排序"
    } else {
        "1v1 传递排序"
    };
    format!("{action}{method}判断")
}

async fn run_comparison_operation<F>(
    action: &'static str,
    operation: F,
) -> Result<ComparisonWorkspace, AppErrorPayload>
where
    F: FnOnce() -> Result<ComparisonWorkspace, AppError> + Send + 'static,
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
