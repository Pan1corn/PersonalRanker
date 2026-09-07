use std::path::PathBuf;

use serde::Deserialize;

use crate::{
    domain::sort_task::{
        CreateSortTask, DragWorkspace, InitialOrder, MoveConflictPreview, MoveConflictResolution,
        SortTaskMode, SortTaskOverview,
    },
    error::AppErrorPayload,
    services::{history_service::AutosaveService, sort_task_service::SortTaskService},
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSortTaskInput {
    pub project_path: PathBuf,
    pub dataset_id: String,
    pub name: String,
    pub criteria: String,
    pub mode: SortTaskMode,
    pub initial_order: InitialOrder,
    pub matrix_comparison_percent: Option<u8>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSortTaskCriteriaInput {
    pub project_path: PathBuf,
    pub task_id: String,
    pub criteria: String,
}

#[tauri::command]
pub async fn create_sort_task(
    input: CreateSortTaskInput,
) -> Result<SortTaskOverview, AppErrorPayload> {
    tauri::async_runtime::spawn_blocking(move || {
        let project_path = input.project_path.clone();
        let task = SortTaskService::create_with_matrix_percent(
            CreateSortTask {
                project_path: input.project_path,
                dataset_id: input.dataset_id,
                name: input.name,
                criteria: input.criteria,
                mode: input.mode,
                initial_order: input.initial_order,
            },
            input.matrix_comparison_percent,
        )?;
        AutosaveService::record(&project_path, "创建排序任务")?;
        Ok::<_, crate::error::AppError>(task)
    })
    .await
    .map_err(|error| AppErrorPayload {
        code: "BACKGROUND_TASK",
        message: "创建排序任务的后台操作失败。".into(),
        detail: Some(error.to_string()),
    })?
    .map_err(Into::into)
}

#[tauri::command]
pub async fn list_sort_tasks(
    project_path: PathBuf,
) -> Result<Vec<SortTaskOverview>, AppErrorPayload> {
    tauri::async_runtime::spawn_blocking(move || SortTaskService::list(&project_path))
        .await
        .map_err(|error| AppErrorPayload {
            code: "BACKGROUND_TASK",
            message: "加载排序任务的后台操作失败。".into(),
            detail: Some(error.to_string()),
        })?
        .map_err(Into::into)
}

#[tauri::command]
pub async fn delete_sort_task(
    project_path: PathBuf,
    task_id: String,
) -> Result<(), AppErrorPayload> {
    tauri::async_runtime::spawn_blocking(move || {
        SortTaskService::delete(&project_path, &task_id)?;
        AutosaveService::record(&project_path, "删除排序任务")?;
        Ok::<_, crate::error::AppError>(())
    })
    .await
    .map_err(|error| AppErrorPayload {
        code: "BACKGROUND_TASK",
        message: "删除排序任务的后台操作失败。".into(),
        detail: Some(error.to_string()),
    })?
    .map_err(Into::into)
}

#[tauri::command]
pub async fn update_sort_task_criteria(
    input: UpdateSortTaskCriteriaInput,
) -> Result<SortTaskOverview, AppErrorPayload> {
    tauri::async_runtime::spawn_blocking(move || {
        let task =
            SortTaskService::update_criteria(&input.project_path, &input.task_id, &input.criteria)?;
        AutosaveService::record(&input.project_path, "修改排序标准")?;
        Ok::<_, crate::error::AppError>(task)
    })
    .await
    .map_err(|error| AppErrorPayload {
        code: "BACKGROUND_TASK",
        message: "修改排序标准的后台操作失败。".into(),
        detail: Some(error.to_string()),
    })?
    .map_err(Into::into)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DragWorkspaceInput {
    pub project_path: PathBuf,
    pub task_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveRankGroupInput {
    pub project_path: PathBuf,
    pub task_id: String,
    pub group_id: String,
    pub to_position: usize,
    pub conflict_resolution: Option<MoveConflictResolution>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyLocalRankOrderInput {
    pub project_path: PathBuf,
    pub task_id: String,
    pub start_position: usize,
    pub ordered_group_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeRankGroupsInput {
    pub project_path: PathBuf,
    pub task_id: String,
    pub source_group_id: String,
    pub target_group_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitRankGroupInput {
    pub project_path: PathBuf,
    pub task_id: String,
    pub group_id: String,
    pub item_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameRankGroupInput {
    pub project_path: PathBuf,
    pub task_id: String,
    pub group_id: String,
    pub name: String,
}

#[tauri::command]
pub async fn load_drag_workspace(
    input: DragWorkspaceInput,
) -> Result<DragWorkspace, AppErrorPayload> {
    run_drag_operation("加载拖拽工作台", move || {
        SortTaskService::load_drag_workspace(&input.project_path, &input.task_id)
    })
    .await
}

#[tauri::command]
pub async fn move_rank_group(input: MoveRankGroupInput) -> Result<DragWorkspace, AppErrorPayload> {
    run_drag_operation("保存新的排序位置", move || {
        let workspace = SortTaskService::move_rank_group_with_resolution(
            &input.project_path,
            &input.task_id,
            &input.group_id,
            input.to_position,
            input
                .conflict_resolution
                .unwrap_or(MoveConflictResolution::Temporary),
        )?;
        AutosaveService::record(&input.project_path, "调整拖拽排序位置")?;
        Ok(workspace)
    })
    .await
}

#[tauri::command]
pub async fn apply_local_rank_order(
    input: ApplyLocalRankOrderInput,
) -> Result<DragWorkspace, AppErrorPayload> {
    run_drag_operation("保存局部重排结果", move || {
        let workspace = SortTaskService::apply_local_rank_order(
            &input.project_path,
            &input.task_id,
            input.start_position,
            &input.ordered_group_ids,
        )?;
        AutosaveService::record(&input.project_path, "完成局部精细重排")?;
        Ok(workspace)
    })
    .await
}

#[tauri::command]
pub async fn preview_rank_group_move(
    input: MoveRankGroupInput,
) -> Result<MoveConflictPreview, AppErrorPayload> {
    tauri::async_runtime::spawn_blocking(move || {
        SortTaskService::preview_rank_group_move(
            &input.project_path,
            &input.task_id,
            &input.group_id,
            input.to_position,
        )
    })
    .await
    .map_err(|error| AppErrorPayload {
        code: "BACKGROUND_TASK",
        message: "检查拖拽冲突的后台操作失败。".into(),
        detail: Some(error.to_string()),
    })?
    .map_err(Into::into)
}

#[tauri::command]
pub async fn merge_rank_groups(
    input: MergeRankGroupsInput,
) -> Result<DragWorkspace, AppErrorPayload> {
    run_drag_operation("创建并列组", move || {
        let workspace = SortTaskService::merge_rank_groups(
            &input.project_path,
            &input.task_id,
            &input.source_group_id,
            &input.target_group_id,
        )?;
        AutosaveService::record(&input.project_path, "创建并列组")?;
        Ok(workspace)
    })
    .await
}

#[tauri::command]
pub async fn split_rank_group_item(
    input: SplitRankGroupInput,
) -> Result<DragWorkspace, AppErrorPayload> {
    run_drag_operation("拆分并列组", move || {
        let workspace = SortTaskService::split_rank_group_item(
            &input.project_path,
            &input.task_id,
            &input.group_id,
            &input.item_id,
        )?;
        AutosaveService::record(&input.project_path, "拆分并列组")?;
        Ok(workspace)
    })
    .await
}

#[tauri::command]
pub async fn rename_rank_group(
    input: RenameRankGroupInput,
) -> Result<DragWorkspace, AppErrorPayload> {
    run_drag_operation("重命名并列组", move || {
        let workspace = SortTaskService::rename_rank_group(
            &input.project_path,
            &input.task_id,
            &input.group_id,
            &input.name,
        )?;
        AutosaveService::record(&input.project_path, "重命名并列组")?;
        Ok(workspace)
    })
    .await
}

#[tauri::command]
pub async fn undo_drag_operation(
    input: DragWorkspaceInput,
) -> Result<DragWorkspace, AppErrorPayload> {
    run_drag_operation("撤销排序操作", move || {
        let workspace = SortTaskService::undo_drag_operation(&input.project_path, &input.task_id)?;
        AutosaveService::record(&input.project_path, "撤销拖拽排序操作")?;
        Ok(workspace)
    })
    .await
}

#[tauri::command]
pub async fn redo_drag_operation(
    input: DragWorkspaceInput,
) -> Result<DragWorkspace, AppErrorPayload> {
    run_drag_operation("重做排序操作", move || {
        let workspace = SortTaskService::redo_drag_operation(&input.project_path, &input.task_id)?;
        AutosaveService::record(&input.project_path, "重做拖拽排序操作")?;
        Ok(workspace)
    })
    .await
}

async fn run_drag_operation<F>(
    action: &'static str,
    operation: F,
) -> Result<DragWorkspace, AppErrorPayload>
where
    F: FnOnce() -> Result<DragWorkspace, crate::error::AppError> + Send + 'static,
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
