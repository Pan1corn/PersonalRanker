//! 排序任务用例层。
//! 负责项目校验和用户输入规范化，具体排序、并列组与历史记录由 repository 在事务中完成。

use std::path::Path;

use crate::{
    domain::sort_task::{
        CreateSortTask, DragWorkspace, MoveConflictPreview, MoveConflictResolution,
        SortTaskOverview,
    },
    error::AppError,
    repository::project_repository::ProjectRepository,
    services::project_service::ProjectService,
};

pub struct SortTaskService;

impl SortTaskService {
    #[cfg(test)]
    pub fn create(request: CreateSortTask) -> Result<SortTaskOverview, AppError> {
        Self::create_with_matrix_percent(request, None)
    }

    pub fn create_with_matrix_percent(
        request: CreateSortTask,
        matrix_comparison_percent: Option<u8>,
    ) -> Result<SortTaskOverview, AppError> {
        let name = request.name.trim();
        let criteria = request.criteria.trim();
        if name.is_empty() {
            return Err(AppError::InvalidInput("排序任务名称不能为空".into()));
        }
        if criteria.is_empty() {
            return Err(AppError::InvalidInput(
                "请说明什么样的条目应该排在前面".into(),
            ));
        }
        ProjectService::open(&request.project_path)?;
        let mut repository = ProjectRepository::open(&request.project_path.join("project.sqlite"))?;
        repository.create_sort_task(
            &CreateSortTask {
                name: name.to_owned(),
                criteria: criteria.to_owned(),
                ..request
            },
            matrix_comparison_percent,
        )
    }

    pub fn list(project_path: &Path) -> Result<Vec<SortTaskOverview>, AppError> {
        ProjectService::open(project_path)?;
        ProjectRepository::open(&project_path.join("project.sqlite"))?.list_sort_tasks()
    }

    pub fn delete(project_path: &Path, task_id: &str) -> Result<(), AppError> {
        ProjectService::open(project_path)?;
        ProjectRepository::open(&project_path.join("project.sqlite"))?.delete_sort_task(task_id)
    }

    pub fn update_criteria(
        project_path: &Path,
        task_id: &str,
        criteria: &str,
    ) -> Result<SortTaskOverview, AppError> {
        let criteria = criteria.trim();
        if criteria.is_empty() || criteria.chars().count() > 500 {
            return Err(AppError::InvalidInput("排序标准应为 1—500 个字符".into()));
        }
        ProjectService::open(project_path)?;
        ProjectRepository::open(&project_path.join("project.sqlite"))?
            .update_sort_task_criteria(task_id, criteria)
    }

    pub fn load_drag_workspace(
        project_path: &Path,
        task_id: &str,
    ) -> Result<DragWorkspace, AppError> {
        ProjectService::open(project_path)?;
        ProjectRepository::open(&project_path.join("project.sqlite"))?.load_drag_workspace(task_id)
    }

    #[cfg(test)]
    pub fn move_rank_group(
        project_path: &Path,
        task_id: &str,
        group_id: &str,
        to_position: usize,
    ) -> Result<DragWorkspace, AppError> {
        Self::move_rank_group_with_resolution(
            project_path,
            task_id,
            group_id,
            to_position,
            MoveConflictResolution::Temporary,
        )
    }

    pub fn move_rank_group_with_resolution(
        project_path: &Path,
        task_id: &str,
        group_id: &str,
        to_position: usize,
        resolution: MoveConflictResolution,
    ) -> Result<DragWorkspace, AppError> {
        ProjectService::open(project_path)?;
        let mut repository = ProjectRepository::open(&project_path.join("project.sqlite"))?;
        // 先按用户选择处理与既有 1v1 关系的冲突，再提交最终组顺序。
        repository.resolve_move_conflicts(task_id, group_id, to_position, resolution)?;
        repository.move_rank_group(task_id, group_id, to_position)
    }

    pub fn apply_local_rank_order(
        project_path: &Path,
        task_id: &str,
        start_position: usize,
        ordered_group_ids: &[String],
    ) -> Result<DragWorkspace, AppError> {
        ProjectService::open(project_path)?;
        ProjectRepository::open(&project_path.join("project.sqlite"))?.apply_local_rank_order(
            task_id,
            start_position,
            ordered_group_ids,
        )
    }

    pub fn preview_rank_group_move(
        project_path: &Path,
        task_id: &str,
        group_id: &str,
        to_position: usize,
    ) -> Result<MoveConflictPreview, AppError> {
        ProjectService::open(project_path)?;
        ProjectRepository::open(&project_path.join("project.sqlite"))?.preview_rank_group_move(
            task_id,
            group_id,
            to_position,
        )
    }

    pub fn merge_rank_groups(
        project_path: &Path,
        task_id: &str,
        source_group_id: &str,
        target_group_id: &str,
    ) -> Result<DragWorkspace, AppError> {
        ProjectService::open(project_path)?;
        ProjectRepository::open(&project_path.join("project.sqlite"))?.merge_rank_groups(
            task_id,
            source_group_id,
            target_group_id,
        )
    }

    pub fn split_rank_group_item(
        project_path: &Path,
        task_id: &str,
        group_id: &str,
        item_id: &str,
    ) -> Result<DragWorkspace, AppError> {
        ProjectService::open(project_path)?;
        ProjectRepository::open(&project_path.join("project.sqlite"))?
            .split_rank_group_item(task_id, group_id, item_id)
    }

    pub fn rename_rank_group(
        project_path: &Path,
        task_id: &str,
        group_id: &str,
        name: &str,
    ) -> Result<DragWorkspace, AppError> {
        ProjectService::open(project_path)?;
        ProjectRepository::open(&project_path.join("project.sqlite"))?
            .rename_rank_group(task_id, group_id, name)
    }

    pub fn undo_drag_operation(
        project_path: &Path,
        task_id: &str,
    ) -> Result<DragWorkspace, AppError> {
        ProjectService::open(project_path)?;
        ProjectRepository::open(&project_path.join("project.sqlite"))?.undo_drag_operation(task_id)
    }

    pub fn redo_drag_operation(
        project_path: &Path,
        task_id: &str,
    ) -> Result<DragWorkspace, AppError> {
        ProjectService::open(project_path)?;
        ProjectRepository::open(&project_path.join("project.sqlite"))?.redo_drag_operation(task_id)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::Value;

    use crate::{
        domain::{
            dataset::{CommitDatasetImport, DatasetImportSource},
            sort_task::{InitialOrder, SortDirection, SortTaskMode},
        },
        repository::project_repository::ProjectRepository,
        services::{dataset_service::DatasetService, result_service::ResultService},
    };

    use super::*;

    #[test]
    fn creates_multiple_tasks_and_single_member_rank_groups() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("排序任务测试", directory.path()).unwrap();
        let source = directory.path().join("items.csv");
        fs::write(
            &source,
            "name,score,date\nA,2,2026-08-02\nB,1,2026-08-01\nC,3,2026-08-03",
        )
        .unwrap();
        let dataset = DatasetService::commit_import(CommitDatasetImport {
            project_path: project.project_path.clone(),
            dataset_name: "候选项".into(),
            source: DatasetImportSource::File(source),
            deduplicate: false,
            json_pointer: None,
            primary_identifier: "name".into(),
            auxiliary_identifiers: vec!["score".into()],
        })
        .unwrap();

        let imported = SortTaskService::create(CreateSortTask {
            project_path: project.project_path.clone(),
            dataset_id: dataset.id.clone(),
            name: "导入顺序".into(),
            criteria: "更重要的排在前面".into(),
            mode: SortTaskMode::Drag,
            initial_order: InitialOrder::Import,
        })
        .unwrap();
        let by_score = SortTaskService::create(CreateSortTask {
            project_path: project.project_path.clone(),
            dataset_id: dataset.id.clone(),
            name: "按分数初排".into(),
            criteria: "分数更高的排在前面".into(),
            mode: SortTaskMode::Matrix,
            initial_order: InitialOrder::Field {
                field_name: "score".into(),
                direction: SortDirection::Descending,
            },
        })
        .unwrap();
        let by_slider = SortTaskService::create(CreateSortTask {
            project_path: project.project_path.clone(),
            dataset_id: dataset.id,
            name: "滑杆评分".into(),
            criteria: "评分更高的排在前面".into(),
            mode: SortTaskMode::Slider,
            initial_order: InitialOrder::Import,
        })
        .unwrap();

        assert_eq!(imported.rank_group_count, 3);
        assert_eq!(by_score.rank_group_count, 3);
        assert_eq!(by_score.mode, SortTaskMode::Matrix);
        assert_eq!(by_slider.mode, SortTaskMode::Slider);
        let repository =
            ProjectRepository::open(&project.project_path.join("project.sqlite")).unwrap();
        assert_eq!(repository.ranked_item_ids(&imported.id).unwrap().len(), 3);
        assert_eq!(
            repository
                .ranked_field_values(&by_score.id, "score")
                .unwrap(),
            [
                Value::String("3".into()),
                Value::String("2".into()),
                Value::String("1".into())
            ]
        );
        let reopened = SortTaskService::list(&project.project_path).unwrap();
        assert_eq!(reopened.len(), 3);
        let updated = SortTaskService::update_criteria(
            &project.project_path,
            &imported.id,
            "  新的排序标准  ",
        )
        .unwrap();
        assert_eq!(updated.criteria, "新的排序标准");
        assert!(matches!(
            SortTaskService::update_criteria(&project.project_path, &imported.id, " "),
            Err(AppError::InvalidInput(_))
        ));
        assert_eq!(
            reopened
                .iter()
                .find(|task| task.id == by_score.id)
                .unwrap()
                .initial_order,
            by_score.initial_order
        );
        assert_eq!(
            reopened
                .iter()
                .find(|task| task.id == by_slider.id)
                .unwrap()
                .mode,
            SortTaskMode::Slider
        );
    }

    #[test]
    fn creates_task_from_confirmed_result_with_tie_groups() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("结果初排", directory.path()).unwrap();
        let dataset = DatasetService::commit_import(CommitDatasetImport {
            project_path: project.project_path.clone(),
            dataset_name: "候选项".into(),
            source: DatasetImportSource::PastedText("A\nB\nC".into()),
            deduplicate: false,
            json_pointer: None,
            primary_identifier: "内容".into(),
            auxiliary_identifiers: vec![],
        })
        .unwrap();
        let source = SortTaskService::create(CreateSortTask {
            project_path: project.project_path.clone(),
            dataset_id: dataset.id.clone(),
            name: "来源".into(),
            criteria: "重要性".into(),
            mode: SortTaskMode::Drag,
            initial_order: InitialOrder::Import,
        })
        .unwrap();
        let workspace =
            SortTaskService::load_drag_workspace(&project.project_path, &source.id).unwrap();
        SortTaskService::merge_rank_groups(
            &project.project_path,
            &source.id,
            &workspace.groups[1].group_id,
            &workspace.groups[0].group_id,
        )
        .unwrap();
        ResultService::confirm(&project.project_path, &source.id).unwrap();

        let copied = SortTaskService::create(CreateSortTask {
            project_path: project.project_path.clone(),
            dataset_id: dataset.id,
            name: "复用结果".into(),
            criteria: "新的判断标准".into(),
            mode: SortTaskMode::Drag,
            initial_order: InitialOrder::TaskResult { task_id: source.id },
        })
        .unwrap();
        let copied_workspace =
            SortTaskService::load_drag_workspace(&project.project_path, &copied.id).unwrap();
        assert_eq!(copied_workspace.groups.len(), 2);
        assert_eq!(copied_workspace.groups[0].items.len(), 2);
        assert_eq!(workspace_labels(&copied_workspace), ["A", "B", "C"]);
        SortTaskService::delete(&project.project_path, &copied.id).unwrap();
        assert_eq!(
            SortTaskService::list(&project.project_path).unwrap().len(),
            1
        );
    }

    #[test]
    fn applies_local_range_as_one_undoable_operation() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("局部重排", directory.path()).unwrap();
        let dataset = DatasetService::commit_import(CommitDatasetImport {
            project_path: project.project_path.clone(),
            dataset_name: "条目".into(),
            source: DatasetImportSource::PastedText("A\nB\nC\nD".into()),
            deduplicate: false,
            json_pointer: None,
            primary_identifier: "内容".into(),
            auxiliary_identifiers: vec![],
        })
        .unwrap();
        let task = SortTaskService::create(CreateSortTask {
            project_path: project.project_path.clone(),
            dataset_id: dataset.id,
            name: "排序".into(),
            criteria: "重要性".into(),
            mode: SortTaskMode::Drag,
            initial_order: InitialOrder::Import,
        })
        .unwrap();
        let initial =
            SortTaskService::load_drag_workspace(&project.project_path, &task.id).unwrap();
        let reversed = initial.groups[1..=2]
            .iter()
            .rev()
            .map(|group| group.group_id.clone())
            .collect::<Vec<_>>();
        let reordered =
            SortTaskService::apply_local_rank_order(&project.project_path, &task.id, 1, &reversed)
                .unwrap();
        assert_eq!(workspace_labels(&reordered), ["A", "C", "B", "D"]);
        let undone = SortTaskService::undo_drag_operation(&project.project_path, &task.id).unwrap();
        assert_eq!(workspace_labels(&undone), ["A", "B", "C", "D"]);
        let redone = SortTaskService::redo_drag_operation(&project.project_path, &task.id).unwrap();
        assert_eq!(workspace_labels(&redone), ["A", "C", "B", "D"]);
    }

    #[test]
    fn rejects_non_numeric_or_date_initial_order_field() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("字段校验", directory.path()).unwrap();
        let dataset = DatasetService::commit_import(CommitDatasetImport {
            project_path: project.project_path.clone(),
            dataset_name: "文本".into(),
            source: DatasetImportSource::PastedText("A\nB".into()),
            deduplicate: false,
            json_pointer: None,
            primary_identifier: "内容".into(),
            auxiliary_identifiers: vec![],
        })
        .unwrap();
        let result = SortTaskService::create(CreateSortTask {
            project_path: project.project_path,
            dataset_id: dataset.id,
            name: "错误初排".into(),
            criteria: "测试".into(),
            mode: SortTaskMode::Drag,
            initial_order: InitialOrder::Field {
                field_name: "内容".into(),
                direction: SortDirection::Ascending,
            },
        });
        assert!(matches!(result, Err(AppError::InvalidInput(_))));
    }

    #[test]
    fn moves_persists_undoes_and_redoes_drag_order() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("拖拽持久化", directory.path()).unwrap();
        let dataset = DatasetService::commit_import(CommitDatasetImport {
            project_path: project.project_path.clone(),
            dataset_name: "待办事项".into(),
            source: DatasetImportSource::PastedText("A\nB\nC".into()),
            deduplicate: false,
            json_pointer: None,
            primary_identifier: "内容".into(),
            auxiliary_identifiers: vec![],
        })
        .unwrap();
        let task = SortTaskService::create(CreateSortTask {
            project_path: project.project_path.clone(),
            dataset_id: dataset.id,
            name: "优先级".into(),
            criteria: "越重要越靠前".into(),
            mode: SortTaskMode::Drag,
            initial_order: InitialOrder::Import,
        })
        .unwrap();

        let initial =
            SortTaskService::load_drag_workspace(&project.project_path, &task.id).unwrap();
        assert_eq!(workspace_labels(&initial), ["A", "B", "C"]);
        assert!(!initial.can_undo);
        let last_group = initial.items[2].group_id.clone();
        let second_group = initial.items[1].group_id.clone();

        let moved =
            SortTaskService::move_rank_group(&project.project_path, &task.id, &last_group, 0)
                .unwrap();
        assert_eq!(workspace_labels(&moved), ["C", "A", "B"]);
        assert_eq!(moved.items[0].original_position, 2);
        assert_eq!(moved.items[1].original_position, 0);
        assert!(moved.can_undo);
        assert!(!moved.can_redo);

        let reopened =
            SortTaskService::load_drag_workspace(&project.project_path, &task.id).unwrap();
        assert_eq!(workspace_labels(&reopened), ["C", "A", "B"]);

        let undone = SortTaskService::undo_drag_operation(&project.project_path, &task.id).unwrap();
        assert_eq!(workspace_labels(&undone), ["A", "B", "C"]);
        assert!(undone.can_redo);
        let redone = SortTaskService::redo_drag_operation(&project.project_path, &task.id).unwrap();
        assert_eq!(workspace_labels(&redone), ["C", "A", "B"]);

        SortTaskService::undo_drag_operation(&project.project_path, &task.id).unwrap();
        let branched =
            SortTaskService::move_rank_group(&project.project_path, &task.id, &second_group, 0)
                .unwrap();
        assert_eq!(workspace_labels(&branched), ["B", "A", "C"]);
        assert!(!branched.can_redo);
    }

    #[test]
    fn handles_one_thousand_drag_items() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("千条拖拽", directory.path()).unwrap();
        let content = (1..=1000)
            .map(|index| format!("条目{index:04}"))
            .collect::<Vec<_>>()
            .join("\n");
        let dataset = DatasetService::commit_import(CommitDatasetImport {
            project_path: project.project_path.clone(),
            dataset_name: "千条数据".into(),
            source: DatasetImportSource::PastedText(content),
            deduplicate: false,
            json_pointer: None,
            primary_identifier: "内容".into(),
            auxiliary_identifiers: vec![],
        })
        .unwrap();
        let task = SortTaskService::create(CreateSortTask {
            project_path: project.project_path.clone(),
            dataset_id: dataset.id,
            name: "千条排序".into(),
            criteria: "越重要越靠前".into(),
            mode: SortTaskMode::Drag,
            initial_order: InitialOrder::Import,
        })
        .unwrap();
        let initial =
            SortTaskService::load_drag_workspace(&project.project_path, &task.id).unwrap();
        assert_eq!(initial.items.len(), 1000);
        let last_group = initial.items[999].group_id.clone();

        SortTaskService::move_rank_group(&project.project_path, &task.id, &last_group, 0).unwrap();
        let reopened =
            SortTaskService::load_drag_workspace(&project.project_path, &task.id).unwrap();
        assert_eq!(reopened.items[0].primary_label, "条目1000");
        assert_eq!(reopened.items[0].position, 0);
        assert_eq!(reopened.items[0].original_position, 999);
    }

    #[test]
    fn rejects_matrix_tasks_above_one_hundred_items() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("矩阵上限", directory.path()).unwrap();
        let content = (1..=101)
            .map(|index| format!("条目{index:03}"))
            .collect::<Vec<_>>()
            .join("\n");
        let dataset = DatasetService::commit_import(CommitDatasetImport {
            project_path: project.project_path.clone(),
            dataset_name: "候选项".into(),
            source: DatasetImportSource::PastedText(content),
            deduplicate: false,
            json_pointer: None,
            primary_identifier: "内容".into(),
            auxiliary_identifiers: vec![],
        })
        .unwrap();
        let result = SortTaskService::create(CreateSortTask {
            project_path: project.project_path,
            dataset_id: dataset.id,
            name: "过大矩阵".into(),
            criteria: "重要性".into(),
            mode: SortTaskMode::Matrix,
            initial_order: InitialOrder::Import,
        });
        assert!(matches!(result, Err(AppError::InvalidInput(message)) if message.contains("100")));
    }

    #[test]
    fn creates_renames_and_splits_tied_groups() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("并列组管理", directory.path()).unwrap();
        let dataset = DatasetService::commit_import(CommitDatasetImport {
            project_path: project.project_path.clone(),
            dataset_name: "条目".into(),
            source: DatasetImportSource::PastedText("A\nB\nC".into()),
            deduplicate: false,
            json_pointer: None,
            primary_identifier: "内容".into(),
            auxiliary_identifiers: vec![],
        })
        .unwrap();
        let task = SortTaskService::create(CreateSortTask {
            project_path: project.project_path.clone(),
            dataset_id: dataset.id,
            name: "并列测试".into(),
            criteria: "同等重要可并列".into(),
            mode: SortTaskMode::Drag,
            initial_order: InitialOrder::Import,
        })
        .unwrap();
        let initial =
            SortTaskService::load_drag_workspace(&project.project_path, &task.id).unwrap();
        let tied = SortTaskService::merge_rank_groups(
            &project.project_path,
            &task.id,
            &initial.groups[1].group_id,
            &initial.groups[0].group_id,
        )
        .unwrap();
        assert_eq!(tied.groups.len(), 2);
        assert_eq!(tied.groups[0].items.len(), 2);
        assert_eq!(tied.groups[0].starting_rank, 1);
        assert_eq!(tied.groups[1].starting_rank, 3);
        assert_eq!(tied.groups[0].name.as_deref(), Some("并列组1"));
        assert!(tied.can_undo);

        let untied = SortTaskService::undo_drag_operation(&project.project_path, &task.id).unwrap();
        assert_eq!(untied.groups.len(), 3);
        assert!(untied.groups.iter().all(|group| group.items.len() == 1));
        assert!(untied.can_redo);
        let tied = SortTaskService::redo_drag_operation(&project.project_path, &task.id).unwrap();
        assert_eq!(tied.groups.len(), 2);
        assert_eq!(tied.groups[0].items.len(), 2);

        let renamed = SortTaskService::rename_rank_group(
            &project.project_path,
            &task.id,
            &tied.groups[0].group_id,
            "第一梯队",
        )
        .unwrap();
        assert_eq!(renamed.groups[0].name.as_deref(), Some("第一梯队"));
        let split_item = renamed.groups[0].items[1].item_id.clone();
        let split = SortTaskService::split_rank_group_item(
            &project.project_path,
            &task.id,
            &renamed.groups[0].group_id,
            &split_item,
        )
        .unwrap();
        assert_eq!(split.groups.len(), 3);
        assert!(split.groups.iter().all(|group| group.items.len() == 1));
        let restored_tie =
            SortTaskService::undo_drag_operation(&project.project_path, &task.id).unwrap();
        assert_eq!(restored_tie.groups.len(), 2);
        assert_eq!(restored_tie.groups[0].items.len(), 2);
        assert_eq!(restored_tie.groups[0].name.as_deref(), Some("第一梯队"));
        let split_again =
            SortTaskService::redo_drag_operation(&project.project_path, &task.id).unwrap();
        assert_eq!(split_again.groups.len(), 3);
        assert!(split_again
            .groups
            .iter()
            .all(|group| group.items.len() == 1));
    }

    fn workspace_labels(workspace: &DragWorkspace) -> Vec<&str> {
        workspace
            .items
            .iter()
            .map(|item| item.primary_label.as_str())
            .collect()
    }
}
