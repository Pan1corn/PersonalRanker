use serde::{Deserialize, Serialize};

use super::sort_task::{SortDisplayField, SortTaskMode, SortTaskStatus};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonDecision {
    LeftBefore,
    RightBefore,
    Tie,
}

impl ComparisonDecision {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LeftBefore => "left_before",
            Self::RightBefore => "right_before",
            Self::Tie => "tie",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ComparisonItem {
    pub group_id: String,
    pub item_id: String,
    pub item_ids: Vec<String>,
    pub member_labels: Vec<String>,
    pub group_name: Option<String>,
    pub primary_label: String,
    pub auxiliary_fields: Vec<SortDisplayField>,
    pub fields: Vec<SortDisplayField>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ComparisonWorkspace {
    pub task_id: String,
    pub task_name: String,
    pub criteria: String,
    pub status: SortTaskStatus,
    pub mode: SortTaskMode,
    pub left: Option<ComparisonItem>,
    pub right: Option<ComparisonItem>,
    pub located_count: usize,
    pub total_count: usize,
    pub comparison_count: usize,
    pub estimated_remaining: usize,
    pub pending_count: usize,
    pub progress_percent: usize,
    pub can_undo: bool,
    pub completed: bool,
    pub ordered_items: Vec<ComparisonItem>,
    pub planned_comparison_count: Option<usize>,
    pub matrix_comparison_percent: Option<u8>,
    pub standings: Vec<MatrixStanding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MatrixStanding {
    pub item: ComparisonItem,
    pub wins: usize,
    pub losses: usize,
    pub ties: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CandidateState {
    pub group_id: String,
    pub low: Option<usize>,
    pub high: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ComparisonSessionState {
    pub sorted_group_ids: Vec<String>,
    pub queue: Vec<CandidateState>,
}

impl ComparisonSessionState {
    pub fn initialize(group_ids: Vec<String>) -> Self {
        let mut groups = group_ids.into_iter();
        let sorted_group_ids = groups.next().into_iter().collect();
        let queue = groups
            .map(|group_id| CandidateState {
                group_id,
                low: None,
                high: None,
            })
            .collect();
        Self {
            sorted_group_ids,
            queue,
        }
    }

    pub fn prepare_active(&mut self) {
        if let Some(active) = self.queue.first_mut() {
            if active.low.is_none() || active.high.is_none() {
                active.low = Some(0);
                active.high = Some(self.sorted_group_ids.len());
            }
        }
    }

    pub fn midpoint(&self) -> Option<usize> {
        let active = self.queue.first()?;
        let low = active.low?;
        let high = active.high?;
        (low < high).then_some(low + (high - low) / 2)
    }
}
