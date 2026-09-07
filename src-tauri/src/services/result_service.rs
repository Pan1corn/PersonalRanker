//! 结果导出用例层。
//! 排名和确认写回项目数据库；导出则从只读结果行生成目标格式，绝不改动 `source` 副本。

use std::{fs, path::Path};

use serde_json::{Map, Value};

use crate::{
    domain::result::{
        ExportFormat, ExportOrder, ExportResultReceipt, ExportResultRequest, ResultPreview,
    },
    domain::scoring::{RankRule, ScoreMethod},
    error::AppError,
    repository::project_repository::ProjectRepository,
    services::project_service::ProjectService,
};

pub struct ResultService;

impl ResultService {
    pub fn load(project_path: &Path, task_id: &str) -> Result<ResultPreview, AppError> {
        ProjectService::open(project_path)?;
        ProjectRepository::open(&project_path.join("project.sqlite"))?.load_result_preview(task_id)
    }

    pub fn confirm(project_path: &Path, task_id: &str) -> Result<ResultPreview, AppError> {
        ProjectService::open(project_path)?;
        ProjectRepository::open(&project_path.join("project.sqlite"))?.confirm_sort_result(task_id)
    }

    pub fn unlock(project_path: &Path, task_id: &str) -> Result<ResultPreview, AppError> {
        ProjectService::open(project_path)?;
        ProjectRepository::open(&project_path.join("project.sqlite"))?.unlock_sort_result(task_id)
    }

    #[cfg(test)]
    pub fn write_rank(
        project_path: &Path,
        task_id: &str,
        field_name: &str,
        overwrite: bool,
    ) -> Result<ResultPreview, AppError> {
        Self::write_rank_with_rule(
            project_path,
            task_id,
            field_name,
            overwrite,
            RankRule::Competition,
        )
    }

    pub fn write_rank_with_rule(
        project_path: &Path,
        task_id: &str,
        field_name: &str,
        overwrite: bool,
        rank_rule: RankRule,
    ) -> Result<ResultPreview, AppError> {
        ProjectService::open(project_path)?;
        ProjectRepository::open(&project_path.join("project.sqlite"))?
            .write_rank_field_with_rule(task_id, field_name, overwrite, rank_rule)
    }

    pub fn target_exists(path: &Path) -> bool {
        path.exists()
    }

    pub fn export(request: &ExportResultRequest) -> Result<ExportResultReceipt, AppError> {
        ProjectService::open(&request.project_path)?;
        validate_export_target(
            &request.project_path,
            &request.target_path,
            request.overwrite,
        )?;
        let repository = ProjectRepository::open(&request.project_path.join("project.sqlite"))?;
        let known_fields = repository.dataset_field_names(&request.task_id)?;
        let invalid_fields = request
            .selected_fields
            .iter()
            .filter(|name| !known_fields.contains(name))
            .cloned()
            .collect::<Vec<_>>();
        if !invalid_fields.is_empty() {
            return Err(AppError::InvalidInput(format!(
                "导出字段不存在：{}",
                invalid_fields.join("、")
            )));
        }
        let rank_field_name = request.rank_field_name.trim();
        if request.include_rank && rank_field_name.is_empty() {
            return Err(AppError::InvalidInput("导出排名字段名称不能为空".into()));
        }
        // repository 始终按最终排名读取；需要原始顺序时只调整导出视图，不改数据库。
        let mut rows = repository.load_export_rows(&request.task_id)?;
        let score_format = repository
            .load_score_config(&request.task_id)?
            .and_then(|config| match config.method {
                ScoreMethod::Linear { decimal_places, .. } => Some(ScoreExportFormat {
                    field_name: config.field_name,
                    decimal_places: usize::from(decimal_places),
                }),
                ScoreMethod::Buckets { .. } => None,
            });
        if request.order == ExportOrder::Original {
            rows.sort_by_key(|row| row.original_index);
        }
        let bytes = match request.format {
            ExportFormat::Csv => export_csv(
                &rows,
                &request.selected_fields,
                request.include_rank,
                rank_field_name,
                request.include_original_index,
                score_format.as_ref(),
            )?,
            ExportFormat::Json => export_json(
                &rows,
                &request.selected_fields,
                request.include_rank,
                rank_field_name,
                request.include_original_index,
                score_format.as_ref(),
            )?,
            ExportFormat::MarkdownTable => export_markdown_table(
                &rows,
                &request.selected_fields,
                request.include_rank,
                rank_field_name,
                request.include_original_index,
                score_format.as_ref(),
            ),
            ExportFormat::MarkdownList => export_markdown_list(
                &rows,
                &request.selected_fields,
                request.include_rank,
                rank_field_name,
                request.include_original_index,
                score_format.as_ref(),
            ),
            ExportFormat::Text => export_text(
                &rows,
                &request.selected_fields,
                request.include_rank,
                rank_field_name,
                request.include_original_index,
                score_format.as_ref(),
            ),
        };
        fs::write(&request.target_path, bytes)
            .map_err(|error| AppError::from_io(error, &request.target_path))?;
        Ok(ExportResultReceipt {
            path: request.target_path.clone(),
            row_count: rows.len(),
            format: request.format.clone(),
        })
    }
}

#[derive(Debug)]
struct ScoreExportFormat {
    field_name: String,
    decimal_places: usize,
}

fn validate_export_target(
    project_path: &Path,
    target_path: &Path,
    overwrite: bool,
) -> Result<(), AppError> {
    if target_path.as_os_str().is_empty() {
        return Err(AppError::InvalidInput("请选择导出文件位置".into()));
    }
    let parent = target_path
        .parent()
        .ok_or_else(|| AppError::InvalidInput("导出路径无效".into()))?;
    if !parent.is_dir() {
        return Err(AppError::InvalidInput("导出目录不存在".into()));
    }
    // canonicalize 后比较父目录，阻止通过相对路径绕过只读 source 目录保护。
    let source_directory = project_path
        .join("source")
        .canonicalize()
        .map_err(|error| AppError::InvalidProject(format!("无法验证项目源文件目录：{error}")))?;
    let resolved_parent = parent
        .canonicalize()
        .map_err(|error| AppError::from_io(error, parent))?;
    if resolved_parent.starts_with(&source_directory) {
        return Err(AppError::InvalidInput(
            "不能导出到项目的只读 source 目录，以免覆盖导入源副本".into(),
        ));
    }
    if target_path.exists() && !overwrite {
        return Err(AppError::ConfirmationRequired(format!(
            "文件“{}”已存在",
            target_path.display()
        )));
    }
    Ok(())
}

fn export_csv(
    rows: &[crate::domain::result::ExportRow],
    selected_fields: &[String],
    include_rank: bool,
    rank_field_name: &str,
    include_original_index: bool,
    score_format: Option<&ScoreExportFormat>,
) -> Result<Vec<u8>, AppError> {
    let mut writer = csv::WriterBuilder::new().from_writer(vec![0xEF, 0xBB, 0xBF]);
    let headers = export_headers(
        selected_fields,
        include_rank,
        rank_field_name,
        include_original_index,
    );
    writer.write_record(&headers)?;
    for row in rows {
        let values = headers
            .iter()
            .map(|header| {
                if include_rank && header == rank_field_name {
                    row.final_rank.to_string()
                } else if include_original_index && header == "original_index" {
                    row.original_index.to_string()
                } else {
                    export_field_value(header, row.fields.get(header), score_format)
                }
            })
            .collect::<Vec<_>>();
        writer.write_record(values)?;
    }
    writer
        .into_inner()
        .map_err(|error| AppError::Io(error.into_error()))
}

fn export_json(
    rows: &[crate::domain::result::ExportRow],
    selected_fields: &[String],
    include_rank: bool,
    rank_field_name: &str,
    include_original_index: bool,
    score_format: Option<&ScoreExportFormat>,
) -> Result<Vec<u8>, AppError> {
    let output = rows
        .iter()
        .map(|row| {
            let mut object = Map::new();
            if include_rank {
                object.insert(rank_field_name.to_owned(), Value::from(row.final_rank));
            }
            if include_original_index {
                object.insert("original_index".into(), Value::from(row.original_index));
            }
            for field in selected_fields {
                if (include_rank && field == rank_field_name)
                    || (include_original_index && field == "original_index")
                {
                    continue;
                }
                object.insert(
                    field.clone(),
                    export_json_value(field, row.fields.get(field), score_format),
                );
            }
            Value::Object(object)
        })
        .collect::<Vec<_>>();
    Ok(serde_json::to_string_pretty(&output)?.into_bytes())
}

fn export_markdown_table(
    rows: &[crate::domain::result::ExportRow],
    selected_fields: &[String],
    include_rank: bool,
    rank_field_name: &str,
    include_original_index: bool,
    score_format: Option<&ScoreExportFormat>,
) -> Vec<u8> {
    let headers = export_headers(
        selected_fields,
        include_rank,
        rank_field_name,
        include_original_index,
    );
    let mut output = String::new();
    output.push_str("| ");
    output.push_str(
        &headers
            .iter()
            .map(|header| markdown_cell(header))
            .collect::<Vec<_>>()
            .join(" | "),
    );
    output.push_str(" |\n|");
    output.push_str(
        &headers
            .iter()
            .map(|_| " --- ")
            .collect::<Vec<_>>()
            .join("|"),
    );
    output.push_str("|\n");
    for row in rows {
        output.push_str("| ");
        output.push_str(
            &export_row_values(
                row,
                &headers,
                include_rank,
                rank_field_name,
                include_original_index,
                score_format,
            )
            .iter()
            .map(|value| markdown_cell(value))
            .collect::<Vec<_>>()
            .join(" | "),
        );
        output.push_str(" |\n");
    }
    output.into_bytes()
}

fn export_markdown_list(
    rows: &[crate::domain::result::ExportRow],
    selected_fields: &[String],
    include_rank: bool,
    rank_field_name: &str,
    include_original_index: bool,
    score_format: Option<&ScoreExportFormat>,
) -> Vec<u8> {
    let headers = export_headers(
        selected_fields,
        include_rank,
        rank_field_name,
        include_original_index,
    );
    let mut output = String::new();
    for row in rows {
        let values = export_row_values(
            row,
            &headers,
            include_rank,
            rank_field_name,
            include_original_index,
            score_format,
        );
        output.push_str("- ");
        if headers.len() == 1 {
            output.push_str(&markdown_inline(&values[0]));
        } else {
            output.push_str(
                &headers
                    .iter()
                    .zip(values)
                    .map(|(header, value)| {
                        format!(
                            "**{}**: {}",
                            markdown_inline(header),
                            markdown_inline(&value)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" · "),
            );
        }
        output.push('\n');
    }
    output.into_bytes()
}

fn export_text(
    rows: &[crate::domain::result::ExportRow],
    selected_fields: &[String],
    include_rank: bool,
    rank_field_name: &str,
    include_original_index: bool,
    score_format: Option<&ScoreExportFormat>,
) -> Vec<u8> {
    let headers = export_headers(
        selected_fields,
        include_rank,
        rank_field_name,
        include_original_index,
    );
    rows.iter()
        .map(|row| {
            export_row_values(
                row,
                &headers,
                include_rank,
                rank_field_name,
                include_original_index,
                score_format,
            )
            .into_iter()
            .map(|value| value.replace(['\r', '\n'], " "))
            .collect::<Vec<_>>()
            .join("\t")
        })
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes()
}

fn export_row_values(
    row: &crate::domain::result::ExportRow,
    headers: &[String],
    include_rank: bool,
    rank_field_name: &str,
    include_original_index: bool,
    score_format: Option<&ScoreExportFormat>,
) -> Vec<String> {
    headers
        .iter()
        .map(|header| {
            if include_rank && header == rank_field_name {
                row.final_rank.to_string()
            } else if include_original_index && header == "original_index" {
                row.original_index.to_string()
            } else {
                export_field_value(header, row.fields.get(header), score_format)
            }
        })
        .collect()
}

fn export_field_value(
    field_name: &str,
    value: Option<&Value>,
    score_format: Option<&ScoreExportFormat>,
) -> String {
    if let (Some(format), Some(number)) = (score_format, value.and_then(Value::as_f64)) {
        if field_name == format.field_name {
            return format!("{number:.precision$}", precision = format.decimal_places);
        }
    }
    csv_value(value)
}

fn export_json_value(
    field_name: &str,
    value: Option<&Value>,
    score_format: Option<&ScoreExportFormat>,
) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    if let (Some(format), Some(number)) = (score_format, value.as_f64()) {
        if field_name == format.field_name {
            let factor = 10_f64.powi(format.decimal_places as i32);
            return Value::from((number * factor).round() / factor);
        }
    }
    value.clone()
}

fn markdown_cell(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace("\r\n", "<br>")
        .replace(['\r', '\n'], "<br>")
}

fn markdown_inline(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace(['\r', '\n'], " ")
        .replace('*', "\\*")
        .replace('_', "\\_")
}

fn export_headers(
    selected_fields: &[String],
    include_rank: bool,
    rank_field_name: &str,
    include_original_index: bool,
) -> Vec<String> {
    let mut headers = Vec::new();
    if include_rank {
        headers.push(rank_field_name.to_owned());
    }
    if include_original_index {
        headers.push("original_index".into());
    }
    for field in selected_fields {
        if !headers.contains(field) {
            headers.push(field.clone());
        }
    }
    headers
}

fn csv_value(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Bool(value)) => value.to_string(),
        Some(Value::Number(value)) => value.to_string(),
        Some(value @ (Value::Array(_) | Value::Object(_))) => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::Value;

    use crate::{
        domain::{
            dataset::{CommitDatasetImport, DatasetImportSource},
            result::{ExportFormat, ExportOrder, ExportResultRequest},
            scoring::RankRule,
            sort_task::{CreateSortTask, InitialOrder, SortTaskMode, SortTaskStatus},
        },
        error::AppError,
        repository::project_repository::ProjectRepository,
        services::{dataset_service::DatasetService, sort_task_service::SortTaskService},
    };

    use super::*;

    #[test]
    fn confirms_locks_writes_rank_and_exports_without_touching_source() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("结果与导出", directory.path()).unwrap();
        let original = directory.path().join("items.csv");
        fs::write(&original, "name,category\nA,one\nB,two\nC,three").unwrap();
        let dataset = DatasetService::commit_import(CommitDatasetImport {
            project_path: project.project_path.clone(),
            dataset_name: "候选项".into(),
            source: DatasetImportSource::File(original),
            deduplicate: false,
            json_pointer: None,
            primary_identifier: "name".into(),
            auxiliary_identifiers: vec!["category".into()],
        })
        .unwrap();
        let task = SortTaskService::create(CreateSortTask {
            project_path: project.project_path.clone(),
            dataset_id: dataset.id,
            name: "优先级".into(),
            criteria: "更重要者靠前".into(),
            mode: SortTaskMode::Drag,
            initial_order: InitialOrder::Import,
        })
        .unwrap();
        let workspace =
            SortTaskService::load_drag_workspace(&project.project_path, &task.id).unwrap();
        SortTaskService::move_rank_group(
            &project.project_path,
            &task.id,
            &workspace.items[2].group_id,
            0,
        )
        .unwrap();

        let preview = ResultService::load(&project.project_path, &task.id).unwrap();
        assert!(preview.integrity.is_valid());
        assert_eq!(preview.items[0].primary_label, "C");
        assert_eq!(preview.items[0].rank_change, 2);
        let confirmed = ResultService::confirm(&project.project_path, &task.id).unwrap();
        assert_eq!(confirmed.status, SortTaskStatus::Confirmed);
        assert!(confirmed.confirmed_at.is_some());
        assert!(SortTaskService::move_rank_group(
            &project.project_path,
            &task.id,
            &workspace.items[0].group_id,
            1
        )
        .is_err());

        let written =
            ResultService::write_rank(&project.project_path, &task.id, "rank", false).unwrap();
        assert!(written.rank_written_at.is_some());
        assert_eq!(written.rank_written_field.as_deref(), Some("rank"));
        let repository =
            ProjectRepository::open(&project.project_path.join("project.sqlite")).unwrap();
        assert_eq!(
            repository.ranked_field_values(&task.id, "rank").unwrap(),
            [Value::from(1), Value::from(2), Value::from(3)]
        );
        assert!(matches!(
            ResultService::write_rank(&project.project_path, &task.id, "rank", false),
            Err(AppError::ConfirmationRequired(_))
        ));

        let source_file = fs::read_dir(project.project_path.join("source"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let source_before = fs::read(&source_file).unwrap();
        let csv_path = directory.path().join("result.csv");
        let receipt = ResultService::export(&ExportResultRequest {
            project_path: project.project_path.clone(),
            task_id: task.id.clone(),
            target_path: csv_path.clone(),
            format: ExportFormat::Csv,
            order: ExportOrder::Final,
            selected_fields: vec!["name".into(), "category".into()],
            include_rank: true,
            rank_field_name: "rank".into(),
            include_original_index: true,
            overwrite: false,
        })
        .unwrap();
        assert_eq!(receipt.row_count, 3);
        let csv = String::from_utf8(fs::read(&csv_path).unwrap()).unwrap();
        assert!(csv
            .trim_start_matches('\u{feff}')
            .starts_with("rank,original_index,name,category\n1,2,C,three"));
        assert!(matches!(
            ResultService::export(&ExportResultRequest {
                project_path: project.project_path.clone(),
                task_id: task.id.clone(),
                target_path: csv_path,
                format: ExportFormat::Csv,
                order: ExportOrder::Final,
                selected_fields: vec!["name".into()],
                include_rank: true,
                rank_field_name: "rank".into(),
                include_original_index: false,
                overwrite: false,
            }),
            Err(AppError::ConfirmationRequired(_))
        ));

        let json_path = directory.path().join("result.json");
        ResultService::export(&ExportResultRequest {
            project_path: project.project_path.clone(),
            task_id: task.id.clone(),
            target_path: json_path.clone(),
            format: ExportFormat::Json,
            order: ExportOrder::Original,
            selected_fields: vec!["name".into()],
            include_rank: true,
            rank_field_name: "rank".into(),
            include_original_index: false,
            overwrite: false,
        })
        .unwrap();
        let json: Value = serde_json::from_slice(&fs::read(json_path).unwrap()).unwrap();
        assert_eq!(json[0]["name"], "A");
        assert_eq!(json[0]["rank"], 2);
        assert_eq!(fs::read(source_file).unwrap(), source_before);

        assert!(ResultService::export(&ExportResultRequest {
            project_path: project.project_path.clone(),
            task_id: task.id,
            target_path: project.project_path.join("source").join("blocked.csv"),
            format: ExportFormat::Csv,
            order: ExportOrder::Final,
            selected_fields: vec!["name".into()],
            include_rank: false,
            rank_field_name: "rank".into(),
            include_original_index: false,
            overwrite: false,
        })
        .is_err());
    }

    #[test]
    fn unlocking_retains_latest_confirmation_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("确认版本", directory.path()).unwrap();
        let dataset = DatasetService::commit_import(CommitDatasetImport {
            project_path: project.project_path.clone(),
            dataset_name: "条目".into(),
            source: DatasetImportSource::PastedText("A\nB".into()),
            deduplicate: false,
            json_pointer: None,
            primary_identifier: "内容".into(),
            auxiliary_identifiers: vec![],
        })
        .unwrap();
        let task = SortTaskService::create(CreateSortTask {
            project_path: project.project_path.clone(),
            dataset_id: dataset.id,
            name: "确认".into(),
            criteria: "测试".into(),
            mode: SortTaskMode::Drag,
            initial_order: InitialOrder::Import,
        })
        .unwrap();
        let first = ResultService::confirm(&project.project_path, &task.id).unwrap();
        let first_time = first.confirmed_at.clone();
        let unlocked = ResultService::unlock(&project.project_path, &task.id).unwrap();
        assert_eq!(unlocked.status, SortTaskStatus::Sorting);
        assert_eq!(unlocked.confirmed_at, first_time);
    }

    #[test]
    fn writes_competition_dense_and_ordinal_ranks_for_ties() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("并列排名", directory.path()).unwrap();
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
            name: "排名规则".into(),
            criteria: "测试".into(),
            mode: SortTaskMode::Drag,
            initial_order: InitialOrder::Import,
        })
        .unwrap();
        let initial =
            SortTaskService::load_drag_workspace(&project.project_path, &task.id).unwrap();
        SortTaskService::merge_rank_groups(
            &project.project_path,
            &task.id,
            &initial.groups[1].group_id,
            &initial.groups[0].group_id,
        )
        .unwrap();
        let confirmed = ResultService::confirm(&project.project_path, &task.id).unwrap();
        assert_eq!(
            confirmed
                .items
                .iter()
                .map(|item| item.rank)
                .collect::<Vec<_>>(),
            [1, 1, 3]
        );
        for (field, rule) in [
            ("competition_rank", RankRule::Competition),
            ("dense_rank", RankRule::Dense),
            ("ordinal_rank", RankRule::Ordinal),
        ] {
            ResultService::write_rank_with_rule(
                &project.project_path,
                &task.id,
                field,
                false,
                rule,
            )
            .unwrap();
        }
        let repository =
            ProjectRepository::open(&project.project_path.join("project.sqlite")).unwrap();
        assert_eq!(
            repository
                .ranked_field_values(&task.id, "competition_rank")
                .unwrap(),
            [Value::from(1), Value::from(1), Value::from(3)]
        );
        assert_eq!(
            repository
                .ranked_field_values(&task.id, "dense_rank")
                .unwrap(),
            [Value::from(1), Value::from(1), Value::from(2)]
        );
        assert_eq!(
            repository
                .ranked_field_values(&task.id, "ordinal_rank")
                .unwrap(),
            [Value::from(1), Value::from(2), Value::from(3)]
        );
    }

    #[test]
    fn exports_markdown_table_list_and_plain_text() {
        let rows = vec![crate::domain::result::ExportRow {
            final_rank: 1,
            original_index: 0,
            fields: serde_json::Map::from_iter([
                ("名称".into(), Value::from("A|B")),
                ("说明".into(), Value::from("第一行\n第二行")),
            ]),
        }];
        let fields = vec!["名称".into(), "说明".into()];
        let table = String::from_utf8(export_markdown_table(
            &rows, &fields, true, "rank", false, None,
        ))
        .unwrap();
        assert!(table.contains("| rank | 名称 | 说明 |"));
        assert!(table.contains("A\\|B"));
        assert!(table.contains("第一行<br>第二行"));
        let list = String::from_utf8(export_markdown_list(
            &rows, &fields, false, "rank", false, None,
        ))
        .unwrap();
        assert!(list.starts_with("- **名称**: A|B · **说明**: 第一行 第二行"));
        let text =
            String::from_utf8(export_text(&rows, &fields, false, "rank", false, None)).unwrap();
        assert_eq!(text, "A|B\t第一行 第二行");
    }

    #[test]
    fn normalizes_linear_score_precision_during_export() {
        let rows = vec![crate::domain::result::ExportRow {
            final_rank: 1,
            original_index: 0,
            fields: serde_json::Map::from_iter([("score".into(), Value::from(8.335))]),
        }];
        let fields = vec!["score".into()];
        let score_format = ScoreExportFormat {
            field_name: "score".into(),
            decimal_places: 2,
        };

        let csv = String::from_utf8(
            export_csv(&rows, &fields, false, "rank", false, Some(&score_format)).unwrap(),
        )
        .unwrap();
        assert!(csv.contains("8.34"));
        assert!(!csv.contains("8.335"));

        let json = export_json(&rows, &fields, false, "rank", false, Some(&score_format)).unwrap();
        let parsed: Value = serde_json::from_slice(&json).unwrap();
        assert_eq!(parsed[0]["score"], Value::from(8.34));
    }
}
