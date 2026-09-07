use std::path::Path;

use crate::{
    domain::{
        comparison::{ComparisonDecision, ComparisonWorkspace},
        sort_task::SortTaskMode,
    },
    error::AppError,
    repository::project_repository::ProjectRepository,
    services::project_service::ProjectService,
};

pub struct ComparisonService;

impl ComparisonService {
    pub fn load(project_path: &Path, task_id: &str) -> Result<ComparisonWorkspace, AppError> {
        ProjectService::open(project_path)?;
        let mut repository = ProjectRepository::open(&project_path.join("project.sqlite"))?;
        match repository.sort_task_mode(task_id)? {
            SortTaskMode::Matrix => repository.load_matrix_workspace(task_id),
            SortTaskMode::Comparison => repository.load_comparison_workspace(task_id),
            _ => Err(AppError::InvalidInput("该任务不是 1v1 排序任务".into())),
        }
    }

    pub fn answer(
        project_path: &Path,
        task_id: &str,
        decision: ComparisonDecision,
    ) -> Result<ComparisonWorkspace, AppError> {
        ProjectService::open(project_path)?;
        let mut repository = ProjectRepository::open(&project_path.join("project.sqlite"))?;
        match repository.sort_task_mode(task_id)? {
            SortTaskMode::Matrix => repository.answer_matrix_comparison(task_id, decision),
            SortTaskMode::Comparison => repository.answer_comparison(task_id, decision),
            _ => Err(AppError::InvalidInput("该任务不是 1v1 排序任务".into())),
        }
    }

    pub fn skip(project_path: &Path, task_id: &str) -> Result<ComparisonWorkspace, AppError> {
        ProjectService::open(project_path)?;
        let mut repository = ProjectRepository::open(&project_path.join("project.sqlite"))?;
        match repository.sort_task_mode(task_id)? {
            SortTaskMode::Matrix => repository.skip_matrix_comparison(task_id),
            SortTaskMode::Comparison => repository.skip_comparison(task_id),
            _ => Err(AppError::InvalidInput("该任务不是 1v1 排序任务".into())),
        }
    }

    pub fn undo(project_path: &Path, task_id: &str) -> Result<ComparisonWorkspace, AppError> {
        ProjectService::open(project_path)?;
        let mut repository = ProjectRepository::open(&project_path.join("project.sqlite"))?;
        match repository.sort_task_mode(task_id)? {
            SortTaskMode::Matrix => repository.undo_matrix_comparison(task_id),
            SortTaskMode::Comparison => repository.undo_comparison(task_id),
            _ => Err(AppError::InvalidInput("该任务不是 1v1 排序任务".into())),
        }
    }

    pub fn rename_group(
        project_path: &Path,
        task_id: &str,
        group_id: &str,
        name: &str,
    ) -> Result<ComparisonWorkspace, AppError> {
        ProjectService::open(project_path)?;
        let mut repository = ProjectRepository::open(&project_path.join("project.sqlite"))?;
        let mode = repository.sort_task_mode(task_id)?;
        if !matches!(mode, SortTaskMode::Matrix | SortTaskMode::Comparison) {
            return Err(AppError::InvalidInput("该任务不是 1v1 排序任务".into()));
        }
        repository.rename_rank_group_name(task_id, group_id, name)?;
        match mode {
            SortTaskMode::Matrix => repository.load_matrix_workspace(task_id),
            SortTaskMode::Comparison => repository.load_comparison_workspace(task_id),
            _ => unreachable!("mode checked above"),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        domain::{
            comparison::ComparisonDecision,
            dataset::{CommitDatasetImport, DatasetImportSource},
            sort_task::{CreateSortTask, InitialOrder, SortTaskMode},
        },
        repository::project_repository::ProjectRepository,
        services::{
            dataset_service::DatasetService, result_service::ResultService,
            sort_task_service::SortTaskService,
        },
    };

    use super::*;

    #[test]
    fn sorts_one_hundred_items_with_pause_resume_and_undo() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("百条比较", directory.path()).unwrap();
        let content = (1..=100)
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
        let task = SortTaskService::create(CreateSortTask {
            project_path: project.project_path.clone(),
            dataset_id: dataset.id,
            name: "逐项比较".into(),
            criteria: "编号较小的排在前面".into(),
            mode: SortTaskMode::Comparison,
            initial_order: InitialOrder::Import,
        })
        .unwrap();
        let database = project.project_path.join("project.sqlite");
        let mut repository = ProjectRepository::open(&database).unwrap();
        let initial = repository.load_comparison_workspace(&task.id).unwrap();
        assert_eq!(initial.located_count, 1);
        assert_eq!(initial.total_count, 100);
        assert_eq!(
            initial.planned_comparison_count,
            Some(initial.comparison_count + initial.estimated_remaining)
        );

        let after_first = repository
            .answer_comparison(&task.id, ComparisonDecision::RightBefore)
            .unwrap();
        assert_eq!(after_first.comparison_count, 1);
        assert_eq!(
            after_first.planned_comparison_count,
            Some(after_first.comparison_count + after_first.estimated_remaining)
        );
        let undone = repository.undo_comparison(&task.id).unwrap();
        assert_eq!(undone.comparison_count, 0);
        assert_eq!(undone.left, initial.left);
        assert_eq!(undone.right, initial.right);

        let mut workspace = undone;
        for _ in 0..12 {
            workspace = repository
                .answer_comparison(&task.id, ComparisonDecision::RightBefore)
                .unwrap();
        }
        drop(repository);
        let mut repository = ProjectRepository::open(&database).unwrap();
        let resumed = repository.load_comparison_workspace(&task.id).unwrap();
        assert_eq!(resumed.comparison_count, workspace.comparison_count);
        assert_eq!(resumed.left, workspace.left);
        assert_eq!(resumed.right, workspace.right);
        workspace = resumed;

        while !workspace.completed {
            workspace = repository
                .answer_comparison(&task.id, ComparisonDecision::RightBefore)
                .unwrap();
        }
        assert_eq!(workspace.located_count, 100);
        assert_eq!(workspace.pending_count, 0);
        assert_eq!(workspace.progress_percent, 100);
        assert!(workspace.comparison_count < 700);
        assert!(workspace.comparison_count < 100 * 99 / 2);
        assert_eq!(workspace.ordered_items[0].primary_label, "条目001");
        assert_eq!(workspace.ordered_items[99].primary_label, "条目100");
    }

    #[test]
    fn skips_candidate_without_recording_comparison() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("跳过比较", directory.path()).unwrap();
        let dataset = DatasetService::commit_import(CommitDatasetImport {
            project_path: project.project_path.clone(),
            dataset_name: "候选项".into(),
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
            name: "跳过测试".into(),
            criteria: "更优者靠前".into(),
            mode: SortTaskMode::Comparison,
            initial_order: InitialOrder::Import,
        })
        .unwrap();
        let before = ComparisonService::load(&project.project_path, &task.id).unwrap();
        let skipped = ComparisonService::skip(&project.project_path, &task.id).unwrap();
        assert_ne!(skipped.left, before.left);
        assert_eq!(skipped.comparison_count, 0);
        assert!(!skipped.can_undo);
        let reopened = ComparisonService::load(&project.project_path, &task.id).unwrap();
        assert_eq!(reopened.left, skipped.left);
    }

    #[test]
    fn creates_tied_group_and_undoes_tie_judgment() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("1v1并列", directory.path()).unwrap();
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
        let task = SortTaskService::create(CreateSortTask {
            project_path: project.project_path.clone(),
            dataset_id: dataset.id,
            name: "并列判断".into(),
            criteria: "同等重要可并列".into(),
            mode: SortTaskMode::Comparison,
            initial_order: InitialOrder::Import,
        })
        .unwrap();
        let tied =
            ComparisonService::answer(&project.project_path, &task.id, ComparisonDecision::Tie)
                .unwrap();
        assert_eq!(tied.located_count, 2);
        assert_eq!(tied.ordered_items[0].member_labels, ["A", "B"]);
        assert_eq!(
            tied.planned_comparison_count,
            Some(tied.comparison_count + tied.estimated_remaining)
        );
        let renamed = ComparisonService::rename_group(
            &project.project_path,
            &task.id,
            &tied.ordered_items[0].group_id,
            "第一梯队",
        )
        .unwrap();
        assert_eq!(
            renamed.ordered_items[0].group_name.as_deref(),
            Some("第一梯队")
        );
        let drag = SortTaskService::load_drag_workspace(&project.project_path, &task.id);
        assert!(drag.is_err(), "比较未完成前不能进入混合拖拽");

        let undone = ComparisonService::undo(&project.project_path, &task.id).unwrap();
        assert_eq!(undone.located_count, 1);
        assert_eq!(undone.left.unwrap().member_labels, ["B"]);
        let repository =
            ProjectRepository::open(&project.project_path.join("project.sqlite")).unwrap();
        let active_group_count = repository
            .list_sort_tasks()
            .unwrap()
            .into_iter()
            .find(|candidate| candidate.id == task.id)
            .unwrap()
            .rank_group_count;
        assert_eq!(active_group_count, 3);
    }

    #[test]
    fn detects_and_resolves_mixed_sort_conflicts() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("混合排序", directory.path()).unwrap();
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
        let task = SortTaskService::create(CreateSortTask {
            project_path: project.project_path.clone(),
            dataset_id: dataset.id,
            name: "混合排序".into(),
            criteria: "字母靠前者优先".into(),
            mode: SortTaskMode::Comparison,
            initial_order: InitialOrder::Import,
        })
        .unwrap();
        let mut workspace = ComparisonService::load(&project.project_path, &task.id).unwrap();
        while !workspace.completed {
            workspace = ComparisonService::answer(
                &project.project_path,
                &task.id,
                ComparisonDecision::RightBefore,
            )
            .unwrap();
        }
        let drag = SortTaskService::load_drag_workspace(&project.project_path, &task.id).unwrap();
        let conflict = SortTaskService::preview_rank_group_move(
            &project.project_path,
            &task.id,
            &drag.groups[0].group_id,
            2,
        )
        .unwrap();
        assert!(conflict.conflict_count > 0);
        SortTaskService::move_rank_group_with_resolution(
            &project.project_path,
            &task.id,
            &drag.groups[0].group_id,
            2,
            crate::domain::sort_task::MoveConflictResolution::UpdateRelations,
        )
        .unwrap();
        let after = SortTaskService::preview_rank_group_move(
            &project.project_path,
            &task.id,
            &drag.groups[0].group_id,
            2,
        )
        .unwrap();
        assert_eq!(after.conflict_count, 0);
    }

    #[test]
    fn completes_full_matrix_groups_equal_wins_and_undoes_finalization() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("完整矩阵", directory.path()).unwrap();
        let dataset = DatasetService::commit_import(CommitDatasetImport {
            project_path: project.project_path.clone(),
            dataset_name: "候选项".into(),
            source: DatasetImportSource::PastedText("A\nB\nC\nD".into()),
            deduplicate: false,
            json_pointer: None,
            primary_identifier: "内容".into(),
            auxiliary_identifiers: vec![],
        })
        .unwrap();
        let task = SortTaskService::create_with_matrix_percent(
            CreateSortTask {
                project_path: project.project_path.clone(),
                dataset_id: dataset.id,
                name: "矩阵比较".into(),
                criteria: "同等重要".into(),
                mode: SortTaskMode::Matrix,
                initial_order: InitialOrder::Import,
            },
            Some(100),
        )
        .unwrap();

        let mut workspace = ComparisonService::load(&project.project_path, &task.id).unwrap();
        assert_eq!(workspace.mode, SortTaskMode::Matrix);
        assert_eq!(workspace.planned_comparison_count, Some(6));
        assert_eq!(workspace.matrix_comparison_percent, Some(100));
        while !workspace.completed {
            workspace =
                ComparisonService::answer(&project.project_path, &task.id, ComparisonDecision::Tie)
                    .unwrap();
        }
        assert_eq!(workspace.comparison_count, 6);
        assert!(workspace
            .standings
            .iter()
            .all(|standing| { standing.wins == 0 && standing.losses == 0 && standing.ties == 3 }));
        let repository =
            ProjectRepository::open(&project.project_path.join("project.sqlite")).unwrap();
        let overview = repository
            .list_sort_tasks()
            .unwrap()
            .into_iter()
            .find(|candidate| candidate.id == task.id)
            .unwrap();
        assert_eq!(overview.rank_group_count, 1);
        let preview = ResultService::load(&project.project_path, &task.id).unwrap();
        assert!(preview.can_confirm);
        assert_eq!(preview.items.len(), 4);
        assert!(preview.items.iter().all(|item| item.rank == 1));

        let undone = ComparisonService::undo(&project.project_path, &task.id).unwrap();
        assert!(!undone.completed);
        assert_eq!(undone.comparison_count, 5);
        let repository =
            ProjectRepository::open(&project.project_path.join("project.sqlite")).unwrap();
        let overview = repository
            .list_sort_tasks()
            .unwrap()
            .into_iter()
            .find(|candidate| candidate.id == task.id)
            .unwrap();
        assert_eq!(overview.rank_group_count, 4);
    }

    #[test]
    fn sampled_matrix_persists_a_bounded_union_of_pairs() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("抽样矩阵", directory.path()).unwrap();
        let content = (1..=10)
            .map(|index| format!("条目{index}"))
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
        let task = SortTaskService::create_with_matrix_percent(
            CreateSortTask {
                project_path: project.project_path.clone(),
                dataset_id: dataset.id,
                name: "半量矩阵".into(),
                criteria: "重要性".into(),
                mode: SortTaskMode::Matrix,
                initial_order: InitialOrder::Import,
            },
            Some(50),
        )
        .unwrap();
        let first = ComparisonService::load(&project.project_path, &task.id).unwrap();
        let planned = first.planned_comparison_count.unwrap();
        assert!(planned > 10 * 9 / 4);
        assert!(planned <= 10 * 9 / 2);
        drop(first);
        let reopened = ComparisonService::load(&project.project_path, &task.id).unwrap();
        assert_eq!(reopened.planned_comparison_count, Some(planned));
        assert_eq!(reopened.matrix_comparison_percent, Some(50));
    }

    #[test]
    fn matrix_orders_results_by_win_count() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("胜场排名", directory.path()).unwrap();
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
        let task = SortTaskService::create_with_matrix_percent(
            CreateSortTask {
                project_path: project.project_path.clone(),
                dataset_id: dataset.id,
                name: "胜场矩阵".into(),
                criteria: "字母靠前者胜出".into(),
                mode: SortTaskMode::Matrix,
                initial_order: InitialOrder::Import,
            },
            Some(100),
        )
        .unwrap();
        let mut workspace = ComparisonService::load(&project.project_path, &task.id).unwrap();
        while !workspace.completed {
            let left = workspace.left.as_ref().unwrap().primary_label.as_str();
            let right = workspace.right.as_ref().unwrap().primary_label.as_str();
            let decision = if left < right {
                ComparisonDecision::LeftBefore
            } else {
                ComparisonDecision::RightBefore
            };
            workspace =
                ComparisonService::answer(&project.project_path, &task.id, decision).unwrap();
        }
        assert_eq!(
            workspace
                .standings
                .iter()
                .map(|standing| (standing.item.primary_label.as_str(), standing.wins))
                .collect::<Vec<_>>(),
            [("A", 2), ("B", 1), ("C", 0)]
        );
        let preview = ResultService::load(&project.project_path, &task.id).unwrap();
        assert_eq!(
            preview
                .items
                .iter()
                .map(|item| item.primary_label.as_str())
                .collect::<Vec<_>>(),
            ["A", "B", "C"]
        );
    }
}
