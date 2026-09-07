use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotType {
    ImportCompleted,
    ComparisonCompleted,
    SliderCompleted,
    SortConfirmed,
    ScoreConfirmed,
    ExportCompleted,
    BeforeRestore,
}

impl SnapshotType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ImportCompleted => "import_completed",
            Self::ComparisonCompleted => "comparison_completed",
            Self::SliderCompleted => "slider_completed",
            Self::SortConfirmed => "sort_confirmed",
            Self::ScoreConfirmed => "score_confirmed",
            Self::ExportCompleted => "export_completed",
            Self::BeforeRestore => "before_restore",
        }
    }

    pub const fn default_label(self) -> &'static str {
        match self {
            Self::ImportCompleted => "数据导入完成",
            Self::ComparisonCompleted => "1v1 排序完成",
            Self::SliderCompleted => "滑杆排序完成",
            Self::SortConfirmed => "排序结果确认",
            Self::ScoreConfirmed => "评分写入确认",
            Self::ExportCompleted => "正式导出",
            Self::BeforeRestore => "恢复快照前的安全备份",
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSnapshot {
    pub id: String,
    pub snapshot_type: String,
    pub label: String,
    pub source_task_id: Option<String>,
    pub source_dataset_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AutosaveStatus {
    pub session_open: bool,
    pub recovered_unclean_session: bool,
    pub last_autosave_at: Option<DateTime<Utc>>,
    pub last_autosave_action: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenProjectResult {
    pub project: crate::domain::project::ProjectMetadata,
    pub autosave: AutosaveStatus,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreSnapshotResult {
    pub snapshot: ProjectSnapshot,
    pub autosave: AutosaveStatus,
}
