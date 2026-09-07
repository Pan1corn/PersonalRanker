use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    path::{Component, Path, PathBuf},
};

use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::Utc;
use uuid::Uuid;

use crate::{
    domain::{
        dataset::DatasetImportSource,
        import::{ImportResult, ImportedFieldType},
        media::{MediaAssetRecord, MediaFolderImportResult, MediaPreview},
    },
    error::AppError,
    repository::project_repository::ProjectRepository,
    services::{import_service::image_reference, project_service::ProjectService},
};

const MAX_PREVIEW_BYTES: u64 = 20 * 1024 * 1024;

pub struct MediaService;

impl MediaService {
    pub fn import_folder(
        project_path: &Path,
        dataset_id: &str,
        folder_path: &Path,
    ) -> Result<MediaFolderImportResult, AppError> {
        ProjectService::open(project_path)?;
        if !folder_path.is_dir() {
            return Err(AppError::InvalidInput("请选择有效的图片文件夹".into()));
        }
        let mut repository = ProjectRepository::open(&project_path.join("project.sqlite"))?;
        let image_fields = {
            let mut statement = repository.connection().prepare(
                "SELECT name FROM field_definitions WHERE dataset_id = ?1 AND field_type = 'image' ORDER BY display_order",
            )?;
            let rows = statement.query_map([dataset_id], |row| row.get::<_, String>(0))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        if image_fields.is_empty() {
            return Err(AppError::InvalidInput("当前数据表没有图片相关字段".into()));
        }
        let items = {
            let mut statement = repository.connection().prepare(
                "SELECT id, fields_json FROM items WHERE dataset_id = ?1 AND is_valid = 1 ORDER BY original_index",
            )?;
            let rows = statement.query_map([dataset_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let image_files = collect_image_files(folder_path)?;
        if image_files.is_empty() {
            return Err(AppError::InvalidInput(
                "所选文件夹中没有支持的图片文件".into(),
            ));
        }
        let folder_root = folder_path
            .canonicalize()
            .map_err(|error| AppError::from_io(error, folder_path))?;
        let mut relative_index = HashMap::<String, PathBuf>::new();
        let mut filename_index = HashMap::<String, Option<PathBuf>>::new();
        for path in &image_files {
            let relative = path.strip_prefix(&folder_root).unwrap_or(path);
            relative_index.insert(normalize_match_path(relative), path.clone());
            if let Some(filename) = path.file_name().and_then(|value| value.to_str()) {
                filename_index
                    .entry(filename.to_lowercase())
                    .and_modify(|entry| *entry = None)
                    .or_insert_with(|| Some(path.clone()));
            }
        }

        let media_directory = project_path.join("assets").join("media");
        fs::create_dir_all(&media_directory)
            .map_err(|error| AppError::from_io(error, &media_directory))?;
        let now = Utc::now().to_rfc3339();
        let mut assets = Vec::new();
        let mut referenced_items = HashSet::new();
        let mut matched_items = HashSet::new();
        for (item_id, fields_json) in items {
            let fields: serde_json::Map<String, serde_json::Value> =
                serde_json::from_str(&fields_json)?;
            for field_name in &image_fields {
                let Some(source_value) = fields.get(field_name).and_then(|value| value.as_str())
                else {
                    continue;
                };
                let Some(reference) = image_reference(source_value) else {
                    continue;
                };
                if reference.starts_with("https://") || reference.starts_with("http://") {
                    continue;
                }
                referenced_items.insert(item_id.clone());
                let reference_without_query = reference
                    .split(['?', '#'])
                    .next()
                    .unwrap_or(reference.as_str());
                let normalized = normalize_match_path(Path::new(reference_without_query));
                let matched = relative_index.get(&normalized).cloned().or_else(|| {
                    Path::new(reference_without_query)
                        .file_name()
                        .and_then(|value| value.to_str())
                        .and_then(|filename| {
                            filename_index
                                .get(&filename.to_lowercase())
                                .and_then(Clone::clone)
                        })
                });
                let Some(matched_path) = matched else {
                    continue;
                };
                let local_path = copy_media_file(&media_directory, &matched_path)?;
                matched_items.insert(item_id.clone());
                assets.push(MediaAssetRecord {
                    id: Uuid::new_v4().to_string(),
                    dataset_id: dataset_id.to_owned(),
                    item_id: item_id.clone(),
                    field_name: field_name.clone(),
                    source_kind: "local".into(),
                    source_value: reference,
                    local_path: Some(local_path),
                    created_at: now.clone(),
                });
            }
        }
        let replaced_paths = repository.replace_media_assets(&assets)?;
        for relative_path in replaced_paths {
            remove_project_media_file(project_path, &relative_path);
        }
        Ok(MediaFolderImportResult {
            image_file_count: image_files.len(),
            matched_item_count: matched_items.len(),
            matched_asset_count: assets.len(),
            unmatched_item_count: referenced_items.difference(&matched_items).count(),
        })
    }

    pub fn index_imported_media(
        project_path: &Path,
        dataset_id: &str,
        source: &DatasetImportSource,
        import: &ImportResult,
    ) -> Result<(), AppError> {
        let image_fields = import
            .fields
            .iter()
            .filter(|field| field.field_type == ImportedFieldType::Image)
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>();
        if image_fields.is_empty() {
            return Ok(());
        }
        let media_directory = project_path.join("assets").join("media");
        fs::create_dir_all(&media_directory)
            .map_err(|error| AppError::from_io(error, &media_directory))?;
        let source_base = match source {
            DatasetImportSource::File(path) => path.parent(),
            DatasetImportSource::PastedText(_) => None,
        };
        let now = Utc::now().to_rfc3339();
        let mut assets = Vec::new();
        for item in &import.items {
            for field_name in &image_fields {
                let Some(source_value) = item
                    .fields
                    .get(*field_name)
                    .and_then(|value| value.as_str())
                else {
                    continue;
                };
                let Some(reference) = image_reference(source_value) else {
                    continue;
                };
                let remote = reference.starts_with("https://") || reference.starts_with("http://");
                let local_path = if remote {
                    None
                } else {
                    copy_local_media(&media_directory, source_base, &reference)?
                };
                assets.push(MediaAssetRecord {
                    id: Uuid::new_v4().to_string(),
                    dataset_id: dataset_id.to_owned(),
                    item_id: item.id.clone(),
                    field_name: (*field_name).to_owned(),
                    source_kind: if remote { "remote" } else { "local" }.into(),
                    source_value: reference,
                    local_path,
                    created_at: now.clone(),
                });
            }
        }
        let mut repository = ProjectRepository::open(&project_path.join("project.sqlite"))?;
        repository.save_media_assets(&assets)
    }

    pub fn preview(
        project_path: &Path,
        item_id: &str,
        field_name: &str,
        allow_network: bool,
    ) -> Result<MediaPreview, AppError> {
        ProjectService::open(project_path)?;
        let repository = ProjectRepository::open(&project_path.join("project.sqlite"))?;
        let Some(asset) = repository.media_asset(item_id, field_name)? else {
            return Ok(MediaPreview::Missing);
        };
        if asset.source_kind == "remote" {
            return Ok(if allow_network {
                MediaPreview::RemoteReady {
                    url: asset.source_value,
                }
            } else {
                MediaPreview::RemoteBlocked
            });
        }
        let Some(relative_path) = asset.local_path else {
            return Ok(MediaPreview::Missing);
        };
        let assets_root = project_path
            .join("assets")
            .canonicalize()
            .map_err(|error| AppError::from_io(error, project_path))?;
        let candidate = project_path.join(relative_path);
        let resolved = match candidate.canonicalize() {
            Ok(path) if path.starts_with(&assets_root) && path.is_file() => path,
            _ => return Ok(MediaPreview::Missing),
        };
        let metadata =
            fs::metadata(&resolved).map_err(|error| AppError::from_io(error, &resolved))?;
        if metadata.len() > MAX_PREVIEW_BYTES {
            return Ok(MediaPreview::TooLarge);
        }
        let bytes = fs::read(&resolved).map_err(|error| AppError::from_io(error, &resolved))?;
        let mime = image_mime(&resolved)
            .ok_or_else(|| AppError::InvalidInput("当前图片格式不支持生成缩略图".into()))?;
        Ok(MediaPreview::LocalReady {
            data_url: format!("data:{mime};base64,{}", STANDARD.encode(bytes)),
        })
    }
}

fn copy_local_media(
    media_directory: &Path,
    source_base: Option<&Path>,
    reference: &str,
) -> Result<Option<String>, AppError> {
    let raw_path = Path::new(reference);
    let candidate = if raw_path.is_absolute() {
        raw_path.to_path_buf()
    } else if let Some(base) = source_base {
        base.join(raw_path)
    } else {
        return Ok(None);
    };
    if !candidate.is_file() {
        return Ok(None);
    }
    copy_media_file(media_directory, &candidate).map(Some)
}

fn copy_media_file(media_directory: &Path, candidate: &Path) -> Result<String, AppError> {
    let extension = candidate
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("img")
        .to_ascii_lowercase();
    let filename = format!("{}.{}", Uuid::new_v4().simple(), extension);
    let destination = media_directory.join(&filename);
    fs::copy(candidate, &destination).map_err(|error| AppError::from_io(error, candidate))?;
    Ok(Path::new("assets")
        .join("media")
        .join(filename)
        .to_string_lossy()
        .into_owned())
}

fn collect_image_files(folder_path: &Path) -> Result<Vec<PathBuf>, AppError> {
    let mut pending = VecDeque::from([folder_path.to_path_buf()]);
    let mut files = Vec::new();
    while let Some(directory) = pending.pop_front() {
        for entry in
            fs::read_dir(&directory).map_err(|error| AppError::from_io(error, &directory))?
        {
            let entry = entry.map_err(|error| AppError::from_io(error, &directory))?;
            let file_type = entry
                .file_type()
                .map_err(|error| AppError::from_io(error, &entry.path()))?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                pending.push_back(entry.path());
            } else if file_type.is_file() && image_mime(&entry.path()).is_some() {
                files.push(
                    entry
                        .path()
                        .canonicalize()
                        .map_err(|error| AppError::from_io(error, &entry.path()))?,
                );
                if files.len() > 10_000 {
                    return Err(AppError::InvalidInput(
                        "单次最多导入 10,000 个图片文件".into(),
                    ));
                }
            }
        }
    }
    Ok(files)
}

fn normalize_match_path(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
        .to_lowercase()
}

fn remove_project_media_file(project_path: &Path, relative_path: &str) {
    let relative = Path::new(relative_path);
    if !relative
        .components()
        .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return;
    }
    let assets_root = project_path.join("assets");
    let candidate = project_path.join(relative);
    if candidate.starts_with(assets_root) {
        let _ = fs::remove_file(candidate);
    }
}

fn image_mime(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        "svg" => Some("image/svg+xml"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        domain::dataset::{CommitDatasetImport, DatasetImportSource},
        services::{dataset_service::DatasetService, project_service::ProjectService},
    };

    use super::*;

    #[test]
    fn copies_local_images_and_blocks_remote_images_by_default() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("图片预览", directory.path()).unwrap();
        let input_directory = directory.path().join("input");
        fs::create_dir(&input_directory).unwrap();
        fs::write(input_directory.join("cover.png"), b"fake-png").unwrap();
        let json_path = input_directory.join("items.json");
        fs::write(
            &json_path,
            r#"[{"name":"A","image":"cover.png"},{"name":"B","image":"https://example.com/b.webp"}]"#,
        )
        .unwrap();
        let dataset = DatasetService::commit_import(CommitDatasetImport {
            project_path: project.project_path.clone(),
            dataset_name: "图片".into(),
            source: DatasetImportSource::File(json_path),
            deduplicate: false,
            json_pointer: None,
            primary_identifier: "name".into(),
            auxiliary_identifiers: vec!["image".into()],
        })
        .unwrap();
        let repository =
            ProjectRepository::open(&project.project_path.join("project.sqlite")).unwrap();
        let mut statement = repository
            .connection()
            .prepare("SELECT item_id, source_kind FROM media_assets WHERE dataset_id = ?1 ORDER BY rowid")
            .unwrap();
        let records = statement
            .query_map([dataset.id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        drop(statement);
        drop(repository);

        let local =
            MediaService::preview(&project.project_path, &records[0].0, "image", false).unwrap();
        assert!(
            matches!(local, MediaPreview::LocalReady { data_url } if data_url.starts_with("data:image/png;base64,"))
        );
        let blocked =
            MediaService::preview(&project.project_path, &records[1].0, "image", false).unwrap();
        assert_eq!(blocked, MediaPreview::RemoteBlocked);
        let enabled =
            MediaService::preview(&project.project_path, &records[1].0, "image", true).unwrap();
        assert_eq!(
            enabled,
            MediaPreview::RemoteReady {
                url: "https://example.com/b.webp".into()
            }
        );
    }

    #[test]
    fn imports_image_folder_by_matching_path_filenames() {
        let directory = tempfile::tempdir().unwrap();
        let project = ProjectService::create("图片文件夹", directory.path()).unwrap();
        let data_path = directory.path().join("items.json");
        fs::write(
            &data_path,
            r#"[{"name":"A","image":"legacy/a.png"},{"name":"B","image":"b.jpg"},{"name":"C","image":"missing.webp"}]"#,
        )
        .unwrap();
        let dataset = DatasetService::commit_import(CommitDatasetImport {
            project_path: project.project_path.clone(),
            dataset_name: "图片条目".into(),
            source: DatasetImportSource::File(data_path),
            deduplicate: false,
            json_pointer: None,
            primary_identifier: "name".into(),
            auxiliary_identifiers: vec!["image".into()],
        })
        .unwrap();
        let image_folder = directory.path().join("photos");
        fs::create_dir(&image_folder).unwrap();
        fs::write(image_folder.join("a.png"), b"png").unwrap();
        fs::write(image_folder.join("b.jpg"), b"jpg").unwrap();

        let result =
            MediaService::import_folder(&project.project_path, &dataset.id, &image_folder).unwrap();
        assert_eq!(result.image_file_count, 2);
        assert_eq!(result.matched_item_count, 2);
        assert_eq!(result.matched_asset_count, 2);
        assert_eq!(result.unmatched_item_count, 1);

        let repository =
            ProjectRepository::open(&project.project_path.join("project.sqlite")).unwrap();
        let first_item = repository
            .connection()
            .query_row(
                "SELECT id FROM items WHERE dataset_id = ?1 ORDER BY original_index LIMIT 1",
                [&dataset.id],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        drop(repository);
        assert!(matches!(
            MediaService::preview(&project.project_path, &first_item, "image", false).unwrap(),
            MediaPreview::LocalReady { .. }
        ));
    }
}
