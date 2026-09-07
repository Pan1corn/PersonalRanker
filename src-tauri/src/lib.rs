//! Tauri 应用装配入口。
//! 这里注册前端可调用的命令，并在窗口正常关闭时结束所有项目的自动保存会话。

mod commands;
mod domain;
mod error;
mod repository;
mod services;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(services::history_service::OpenProjectRegistry::default())
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                // 会话标记用于区分正常退出和崩溃；关闭窗口时必须逐个清理已打开项目。
                let registry = window
                    .app_handle()
                    .state::<services::history_service::OpenProjectRegistry>();
                for project_path in registry.paths() {
                    let _ = services::history_service::AutosaveService::end_session(&project_path);
                }
            }
        })
        // commands 层是唯一 IPC 边界，输入校验和业务事务继续下沉到 service/repository。
        .invoke_handler(tauri::generate_handler![
            commands::comparison::load_comparison_workspace,
            commands::comparison::answer_comparison,
            commands::comparison::skip_comparison,
            commands::comparison::undo_comparison,
            commands::comparison::rename_comparison_rank_group,
            commands::dataset::commit_dataset_import,
            commands::dataset::list_datasets,
            commands::dataset::delete_dataset,
            commands::import::preview_import_file,
            commands::import::preview_pasted_text,
            commands::import::inspect_json_array_nodes,
            commands::media::load_media_preview,
            commands::media::import_media_folder,
            commands::project::create_project,
            commands::project::open_project,
            commands::project::close_project,
            commands::history::get_autosave_status,
            commands::history::record_autosave,
            commands::history::list_project_snapshots,
            commands::history::restore_project_snapshot,
            commands::result::load_result_preview,
            commands::result::confirm_sort_result,
            commands::result::unlock_sort_result,
            commands::result::write_rank_field,
            commands::result::export_target_exists,
            commands::result::export_sort_result,
            commands::scoring::preview_scores,
            commands::scoring::load_score_config,
            commands::scoring::save_score_config,
            commands::scoring::write_scores,
            commands::slider::load_slider_workspace,
            commands::slider::confirm_slider_value,
            commands::sort_task::create_sort_task,
            commands::sort_task::list_sort_tasks,
            commands::sort_task::delete_sort_task,
            commands::sort_task::update_sort_task_criteria,
            commands::sort_task::load_drag_workspace,
            commands::sort_task::move_rank_group,
            commands::sort_task::apply_local_rank_order,
            commands::sort_task::preview_rank_group_move,
            commands::sort_task::merge_rank_groups,
            commands::sort_task::split_rank_group_item,
            commands::sort_task::rename_rank_group,
            commands::sort_task::undo_drag_operation,
            commands::sort_task::redo_drag_operation
        ])
        .run(tauri::generate_context!())
        .expect("启动主观排序器失败");
}
