use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ImportSourceType {
    Text,
    Csv,
    Json,
    Markdown,
}

impl ImportSourceType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Csv => "csv",
            Self::Json => "json",
            Self::Markdown => "markdown",
        }
    }

    pub fn from_storage(value: &str) -> Option<Self> {
        match value {
            "text" => Some(Self::Text),
            "csv" => Some(Self::Csv),
            "json" => Some(Self::Json),
            "markdown" => Some(Self::Markdown),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportedFieldType {
    Text,
    Image,
    Number,
    Boolean,
    Date,
    Null,
    Object,
    Array,
    Mixed,
}

impl ImportedFieldType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Image => "image",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Date => "date",
            Self::Null => "null",
            Self::Object => "object",
            Self::Array => "array",
            Self::Mixed => "mixed",
        }
    }

    pub fn from_storage(value: &str) -> Option<Self> {
        match value {
            "text" => Some(Self::Text),
            "image" => Some(Self::Image),
            "number" => Some(Self::Number),
            "boolean" => Some(Self::Boolean),
            "date" => Some(Self::Date),
            "null" => Some(Self::Null),
            "object" => Some(Self::Object),
            "array" => Some(Self::Array),
            "mixed" => Some(Self::Mixed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportedFieldDefinition {
    pub name: String,
    pub field_type: ImportedFieldType,
    pub display_order: usize,
    pub empty_count: usize,
    pub unique_count: usize,
    pub samples: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportedItem {
    pub id: String,
    pub original_index: usize,
    pub fields: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub source_type: ImportSourceType,
    pub fields: Vec<ImportedFieldDefinition>,
    pub items: Vec<ImportedItem>,
    pub removed_empty_count: usize,
    pub duplicate_count: usize,
    pub deduplicated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct JsonArrayNode {
    /// RFC 6901 JSON Pointer。根数组使用空字符串。
    pub pointer: String,
    pub item_count: usize,
}
