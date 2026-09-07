use std::{io, path::Path};

use serde::Serialize;
use thiserror::Error;

use crate::services::import_service::ImportError;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("参数无效：{0}")]
    InvalidInput(String),
    #[error("需要确认：{0}")]
    ConfirmationRequired(String),
    #[error("项目已经存在：{0}")]
    AlreadyExists(String),
    #[error("找不到有效的项目：{0}")]
    InvalidProject(String),
    #[error("项目版本不受支持：当前为 {found}，本应用支持 {supported}")]
    UnsupportedVersion { found: u32, supported: u32 },
    #[error("项目数据库结构不受支持；仅支持由 v1.0.0 正式版创建的项目")]
    UnsupportedDatabaseSchema,
    #[error("所选目录不可写：{0}")]
    NotWritable(String),
    #[error("读取或写入文件失败：{0}")]
    Io(#[from] io::Error),
    #[error("生成 CSV 文件失败：{0}")]
    Csv(#[from] csv::Error),
    #[error("数据库操作失败：{0}")]
    Database(#[from] rusqlite::Error),
    #[error("项目元数据格式错误：{0}")]
    Metadata(#[from] serde_json::Error),
    #[error(transparent)]
    Import(#[from] ImportError),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppErrorPayload {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl AppError {
    pub fn from_io(error: io::Error, path: &Path) -> Self {
        if error.kind() == io::ErrorKind::PermissionDenied {
            Self::NotWritable(path.display().to_string())
        } else {
            Self::Io(error)
        }
    }
}

impl From<AppError> for AppErrorPayload {
    fn from(error: AppError) -> Self {
        let (code, message) = match &error {
            AppError::InvalidInput(_) => ("INVALID_INPUT", error.to_string()),
            AppError::ConfirmationRequired(_) => ("CONFIRMATION_REQUIRED", error.to_string()),
            AppError::AlreadyExists(_) => ("PROJECT_EXISTS", error.to_string()),
            AppError::InvalidProject(_) => ("INVALID_PROJECT", error.to_string()),
            AppError::UnsupportedVersion { .. } => ("UNSUPPORTED_VERSION", error.to_string()),
            AppError::UnsupportedDatabaseSchema => {
                ("UNSUPPORTED_DATABASE_SCHEMA", error.to_string())
            }
            AppError::NotWritable(_) => ("NOT_WRITABLE", error.to_string()),
            AppError::Io(_) => ("FILE_IO", "无法读写项目文件，请检查路径和权限。".into()),
            AppError::Csv(_) => ("CSV_EXPORT", "无法生成 CSV 导出文件。".into()),
            AppError::Database(_) => ("DATABASE", "无法保存项目数据库，数据未被静默忽略。".into()),
            AppError::Metadata(_) => (
                "INVALID_METADATA",
                "metadata.json 内容损坏或格式不正确。".into(),
            ),
            AppError::Import(import_error) => (import_error.code(), import_error.to_string()),
        };
        Self {
            code,
            message,
            detail: Some(error.to_string()),
        }
    }
}
