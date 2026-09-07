use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::sort_task::{SortDisplayField, SortTaskStatus};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResultIntegrity {
    pub all_items_included: bool,
    pub no_unresolved_comparisons: bool,
    pub no_duplicate_items: bool,
    pub no_invalid_items: bool,
    pub problems: Vec<String>,
}

impl ResultIntegrity {
    pub fn is_valid(&self) -> bool {
        self.all_items_included
            && self.no_unresolved_comparisons
            && self.no_duplicate_items
            && self.no_invalid_items
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResultItem {
    pub group_id: String,
    pub group_name: Option<String>,
    pub tie_size: usize,
    pub item_id: String,
    pub rank: usize,
    pub original_rank: usize,
    pub rank_change: i64,
    pub primary_label: String,
    pub fields: Vec<SortDisplayField>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResultPreview {
    pub task_id: String,
    pub task_name: String,
    pub status: SortTaskStatus,
    pub items: Vec<ResultItem>,
    pub integrity: ResultIntegrity,
    pub can_confirm: bool,
    pub confirmed_at: Option<String>,
    pub rank_written_at: Option<String>,
    pub rank_written_field: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Csv,
    Json,
    MarkdownTable,
    MarkdownList,
    Text,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportOrder {
    Final,
    Original,
}

#[derive(Debug, Clone)]
pub struct ExportResultRequest {
    pub project_path: PathBuf,
    pub task_id: String,
    pub target_path: PathBuf,
    pub format: ExportFormat,
    pub order: ExportOrder,
    pub selected_fields: Vec<String>,
    pub include_rank: bool,
    pub rank_field_name: String,
    pub include_original_index: bool,
    pub overwrite: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExportResultReceipt {
    pub path: PathBuf,
    pub row_count: usize,
    pub format: ExportFormat,
}

#[derive(Debug)]
pub(crate) struct ExportRow {
    pub final_rank: usize,
    pub original_index: usize,
    pub fields: serde_json::Map<String, Value>,
}
