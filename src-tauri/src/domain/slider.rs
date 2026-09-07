use serde::{Deserialize, Serialize};

use super::{comparison::ComparisonItem, sort_task::SortTaskStatus};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SliderRating {
    pub item: ComparisonItem,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SliderWorkspace {
    pub task_id: String,
    pub task_name: String,
    pub criteria: String,
    pub status: SortTaskStatus,
    pub current: Option<ComparisonItem>,
    pub completed_count: usize,
    pub total_count: usize,
    pub progress_percent: usize,
    pub completed: bool,
    pub ratings: Vec<SliderRating>,
}
