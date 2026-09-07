use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SortTaskMode {
    Drag,
    Matrix,
    Comparison,
    Slider,
}

impl SortTaskMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Drag => "drag",
            Self::Matrix => "matrix",
            Self::Comparison => "comparison",
            Self::Slider => "slider",
        }
    }

    pub fn from_storage(value: &str) -> Option<Self> {
        match value {
            "drag" => Some(Self::Drag),
            "matrix" => Some(Self::Matrix),
            "comparison" => Some(Self::Comparison),
            "slider" => Some(Self::Slider),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SortTaskStatus {
    Draft,
    Sorting,
    Confirmed,
}

impl SortTaskStatus {
    pub fn from_storage(value: &str) -> Option<Self> {
        match value {
            "draft" => Some(Self::Draft),
            "sorting" => Some(Self::Sorting),
            "confirmed" => Some(Self::Confirmed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SortDirection {
    Ascending,
    Descending,
}

impl SortDirection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ascending => "ascending",
            Self::Descending => "descending",
        }
    }

    pub fn from_storage(value: &str) -> Option<Self> {
        match value {
            "ascending" => Some(Self::Ascending),
            "descending" => Some(Self::Descending),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum InitialOrder {
    Import,
    Field {
        field_name: String,
        direction: SortDirection,
    },
    TaskResult {
        task_id: String,
    },
}

#[derive(Debug, Clone)]
pub struct CreateSortTask {
    pub project_path: PathBuf,
    pub dataset_id: String,
    pub name: String,
    pub criteria: String,
    pub mode: SortTaskMode,
    pub initial_order: InitialOrder,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SortTaskOverview {
    pub id: String,
    pub dataset_id: String,
    pub name: String,
    pub criteria: String,
    pub mode: SortTaskMode,
    pub status: SortTaskStatus,
    pub initial_order: InitialOrder,
    pub matrix_comparison_percent: Option<u8>,
    pub rank_group_count: usize,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SortDisplayField {
    pub name: String,
    pub value: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RankedItem {
    pub group_id: String,
    pub item_id: String,
    pub position: usize,
    pub original_position: usize,
    pub primary_label: String,
    pub auxiliary_fields: Vec<SortDisplayField>,
    pub fields: Vec<SortDisplayField>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RankedGroup {
    pub group_id: String,
    pub name: Option<String>,
    pub position: usize,
    pub starting_rank: usize,
    pub items: Vec<RankedItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DragWorkspace {
    pub task_id: String,
    pub task_name: String,
    pub criteria: String,
    pub status: SortTaskStatus,
    pub items: Vec<RankedItem>,
    pub groups: Vec<RankedGroup>,
    pub has_comparisons: bool,
    pub can_undo: bool,
    pub can_redo: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MoveConflictResolution {
    UpdateRelations,
    Temporary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MoveConflictPreview {
    pub conflict_count: usize,
    pub descriptions: Vec<String>,
}
