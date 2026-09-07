use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::domain::import::{ImportSourceType, ImportedFieldType};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SavedFieldDefinition {
    pub id: String,
    pub name: String,
    pub field_type: ImportedFieldType,
    pub display_order: usize,
    pub is_primary_identifier: bool,
    pub is_auxiliary_identifier: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DatasetOverview {
    pub id: String,
    pub name: String,
    pub source_type: ImportSourceType,
    pub source_filename: Option<String>,
    pub item_count: usize,
    pub fields: Vec<SavedFieldDefinition>,
}

#[derive(Debug, Clone)]
pub enum DatasetImportSource {
    File(PathBuf),
    PastedText(String),
}

#[derive(Debug, Clone)]
pub struct CommitDatasetImport {
    pub project_path: PathBuf,
    pub dataset_name: String,
    pub source: DatasetImportSource,
    pub deduplicate: bool,
    pub json_pointer: Option<String>,
    pub primary_identifier: String,
    pub auxiliary_identifiers: Vec<String>,
}
