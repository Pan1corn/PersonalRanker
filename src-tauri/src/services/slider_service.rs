use std::path::Path;

use crate::{
    domain::slider::SliderWorkspace, error::AppError,
    repository::project_repository::ProjectRepository, services::project_service::ProjectService,
};

pub struct SliderService;

impl SliderService {
    pub fn load(project_path: &Path, task_id: &str) -> Result<SliderWorkspace, AppError> {
        ProjectService::open(project_path)?;
        ProjectRepository::open(&project_path.join("project.sqlite"))?
            .load_slider_workspace(task_id)
    }

    pub fn confirm(
        project_path: &Path,
        task_id: &str,
        value: f64,
    ) -> Result<SliderWorkspace, AppError> {
        if !value.is_finite() || !(0.0..=100.0).contains(&value) {
            return Err(AppError::InvalidInput(
                "滑杆位置必须在 0.00 到 100.00 之间".into(),
            ));
        }
        let value_hundredths = (value * 100.0).round() as u16;
        ProjectService::open(project_path)?;
        ProjectRepository::open(&project_path.join("project.sqlite"))?
            .confirm_slider_rating(task_id, value_hundredths)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        domain::{
            dataset::{CommitDatasetImport, DatasetImportSource},
            sort_task::{CreateSortTask, InitialOrder, SortTaskMode},
        },
        services::{
            dataset_service::DatasetService, project_service::ProjectService,
            result_service::ResultService, sort_task_service::SortTaskService,
        },
    };

    use super::*;

    #[test]
    fn persists_two_decimal_values_and_groups_equal_scores() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("滑杆测试", directory.path()).unwrap();
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
            name: "重要性滑杆".into(),
            criteria: "数值越高越靠前".into(),
            mode: SortTaskMode::Slider,
            initial_order: InitialOrder::Import,
        })
        .unwrap();

        let initial = SliderService::load(&project.project_path, &task.id).unwrap();
        assert_eq!(initial.current.unwrap().primary_label, "A");
        assert!(SortTaskService::load_drag_workspace(&project.project_path, &task.id).is_err());
        let second = SliderService::confirm(&project.project_path, &task.id, 80.126).unwrap();
        assert_eq!(second.ratings[0].value, 80.13);
        assert_eq!(second.current.unwrap().primary_label, "B");
        SliderService::confirm(&project.project_path, &task.id, 20.0).unwrap();
        let completed = SliderService::confirm(&project.project_path, &task.id, 80.13).unwrap();
        assert!(completed.completed);
        assert_eq!(completed.progress_percent, 100);

        let preview = ResultService::load(&project.project_path, &task.id).unwrap();
        assert!(preview.can_confirm);
        assert_eq!(
            preview
                .items
                .iter()
                .map(|item| (item.primary_label.as_str(), item.rank))
                .collect::<Vec<_>>(),
            [("A", 1), ("C", 1), ("B", 3)]
        );
        assert!(SortTaskService::load_drag_workspace(&project.project_path, &task.id).is_ok());
    }

    #[test]
    fn rejects_out_of_range_slider_values() {
        let result = SliderService::confirm(Path::new("unused"), "unused", 100.001);
        assert!(matches!(result, Err(AppError::InvalidInput(_))));
    }
}
