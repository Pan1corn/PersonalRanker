use std::path::PathBuf;

use serde::Deserialize;

use crate::{
    domain::import::{ImportResult, JsonArrayNode},
    error::{AppError, AppErrorPayload},
    services::import_service::{ImportOptions, ImportService},
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportFileInput {
    pub path: PathBuf,
    #[serde(default)]
    pub deduplicate: bool,
    pub json_pointer: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportTextInput {
    pub content: String,
    #[serde(default)]
    pub deduplicate: bool,
}

#[tauri::command]
pub async fn preview_import_file(input: ImportFileInput) -> Result<ImportResult, AppErrorPayload> {
    tauri::async_runtime::spawn_blocking(move || {
        ImportService::import_file(
            &input.path,
            ImportOptions {
                deduplicate: input.deduplicate,
                json_pointer: input.json_pointer.as_deref(),
            },
        )
    })
    .await
    .map_err(|error| AppErrorPayload {
        code: "BACKGROUND_TASK",
        message: "导入文件的后台任务失败。".into(),
        detail: Some(error.to_string()),
    })?
    .map_err(|error| AppErrorPayload::from(AppError::from(error)))
}

#[tauri::command]
pub async fn preview_pasted_text(input: ImportTextInput) -> Result<ImportResult, AppErrorPayload> {
    tauri::async_runtime::spawn_blocking(move || {
        ImportService::import_text(&input.content, input.deduplicate)
    })
    .await
    .map_err(|error| AppErrorPayload {
        code: "BACKGROUND_TASK",
        message: "解析粘贴文本的后台任务失败。".into(),
        detail: Some(error.to_string()),
    })?
    .map_err(|error| AppErrorPayload::from(AppError::from(error)))
}

#[tauri::command]
pub async fn inspect_json_array_nodes(
    path: PathBuf,
) -> Result<Vec<JsonArrayNode>, AppErrorPayload> {
    tauri::async_runtime::spawn_blocking(move || {
        ImportService::list_json_array_nodes_from_file(&path)
    })
    .await
    .map_err(|error| AppErrorPayload {
        code: "BACKGROUND_TASK",
        message: "检查 JSON 节点的后台任务失败。".into(),
        detail: Some(error.to_string()),
    })?
    .map_err(|error| AppErrorPayload::from(AppError::from(error)))
}
