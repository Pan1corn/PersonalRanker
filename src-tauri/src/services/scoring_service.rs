use crate::{
    domain::scoring::{
        ScoreConfig, ScoreMethod, ScorePreview, ScorePreviewItem, ScorePreviewRequest, TieScoreRule,
    },
    error::AppError,
    repository::project_repository::ProjectRepository,
    services::project_service::ProjectService,
};

pub struct ScoringService;

impl ScoringService {
    pub fn load_config(
        project_path: &std::path::Path,
        task_id: &str,
    ) -> Result<Option<ScoreConfig>, AppError> {
        ProjectService::open(project_path)?;
        let repository = ProjectRepository::open(&project_path.join("project.sqlite"))?;
        repository.load_score_config(task_id)
    }

    pub fn save_config(
        project_path: &std::path::Path,
        task_id: &str,
        config: &ScoreConfig,
    ) -> Result<ScoreConfig, AppError> {
        ProjectService::open(project_path)?;
        validate_config(config)?;
        let repository = ProjectRepository::open(&project_path.join("project.sqlite"))?;
        repository.save_score_config(task_id, config)?;
        Ok(config.clone())
    }

    pub fn preview(request: &ScorePreviewRequest) -> Result<ScorePreview, AppError> {
        ProjectService::open(&request.project_path)?;
        validate_config(&request.config)?;
        let repository = ProjectRepository::open(&request.project_path.join("project.sqlite"))?;
        let groups = repository.load_scoring_groups(&request.task_id)?;
        let total_items = groups.iter().map(|group| group.items.len()).sum::<usize>();
        let position_scores = position_scores(total_items, &request.config.method)?;
        let mut occupied = 0;
        let mut items = Vec::with_capacity(total_items);
        for group in groups {
            let end = occupied + group.items.len();
            let tied_scores = &position_scores[occupied..end];
            // 并列组取平均值后可能再次产生更多小数，必须在最终组分数上应用精度。
            let score = normalize_score(
                tie_score(tied_scores, request.config.tie_rule),
                &request.config.method,
            );
            let rank = occupied + 1;
            for item in group.items {
                items.push(ScorePreviewItem {
                    group_id: group.group_id.clone(),
                    group_name: group.group_name.clone(),
                    item_id: item.item_id,
                    primary_label: item.primary_label,
                    rank,
                    score,
                    old_value: item
                        .fields
                        .get(&request.config.field_name)
                        .cloned()
                        .unwrap_or(serde_json::Value::Null),
                });
            }
            occupied = end;
        }
        let existing_type =
            repository.numeric_field_type(&request.task_id, &request.config.field_name)?;
        let can_write = existing_type.as_deref().is_none_or(|kind| kind == "number");
        Ok(ScorePreview {
            task_id: request.task_id.clone(),
            field_name: request.config.field_name.clone(),
            field_exists: existing_type.is_some(),
            can_write,
            items,
        })
    }

    pub fn write(request: &ScorePreviewRequest, overwrite: bool) -> Result<ScorePreview, AppError> {
        let preview = Self::preview(request)?;
        if !preview.can_write {
            return Err(AppError::InvalidInput(
                "目标字段不是数字类型，无法写入评分".into(),
            ));
        }
        let values = preview
            .items
            .iter()
            .map(|item| (item.item_id.clone(), item.score))
            .collect::<Vec<_>>();
        let mut repository = ProjectRepository::open(&request.project_path.join("project.sqlite"))?;
        repository.write_scores(&request.task_id, &request.config, &values, overwrite)?;
        Self::preview(request)
    }
}

fn validate_config(config: &ScoreConfig) -> Result<(), AppError> {
    if config.field_name.trim().is_empty() {
        return Err(AppError::InvalidInput("评分字段名称不能为空".into()));
    }
    match &config.method {
        ScoreMethod::Linear {
            highest_score,
            lowest_score,
            decimal_places,
            ..
        } => {
            if !highest_score.is_finite() || !lowest_score.is_finite() {
                return Err(AppError::InvalidInput(
                    "最高分和最低分必须是有限数字".into(),
                ));
            }
            if highest_score < lowest_score {
                return Err(AppError::InvalidInput("最高分不能低于最低分".into()));
            }
            if *decimal_places > 6 {
                return Err(AppError::InvalidInput("评分最多保留 6 位小数".into()));
            }
        }
        ScoreMethod::Buckets { levels } => {
            if levels.is_empty() || levels.iter().any(|level| !level.is_finite()) {
                return Err(AppError::InvalidInput(
                    "等量分档至少需要一个有效分数档位".into(),
                ));
            }
        }
    }
    Ok(())
}

fn position_scores(total_items: usize, method: &ScoreMethod) -> Result<Vec<f64>, AppError> {
    if total_items == 0 {
        return Err(AppError::InvalidInput("排序结果中没有可评分条目".into()));
    }
    match method {
        ScoreMethod::Linear {
            highest_score,
            lowest_score,
            decimal_places,
            high_rank_high_score,
        } => {
            let range = highest_score - lowest_score;
            Ok((0..total_items)
                .map(|index| {
                    let ratio = if total_items == 1 {
                        0.0
                    } else {
                        index as f64 / (total_items - 1) as f64
                    };
                    let raw = if *high_rank_high_score {
                        highest_score - range * ratio
                    } else {
                        lowest_score + range * ratio
                    };
                    round_decimal(raw, *decimal_places)
                })
                .collect())
        }
        ScoreMethod::Buckets { levels } => Ok((0..total_items)
            .map(|index| {
                let bucket = index.saturating_mul(levels.len()) / total_items;
                levels[bucket.min(levels.len() - 1)]
            })
            .collect()),
    }
}

fn normalize_score(value: f64, method: &ScoreMethod) -> f64 {
    match method {
        ScoreMethod::Linear { decimal_places, .. } => round_decimal(value, *decimal_places),
        ScoreMethod::Buckets { .. } => value,
    }
}

fn round_decimal(value: f64, decimal_places: u8) -> f64 {
    let factor = 10_f64.powi(i32::from(decimal_places));
    (value * factor).round() / factor
}

fn tie_score(scores: &[f64], rule: TieScoreRule) -> f64 {
    match rule {
        TieScoreRule::Average => scores.iter().sum::<f64>() / scores.len() as f64,
        TieScoreRule::Highest => scores.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        TieScoreRule::Lowest => scores.iter().copied().fold(f64::INFINITY, f64::min),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use crate::{
        domain::{
            dataset::{CommitDatasetImport, DatasetImportSource},
            result::{ExportFormat, ExportOrder, ExportResultRequest},
            scoring::{ScoreConfig, ScoreMethod, ScorePreviewRequest, TieScoreRule},
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
    fn previews_and_writes_linear_scores_for_tied_groups() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("并列评分", directory.path()).unwrap();
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
            name: "评分".into(),
            criteria: "越重要越靠前".into(),
            mode: SortTaskMode::Drag,
            initial_order: InitialOrder::Import,
        })
        .unwrap();
        let workspace =
            SortTaskService::load_drag_workspace(&project.project_path, &task.id).unwrap();
        SortTaskService::merge_rank_groups(
            &project.project_path,
            &task.id,
            &workspace.groups[1].group_id,
            &workspace.groups[0].group_id,
        )
        .unwrap();
        ResultService::confirm(&project.project_path, &task.id).unwrap();

        let request = ScorePreviewRequest {
            project_path: project.project_path.clone(),
            task_id: task.id.clone(),
            config: ScoreConfig {
                field_name: "score".into(),
                method: ScoreMethod::Linear {
                    highest_score: 10.0,
                    lowest_score: 0.0,
                    decimal_places: 2,
                    high_rank_high_score: true,
                },
                tie_rule: TieScoreRule::Average,
            },
        };
        let preview = ScoringService::preview(&request).unwrap();
        assert_eq!(
            preview
                .items
                .iter()
                .map(|item| item.score)
                .collect::<Vec<_>>(),
            [8.34, 8.34, 3.33, 0.0]
        );
        assert!(preview
            .items
            .iter()
            .all(|item| item.old_value == Value::Null));
        let written = ScoringService::write(&request, false).unwrap();
        assert!(written.field_exists);
        let repository =
            ProjectRepository::open(&project.project_path.join("project.sqlite")).unwrap();
        assert_eq!(
            repository.ranked_field_values(&task.id, "score").unwrap(),
            [
                Value::from(8.34),
                Value::from(8.34),
                Value::from(3.33),
                Value::from(0.0)
            ]
        );
        drop(repository);

        let target_path = directory.path().join("scores.csv");
        ResultService::export(&ExportResultRequest {
            project_path: project.project_path.clone(),
            task_id: task.id,
            target_path: target_path.clone(),
            format: ExportFormat::Csv,
            order: ExportOrder::Final,
            selected_fields: vec!["score".into()],
            include_rank: false,
            rank_field_name: "rank".into(),
            include_original_index: false,
            overwrite: false,
        })
        .unwrap();
        let exported = std::fs::read_to_string(target_path).unwrap();
        assert!(exported.contains("8.34"));
        assert!(exported.contains("3.33"));
        assert!(exported.contains("0.00"));
        assert!(!exported.contains("8.335"));
    }

    #[test]
    fn distributes_bucket_levels_evenly() {
        let scores = position_scores(
            10,
            &ScoreMethod::Buckets {
                levels: vec![5.0, 4.0, 3.0, 2.0, 1.0],
            },
        )
        .unwrap();
        assert_eq!(scores, [5.0, 5.0, 4.0, 4.0, 3.0, 3.0, 2.0, 2.0, 1.0, 1.0]);
        assert_eq!(tie_score(&[5.0, 3.0], TieScoreRule::Highest), 5.0);
        assert_eq!(tie_score(&[5.0, 3.0], TieScoreRule::Lowest), 3.0);
    }
}
