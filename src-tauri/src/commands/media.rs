use std::path::PathBuf;

use serde::Deserialize;

use crate::{
    domain::media::{MediaFolderImportResult, MediaPreview},
    error::AppErrorPayload,
    services::{history_service::AutosaveService, media_service::MediaService},
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaPreviewInput {
    pub project_path: PathBuf,
    pub item_id: String,
    pub field_name: String,
    #[serde(default)]
    pub allow_network: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportMediaFolderInput {
    pub project_path: PathBuf,
    pub dataset_id: String,
    pub folder_path: PathBuf,
}

#[tauri::command]
pub async fn import_media_folder(
    input: ImportMediaFolderInput,
) -> Result<MediaFolderImportResult, AppErrorPayload> {
    tauri::async_runtime::spawn_blocking(move || {
        let result = MediaService::import_folder(
            &input.project_path,
            &input.dataset_id,
            &input.folder_path,
        )?;
        AutosaveService::record(&input.project_path, "导入图片文件夹并匹配图片字段")?;
        Ok::<_, crate::error::AppError>(result)
    })
    .await
    .map_err(|error| AppErrorPayload {
        code: "BACKGROUND_TASK",
        message: "导入图片文件夹的后台任务失败。".into(),
        detail: Some(error.to_string()),
    })?
    .map_err(Into::into)
}

#[tauri::command]
pub async fn load_media_preview(input: MediaPreviewInput) -> Result<MediaPreview, AppErrorPayload> {
    tauri::async_runtime::spawn_blocking(move || {
        MediaService::preview(
            &input.project_path,
            &input.item_id,
            &input.field_name,
            input.allow_network,
        )
    })
    .await
    .map_err(|error| AppErrorPayload {
        code: "BACKGROUND_TASK",
        message: "加载图片缩略图的后台任务失败。".into(),
        detail: Some(error.to_string()),
    })?
    .map_err(Into::into)
}
