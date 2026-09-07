use std::path::PathBuf;

use serde::Deserialize;

use crate::{
    domain::{history::OpenProjectResult, project::ProjectMetadata},
    error::AppErrorPayload,
    services::{
        history_service::{AutosaveService, OpenProjectRegistry},
        project_service::ProjectService,
    },
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectInput {
    pub name: String,
    pub parent_directory: PathBuf,
}

#[tauri::command]
pub async fn create_project(
    input: CreateProjectInput,
    registry: tauri::State<'_, OpenProjectRegistry>,
) -> Result<ProjectMetadata, AppErrorPayload> {
    let project = tauri::async_runtime::spawn_blocking(move || {
        ProjectService::create(&input.name, &input.parent_directory)
    })
    .await
    .map_err(|error| AppErrorPayload {
        code: "BACKGROUND_TASK",
        message: "创建项目的后台任务失败。".into(),
        detail: Some(error.to_string()),
    })?
    .map_err(AppErrorPayload::from)?;
    AutosaveService::begin_session(&project.project_path).map_err(AppErrorPayload::from)?;
    registry.register(project.project_path.clone());
    Ok(project)
}

#[tauri::command]
pub async fn open_project(
    project_path: PathBuf,
    registry: tauri::State<'_, OpenProjectRegistry>,
) -> Result<OpenProjectResult, AppErrorPayload> {
    let project = tauri::async_runtime::spawn_blocking(move || ProjectService::open(&project_path))
        .await
        .map_err(|error| AppErrorPayload {
            code: "BACKGROUND_TASK",
            message: "打开项目的后台任务失败。".into(),
            detail: Some(error.to_string()),
        })?
        .map_err(AppErrorPayload::from)?;
    let autosave =
        AutosaveService::begin_session(&project.project_path).map_err(AppErrorPayload::from)?;
    registry.register(project.project_path.clone());
    Ok(OpenProjectResult { project, autosave })
}

#[tauri::command]
pub async fn close_project(
    project_path: PathBuf,
    registry: tauri::State<'_, OpenProjectRegistry>,
) -> Result<(), AppErrorPayload> {
    let saved_path = project_path.clone();
    tauri::async_runtime::spawn_blocking(move || AutosaveService::end_session(&saved_path))
        .await
        .map_err(|error| AppErrorPayload {
            code: "BACKGROUND_TASK",
            message: "关闭项目的后台操作失败。".into(),
            detail: Some(error.to_string()),
        })?
        .map_err(AppErrorPayload::from)?;
    registry.unregister(&project_path);
    Ok(())
}
