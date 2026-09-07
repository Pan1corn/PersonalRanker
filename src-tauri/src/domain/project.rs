use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// v1.0.0 首次公开版本的项目格式标识，按主版本、次版本、修订号编码。
/// 预发布阶段使用的格式 `1` 不再受支持，也不会被自动迁移。
pub const CURRENT_DATA_FORMAT_VERSION: u32 = 10_000;
pub const PROJECT_EXTENSION: &str = "subject-sort";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMetadata {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub last_opened_at: DateTime<Utc>,
    pub data_format_version: u32,
    #[serde(skip_deserializing, default)]
    pub project_path: PathBuf,
}
