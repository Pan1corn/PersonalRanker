use std::path::PathBuf;

use serde::Deserialize;

use crate::{
    domain::history::SnapshotType,
    domain::result::{
        ExportFormat, ExportOrder, ExportResultReceipt, ExportResultRequest, ResultPreview,
    },
    domain::scoring::RankRule,
    error::{AppError, AppErrorPayload},
    services::{
        history_service::{AutosaveService, SnapshotService},
        result_service::ResultService,
    },
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultInput {
    pub project_path: PathBuf,
    pub task_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteRankFieldInput {
    pub project_path: PathBuf,
    pub task_id: String,
    pub field_name: String,
    #[serde(default)]
    pub overwrite: bool,
    #[serde(default)]
    pub rank_rule: RankRule,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResultInput {
    pub project_path: PathBuf,
    pub task_id: String,
    pub target_path: PathBuf,
    pub format: ExportFormat,
    pub order: ExportOrder,
    pub selected_fields: Vec<String>,
    pub include_rank: bool,
    pub rank_field_name: String,
    pub include_original_index: bool,
    #[serde(default)]
    pub overwrite: bool,
}

#[tauri::command]
pub async fn load_result_preview(input: ResultInput) -> Result<ResultPreview, AppErrorPayload> {
    run_result_operation("加载结果预览", move || {
        ResultService::load(&input.project_path, &input.task_id)
    })
    .await
}

#[tauri::command]
pub async fn confirm_sort_result(input: ResultInput) -> Result<ResultPreview, AppErrorPayload> {
    run_result_operation("确认排序结果", move || {
        let should_snapshot = ResultService::load(&input.project_path, &input.task_id)?.can_confirm;
        let preview = ResultService::confirm(&input.project_path, &input.task_id)?;
        AutosaveService::record(&input.project_path, "确认并锁定排序结果")?;
        if should_snapshot {
            SnapshotService::create(
                &input.project_path,
                SnapshotType::SortConfirmed,
                Some(&input.task_id),
                None,
            )?;
        }
        Ok(preview)
    })
    .await
}

#[tauri::command]
pub async fn unlock_sort_result(input: ResultInput) -> Result<ResultPreview, AppErrorPayload> {
    run_result_operation("解锁排序结果", move || {
        let preview = ResultService::unlock(&input.project_path, &input.task_id)?;
        AutosaveService::record(&input.project_path, "解锁排序结果")?;
        Ok(preview)
    })
    .await
}

#[tauri::command]
pub async fn write_rank_field(
    input: WriteRankFieldInput,
) -> Result<ResultPreview, AppErrorPayload> {
    run_result_operation("写入排名字段", move || {
        let preview = ResultService::write_rank_with_rule(
            &input.project_path,
            &input.task_id,
            &input.field_name,
            input.overwrite,
            input.rank_rule,
        )?;
        AutosaveService::record(&input.project_path, "写入排名字段")?;
        Ok(preview)
    })
    .await
}

#[tauri::command]
pub async fn export_target_exists(path: PathBuf) -> bool {
    ResultService::target_exists(&path)
}

#[tauri::command]
pub async fn export_sort_result(
    input: ExportResultInput,
) -> Result<ExportResultReceipt, AppErrorPayload> {
    run_result_operation("导出排序结果", move || {
        let project_path = input.project_path.clone();
        let task_id = input.task_id.clone();
        let receipt = ResultService::export(&ExportResultRequest {
            project_path: input.project_path,
            task_id: input.task_id,
            target_path: input.target_path,
            format: input.format,
            order: input.order,
            selected_fields: input.selected_fields,
            include_rank: input.include_rank,
            rank_field_name: input.rank_field_name,
            include_original_index: input.include_original_index,
            overwrite: input.overwrite,
        })?;
        AutosaveService::record(&project_path, "正式导出排序结果")?;
        SnapshotService::create(
            &project_path,
            SnapshotType::ExportCompleted,
            Some(&task_id),
            None,
        )?;
        Ok(receipt)
    })
    .await
}

async fn run_result_operation<T, F>(
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
