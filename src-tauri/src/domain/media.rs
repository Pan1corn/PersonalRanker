use serde::Serialize;

#[derive(Debug, Clone)]
pub(crate) struct MediaAssetRecord {
    pub id: String,
    pub dataset_id: String,
    pub item_id: String,
    pub field_name: String,
    pub source_kind: String,
    pub source_value: String,
    pub local_path: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum MediaPreview {
    LocalReady { data_url: String },
    RemoteReady { url: String },
    RemoteBlocked,
    Missing,
    TooLarge,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MediaFolderImportResult {
    pub image_file_count: usize,
    pub matched_item_count: usize,
    pub matched_asset_count: usize,
    pub unmatched_item_count: usize,
}
