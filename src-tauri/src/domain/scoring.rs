use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RankRule {
    #[default]
    Competition,
    Dense,
    Ordinal,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TieScoreRule {
    Average,
    Highest,
    Lowest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ScoreMethod {
    Linear {
        highest_score: f64,
        lowest_score: f64,
        decimal_places: u8,
        high_rank_high_score: bool,
    },
    Buckets {
        levels: Vec<f64>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScoreConfig {
    pub field_name: String,
    pub method: ScoreMethod,
    pub tie_rule: TieScoreRule,
}

#[derive(Debug, Clone)]
pub struct ScorePreviewRequest {
    pub project_path: PathBuf,
    pub task_id: String,
    pub config: ScoreConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScorePreviewItem {
    pub group_id: String,
    pub group_name: Option<String>,
    pub item_id: String,
    pub primary_label: String,
    pub rank: usize,
    pub score: f64,
    pub old_value: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScorePreview {
    pub task_id: String,
    pub field_name: String,
    pub field_exists: bool,
    pub can_write: bool,
    pub items: Vec<ScorePreviewItem>,
}

#[derive(Debug, Clone)]
pub(crate) struct ScoringItem {
    pub item_id: String,
    pub primary_label: String,
    pub fields: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone)]
pub(crate) struct ScoringGroup {
    pub group_id: String,
    pub group_name: Option<String>,
    pub items: Vec<ScoringItem>,
}
