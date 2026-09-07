//! 多格式导入的纯解析层。
//! 各格式先转换为统一的“原始序号 + 字段映射”，再经 `finalize` 完成去重、统计和 UUID 分配。

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs,
    path::Path,
};

use chrono::{DateTime, NaiveDate};
use csv::StringRecord;
use serde_json::{Map, Value};
use thiserror::Error;
use uuid::Uuid;

use crate::domain::import::{
    ImportResult, ImportSourceType, ImportedFieldDefinition, ImportedFieldType, ImportedItem,
    JsonArrayNode,
};

const MINIMUM_ITEM_COUNT: usize = 2;

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("无法读取导入文件“{path}”：{reason}")]
    FileRead { path: String, reason: String },
    #[error("文件不是有效的 UTF-8 编码，请将文件另存为 UTF-8 后重试。")]
    InvalidEncoding,
    #[error("不支持的导入格式“{0}”，请选择 .txt、.csv、.json 或 .md 文件。")]
    UnsupportedFormat(String),
    #[error("文本中没有可导入的非空行。")]
    EmptyText,
    #[error("CSV 中没有可识别的记录。")]
    EmptyCsv,
    #[error("CSV 第 {line} 行解析失败：{reason}")]
    InvalidCsv { line: u64, reason: String },
    #[error("Markdown 中未找到至少两个表格数据行或列表项。")]
    InvalidMarkdown,
    #[error("JSON 解析失败（第 {line} 行，第 {column} 列）：{reason}")]
    InvalidJson {
        line: usize,
        column: usize,
        reason: String,
    },
    #[error("JSON 中未找到可用的对象数组。")]
    NoUsableJsonArray,
    #[error("找不到 JSON 数组节点“{0}”。")]
    JsonNodeNotFound(String),
    #[error("JSON 数组第 {index} 项不是对象，当前版本仅支持对象数组。")]
    JsonItemNotObject { index: usize },
    #[error("清理空条目和去重后仅剩 {found} 个条目；至少需要 {required} 个条目才能继续。")]
    TooFewItems { found: usize, required: usize },
}

impl ImportError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::FileRead { .. } => "IMPORT_FILE_READ",
            Self::InvalidEncoding => "IMPORT_INVALID_ENCODING",
            Self::UnsupportedFormat(_) => "IMPORT_UNSUPPORTED_FORMAT",
            Self::EmptyText => "IMPORT_EMPTY_TEXT",
            Self::EmptyCsv => "IMPORT_EMPTY_CSV",
            Self::InvalidCsv { .. } => "IMPORT_INVALID_CSV",
            Self::InvalidMarkdown => "IMPORT_INVALID_MARKDOWN",
            Self::InvalidJson { .. } => "IMPORT_INVALID_JSON",
            Self::NoUsableJsonArray => "IMPORT_NO_JSON_ARRAY",
            Self::JsonNodeNotFound(_) => "IMPORT_JSON_NODE_NOT_FOUND",
            Self::JsonItemNotObject { .. } => "IMPORT_JSON_ITEM_NOT_OBJECT",
            Self::TooFewItems { .. } => "IMPORT_TOO_FEW_ITEMS",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ImportOptions<'a> {
    pub deduplicate: bool,
    pub json_pointer: Option<&'a str>,
}

pub struct ImportService;

impl ImportService {
    pub fn import_file(
        path: &Path,
        options: ImportOptions<'_>,
    ) -> Result<ImportResult, ImportError> {
        let bytes = fs::read(path).map_err(|error| ImportError::FileRead {
            path: path.display().to_string(),
            reason: error.to_string(),
        })?;
        let content = decode_utf8(&bytes)?;
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        match extension.as_str() {
            "txt" => Self::import_text(content, options.deduplicate),
            "csv" => Self::import_csv(content, options.deduplicate),
            "json" => Self::import_json(content, options.json_pointer, options.deduplicate),
            "md" | "markdown" => Self::import_markdown(content, options.deduplicate),
            _ => Err(ImportError::UnsupportedFormat(extension)),
        }
    }

    pub fn import_text(content: &str, deduplicate: bool) -> Result<ImportResult, ImportError> {
        let mut candidates: Vec<(usize, BTreeMap<String, Value>)> = Vec::new();
        let mut removed_empty_count = 0;
        for (original_index, line) in content.lines().enumerate() {
            let value = line.trim();
            if value.is_empty() {
                removed_empty_count += 1;
                continue;
            }
            let fields = BTreeMap::from([("内容".to_owned(), Value::String(value.to_owned()))]);
            candidates.push((original_index, fields));
        }
        if candidates.is_empty() {
            return Err(ImportError::EmptyText);
        }

        let fields = vec![ImportedFieldDefinition {
            name: "内容".to_owned(),
            field_type: ImportedFieldType::Text,
            display_order: 0,
            empty_count: 0,
            unique_count: candidates
                .iter()
                .filter_map(|(_, fields)| fields.get("内容"))
                .collect::<HashSet<_>>()
                .len(),
            samples: candidates
                .iter()
                .filter_map(|(_, fields)| fields.get("内容").cloned())
                .take(3)
                .collect(),
        }];
        finalize(
            ImportSourceType::Text,
            fields,
            candidates,
            removed_empty_count,
            deduplicate,
        )
    }

    pub fn import_csv(content: &str, deduplicate: bool) -> Result<ImportResult, ImportError> {
        let mut reader = csv::ReaderBuilder::new()
            .flexible(false)
            .trim(csv::Trim::All)
            .from_reader(content.as_bytes());
        let raw_headers = reader.headers().map_err(map_csv_error)?.clone();
        if raw_headers.is_empty() {
            return Err(ImportError::EmptyCsv);
        }
        let headers = normalize_headers(&raw_headers);
        let mut candidates = Vec::new();
        let mut removed_empty_count = 0;
        for (original_index, result) in reader.records().enumerate() {
            let record = result.map_err(map_csv_error)?;
            if record.iter().all(|value| value.trim().is_empty()) {
                removed_empty_count += 1;
                continue;
            }
            let values = headers
                .iter()
                .zip(record.iter())
                .map(|(header, value)| (header.clone(), Value::String(value.trim().to_owned())))
                .collect();
            candidates.push((original_index, values));
        }
        if candidates.is_empty() {
            return Err(ImportError::EmptyCsv);
        }

        let fields = headers
            .iter()
            .enumerate()
            .map(|(display_order, name)| {
                build_field_definition(
                    name,
                    infer_text_field_type(name, &candidates),
                    display_order,
                    &candidates,
                )
            })
            .collect();

        finalize(
            ImportSourceType::Csv,
            fields,
            candidates,
            removed_empty_count,
            deduplicate,
        )
    }

    pub fn import_json(
        content: &str,
        json_pointer: Option<&str>,
        deduplicate: bool,
    ) -> Result<ImportResult, ImportError> {
        let root = parse_json(content)?;
        let selected = select_json_array(&root, json_pointer)?;
        let mut candidates: Vec<(usize, BTreeMap<String, Value>)> = Vec::new();
        let mut removed_empty_count = 0;

        for (original_index, value) in selected.iter().enumerate() {
            let object = value.as_object().ok_or(ImportError::JsonItemNotObject {
                index: original_index,
            })?;
            if object_is_empty(object) {
                removed_empty_count += 1;
                continue;
            }
            candidates.push((
                original_index,
                object
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect(),
            ));
        }

        let field_names: BTreeSet<String> = candidates
            .iter()
            .flat_map(|(_, fields)| fields.keys().cloned())
            .collect();
        let fields = field_names
            .iter()
            .enumerate()
            .map(|(display_order, name)| ImportedFieldDefinition {
                ..build_field_definition(
                    name,
                    infer_json_field_type(name, &candidates),
                    display_order,
                    &candidates,
                )
            })
            .collect();

        finalize(
            ImportSourceType::Json,
            fields,
            candidates,
            removed_empty_count,
            deduplicate,
        )
    }

    pub fn import_markdown(content: &str, deduplicate: bool) -> Result<ImportResult, ImportError> {
        let lines = content.lines().collect::<Vec<_>>();
        if let Some(separator_index) = lines.iter().position(|line| is_table_separator(line)) {
            if separator_index > 0 {
                let header_values = split_markdown_row(lines[separator_index - 1]);
                let separators = split_markdown_row(lines[separator_index]);
                if !header_values.is_empty() && header_values.len() == separators.len() {
                    let headers = normalize_headers(&StringRecord::from(header_values));
                    let mut candidates = Vec::new();
                    let mut removed_empty_count = 0;
                    for (original_index, line) in lines.iter().enumerate().skip(separator_index + 1)
                    {
                        if line.trim().is_empty() {
                            if !candidates.is_empty() {
                                break;
                            }
                            continue;
                        }
                        if !line.contains('|') {
                            break;
                        }
                        let cells = split_markdown_row(line);
                        if cells.iter().all(|cell| cell.trim().is_empty()) {
                            removed_empty_count += 1;
                            continue;
                        }
                        let fields = headers
                            .iter()
                            .enumerate()
                            .map(|(index, header)| {
                                (
                                    header.clone(),
                                    Value::String(cells.get(index).cloned().unwrap_or_default()),
                                )
                            })
                            .collect();
                        candidates.push((original_index, fields));
                    }
                    if candidates.len() >= MINIMUM_ITEM_COUNT {
                        let fields = headers
                            .iter()
                            .enumerate()
                            .map(|(display_order, name)| {
                                build_field_definition(
                                    name,
                                    infer_text_field_type(name, &candidates),
                                    display_order,
                                    &candidates,
                                )
                            })
                            .collect();
                        return finalize(
                            ImportSourceType::Markdown,
                            fields,
                            candidates,
                            removed_empty_count,
                            deduplicate,
                        );
                    }
                }
            }
        }

        let candidates = lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| {
                strip_markdown_list_marker(line).map(|value| {
                    (
                        index,
                        BTreeMap::from([("内容".to_owned(), Value::String(value.to_owned()))]),
                    )
                })
            })
            .collect::<Vec<_>>();
        if candidates.len() < MINIMUM_ITEM_COUNT {
            return Err(ImportError::InvalidMarkdown);
        }
        let fields = vec![build_field_definition(
            "内容",
            infer_text_field_type("内容", &candidates),
            0,
            &candidates,
        )];
        finalize(
            ImportSourceType::Markdown,
            fields,
            candidates,
            0,
            deduplicate,
        )
    }

    pub fn list_json_array_nodes(content: &str) -> Result<Vec<JsonArrayNode>, ImportError> {
        let root = parse_json(content)?;
        let mut nodes = Vec::new();
        collect_json_array_nodes(&root, "", &mut nodes);
        if nodes.is_empty() {
            return Err(ImportError::NoUsableJsonArray);
        }
        Ok(nodes)
    }

    pub fn list_json_array_nodes_from_file(path: &Path) -> Result<Vec<JsonArrayNode>, ImportError> {
        let bytes = fs::read(path).map_err(|error| ImportError::FileRead {
            path: path.display().to_string(),
            reason: error.to_string(),
        })?;
        Self::list_json_array_nodes(decode_utf8(&bytes)?)
    }
}

fn decode_utf8(bytes: &[u8]) -> Result<&str, ImportError> {
    let without_bom = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    std::str::from_utf8(without_bom).map_err(|_| ImportError::InvalidEncoding)
}

fn parse_json(content: &str) -> Result<Value, ImportError> {
    serde_json::from_str(content).map_err(|error| ImportError::InvalidJson {
        line: error.line(),
        column: error.column(),
        reason: error.to_string(),
    })
}

fn select_json_array<'a>(
    root: &'a Value,
    json_pointer: Option<&str>,
) -> Result<&'a Vec<Value>, ImportError> {
    if let Some(pointer) = json_pointer {
        return root
            .pointer(pointer)
            .and_then(Value::as_array)
            .ok_or_else(|| ImportError::JsonNodeNotFound(pointer.to_owned()));
    }
    if let Some(array) = root.as_array() {
        return Ok(array);
    }
    let mut nodes = Vec::new();
    collect_json_array_nodes(root, "", &mut nodes);
    let first = nodes.first().ok_or(ImportError::NoUsableJsonArray)?;
    root.pointer(&first.pointer)
        .and_then(Value::as_array)
        .ok_or(ImportError::NoUsableJsonArray)
}

fn collect_json_array_nodes(value: &Value, pointer: &str, nodes: &mut Vec<JsonArrayNode>) {
    // 深度遍历时同步构造 RFC 6901 指针，供前端让用户选择嵌套对象数组。
    match value {
        Value::Array(values) => {
            if !values.is_empty() && values.iter().all(Value::is_object) {
                nodes.push(JsonArrayNode {
                    pointer: pointer.to_owned(),
                    item_count: values.len(),
                });
            }
            for (index, child) in values.iter().enumerate() {
                collect_json_array_nodes(child, &format!("{pointer}/{index}"), nodes);
            }
        }
        Value::Object(object) => {
            for (key, child) in object {
                let escaped = key.replace('~', "~0").replace('/', "~1");
                collect_json_array_nodes(child, &format!("{pointer}/{escaped}"), nodes);
            }
        }
        _ => {}
    }
}

fn normalize_headers(headers: &StringRecord) -> Vec<String> {
    // 生成稳定且唯一的列名，避免后续用 JSON 对象保存一行时发生键覆盖。
    let mut used = HashSet::new();
    headers
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let base = if value.trim().is_empty() {
                format!("字段{}", index + 1)
            } else {
                value.trim().to_owned()
            };
            let mut candidate = base.clone();
            let mut suffix = 2;
            while !used.insert(candidate.clone()) {
                candidate = format!("{base}_{suffix}");
                suffix += 1;
            }
            candidate
        })
        .collect()
}

fn map_csv_error(error: csv::Error) -> ImportError {
    ImportError::InvalidCsv {
        line: error.position().map_or(0, csv::Position::line),
        reason: error.to_string(),
    }
}

fn object_is_empty(object: &Map<String, Value>) -> bool {
    object.is_empty()
        || object.values().all(|value| {
            value.is_null() || value.as_str().is_some_and(|text| text.trim().is_empty())
        })
}

fn infer_json_field_type(
    field_name: &str,
    candidates: &[(usize, BTreeMap<String, Value>)],
) -> ImportedFieldType {
    // JSON 保留原生类型；只要同一列出现多种非一致类型，就按 mixed 展示而不强制转换。
    let present_values = candidates
        .iter()
        .filter_map(|(_, fields)| fields.get(field_name))
        .collect::<Vec<_>>();
    let image_value_count = present_values
        .iter()
        .filter(|value| {
            value
                .as_str()
                .is_some_and(|value| image_reference(value).is_some())
        })
        .count();
    if image_value_count > 0
        && present_values.iter().all(|value| {
            value.is_null()
                || value.as_str().is_some_and(|value| {
                    value.trim().is_empty() || image_reference(value).is_some()
                })
        })
    {
        return ImportedFieldType::Image;
    }
    let types: BTreeSet<u8> = candidates
        .iter()
        .filter_map(|(_, fields)| fields.get(field_name))
        .map(|value| match value {
            Value::Null => 0,
            Value::String(_) => 1,
            Value::Number(_) => 2,
            Value::Bool(_) => 3,
            Value::Object(_) => 4,
            Value::Array(_) => 5,
        })
        .collect();
    if types.len() != 1 {
        return ImportedFieldType::Mixed;
    }
    match types.first().copied() {
        Some(0) => ImportedFieldType::Null,
        Some(1) => ImportedFieldType::Text,
        Some(2) => ImportedFieldType::Number,
        Some(3) => ImportedFieldType::Boolean,
        Some(4) => ImportedFieldType::Object,
        Some(5) => ImportedFieldType::Array,
        _ => ImportedFieldType::Mixed,
    }
}

fn infer_text_field_type(
    field_name: &str,
    candidates: &[(usize, BTreeMap<String, Value>)],
) -> ImportedFieldType {
    // CSV/Markdown 没有类型元数据，采用“整列都满足”原则，防止局部样例造成错误推断。
    let values: Vec<&str> = candidates
        .iter()
        .filter_map(|(_, fields)| fields.get(field_name)?.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect();
    if values.is_empty() {
        return ImportedFieldType::Null;
    }
    if values.iter().all(|value| image_reference(value).is_some()) {
        return ImportedFieldType::Image;
    }
    if values
        .iter()
        .all(|value| value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("false"))
    {
        return ImportedFieldType::Boolean;
    }
    if values.iter().all(|value| value.parse::<f64>().is_ok()) {
        return ImportedFieldType::Number;
    }
    if values.iter().all(|value| is_date(value)) {
        return ImportedFieldType::Date;
    }
    ImportedFieldType::Text
}

pub(crate) fn image_reference(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let target = if trimmed.starts_with("![") {
        let open = trimmed.find("](")? + 2;
        let close = trimmed[open..].find(')')? + open;
        trimmed[open..close].trim()
    } else {
        trimmed
    };
    let without_query = target.split(['?', '#']).next().unwrap_or(target);
    let extension = Path::new(without_query)
        .extension()
        .and_then(|value| value.to_str())?
        .to_ascii_lowercase();
    ["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg"]
        .contains(&extension.as_str())
        .then(|| target.to_owned())
}

fn is_table_separator(line: &str) -> bool {
    let cells = split_markdown_row(line);
    !cells.is_empty()
        && cells.iter().all(|cell| {
            let marker = cell.trim().trim_matches(':').trim();
            marker.len() >= 3 && marker.chars().all(|character| character == '-')
        })
}

fn split_markdown_row(line: &str) -> Vec<String> {
    let trimmed = line.trim().trim_start_matches('|').trim_end_matches('|');
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut escaped = false;
    for character in trimmed.chars() {
        if escaped {
            current.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '|' {
            cells.push(current.trim().to_owned());
            current.clear();
        } else {
            current.push(character);
        }
    }
    if escaped {
        current.push('\\');
    }
    cells.push(current.trim().to_owned());
    cells
}

fn strip_markdown_list_marker(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    for marker in ["- ", "* ", "+ "] {
        if let Some(value) = trimmed.strip_prefix(marker) {
            return (!value.trim().is_empty()).then(|| value.trim());
        }
    }
    let (number, value) = trimmed.split_once(". ")?;
    (!number.is_empty()
        && number.chars().all(|character| character.is_ascii_digit())
        && !value.trim().is_empty())
    .then(|| value.trim())
}

fn is_date(value: &str) -> bool {
    DateTime::parse_from_rfc3339(value).is_ok()
        || ["%Y-%m-%d", "%Y/%m/%d", "%Y-%m-%d %H:%M:%S"]
            .iter()
            .any(|format| NaiveDate::parse_from_str(value, format).is_ok())
}

fn build_field_definition(
    name: &str,
    field_type: ImportedFieldType,
    display_order: usize,
    candidates: &[(usize, BTreeMap<String, Value>)],
) -> ImportedFieldDefinition {
    let values: Vec<&Value> = candidates
        .iter()
        .filter_map(|(_, fields)| fields.get(name))
        .collect();
    let empty_count =
        candidates.len() - values.iter().filter(|value| !is_empty_value(value)).count();
    let unique_count = values
        .iter()
        .filter(|value| !is_empty_value(value))
        .filter_map(|value| serde_json::to_string(value).ok())
        .collect::<HashSet<_>>()
        .len();
    let samples = values
        .into_iter()
        .filter(|value| !is_empty_value(value))
        .take(3)
        .cloned()
        .collect();
    ImportedFieldDefinition {
        name: name.to_owned(),
        field_type,
        display_order,
        empty_count,
        unique_count,
        samples,
    }
}

fn is_empty_value(value: &Value) -> bool {
    value.is_null() || value.as_str().is_some_and(|text| text.trim().is_empty())
}

fn finalize(
    source_type: ImportSourceType,
    mut fields: Vec<ImportedFieldDefinition>,
    candidates: Vec<(usize, BTreeMap<String, Value>)>,
    removed_empty_count: usize,
    deduplicate: bool,
) -> Result<ImportResult, ImportError> {
    // 序列化标准化字段作为整行身份；即使去重，也保留首条记录在源文件中的 original_index。
    let mut seen = HashSet::new();
    let mut duplicate_count = 0;
    let mut items = Vec::with_capacity(candidates.len());
    for (original_index, fields) in candidates {
        let identity = serde_json::to_string(&fields).expect("标准化字段必须可序列化");
        let is_duplicate = !seen.insert(identity);
        if is_duplicate {
            duplicate_count += 1;
            if deduplicate {
                continue;
            }
        }
        items.push(ImportedItem {
            id: Uuid::new_v4().to_string(),
            original_index,
            fields,
        });
    }
    if items.len() < MINIMUM_ITEM_COUNT {
        return Err(ImportError::TooFewItems {
            found: items.len(),
            required: MINIMUM_ITEM_COUNT,
        });
    }
    refresh_field_stats(&mut fields, &items);
    Ok(ImportResult {
        source_type,
        fields,
        items,
        removed_empty_count,
        duplicate_count,
        deduplicated: deduplicate,
    })
}

fn refresh_field_stats(fields: &mut [ImportedFieldDefinition], items: &[ImportedItem]) {
    for field in fields {
        let values: Vec<&Value> = items
            .iter()
            .filter_map(|item| item.fields.get(&field.name))
            .collect();
        field.empty_count =
            items.len() - values.iter().filter(|value| !is_empty_value(value)).count();
        field.unique_count = values
            .iter()
            .filter(|value| !is_empty_value(value))
            .filter_map(|value| serde_json::to_string(value).ok())
            .collect::<HashSet<_>>()
            .len();
        field.samples = values
            .into_iter()
            .filter(|value| !is_empty_value(value))
            .take(3)
            .cloned()
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_text_and_removes_empty_lines() {
        let result = ImportService::import_text(" 第一项 \n\n第二项\n", false).unwrap();
        assert_eq!(result.items.len(), 2);
        assert_eq!(result.removed_empty_count, 1);
        assert_eq!(result.items[0].original_index, 0);
        assert_eq!(result.items[1].original_index, 2);
        assert_ne!(result.items[0].id, result.items[1].id);
    }

    #[test]
    fn imports_markdown_table_and_escaped_pipes() {
        let result = ImportService::import_markdown(
            "| 名称 | 说明 | 图片 |\n| --- | :--- | ---: |\n| A | 甲\\|乙 | ./a.png |\n| B | 丙 | https://example.com/b.webp |",
            false,
        )
        .unwrap();
        assert_eq!(result.source_type, ImportSourceType::Markdown);
        assert_eq!(result.items.len(), 2);
        assert_eq!(result.items[0].fields["说明"], "甲|乙");
        assert_eq!(result.fields[2].field_type, ImportedFieldType::Image);
    }

    #[test]
    fn imports_ordered_and_unordered_markdown_lists() {
        let result =
            ImportService::import_markdown("- 第一项\n* 第二项\n3. 第三项", false).unwrap();
        assert_eq!(result.items.len(), 3);
        assert_eq!(result.items[2].fields["内容"], "第三项");
    }

    #[test]
    fn rejects_markdown_without_a_table_or_list() {
        assert!(matches!(
            ImportService::import_markdown("# 标题\n普通段落", false),
            Err(ImportError::InvalidMarkdown)
        ));
    }

    #[test]
    fn optionally_deduplicates_without_losing_original_index() {
        let result = ImportService::import_text("A\nA\nB", true).unwrap();
        assert_eq!(result.items.len(), 2);
        assert_eq!(result.duplicate_count, 1);
        assert_eq!(result.items[1].original_index, 2);
    }

    #[test]
    fn imports_csv_bom_and_normalizes_headers() {
        let result = ImportService::import_csv("\u{feff}名称,,名称\nA,x,1\nB,y,2", false).unwrap();
        let names: Vec<_> = result
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect();
        assert_eq!(names, ["名称", "字段2", "名称_2"]);
        assert_eq!(result.items.len(), 2);
        assert_eq!(result.fields[2].field_type, ImportedFieldType::Number);
        assert_eq!(result.fields[0].empty_count, 0);
        assert_eq!(result.fields[0].unique_count, 2);
        assert_eq!(result.fields[0].samples.len(), 2);
    }

    #[test]
    fn infers_dates_and_recalculates_stats_after_deduplication() {
        let result = ImportService::import_csv(
            "name,date,note\nA,2026-08-01,\nA,2026-08-01,\nB,2026-08-02,x",
            true,
        )
        .unwrap();
        assert_eq!(result.items.len(), 2);
        assert_eq!(result.fields[1].field_type, ImportedFieldType::Date);
        assert_eq!(result.fields[0].unique_count, 2);
        assert_eq!(result.fields[2].empty_count, 1);
    }

    #[test]
    fn reports_csv_line_for_malformed_record() {
        let error = ImportService::import_csv("a,b\n1,2\n3", false).unwrap_err();
        assert!(matches!(error, ImportError::InvalidCsv { line: 3, .. }));
    }

    #[test]
    fn lists_and_imports_nested_json_array() {
        let content = r#"{"data":{"items":[{"name":"A","score":1},{"name":"B","score":2}]}}"#;
        let nodes = ImportService::list_json_array_nodes(content).unwrap();
        assert_eq!(
            nodes,
            [JsonArrayNode {
                pointer: "/data/items".into(),
                item_count: 2
            }]
        );
        let result = ImportService::import_json(content, Some("/data/items"), false).unwrap();
        assert_eq!(result.items.len(), 2);
        assert_eq!(result.fields[1].field_type, ImportedFieldType::Number);
    }

    #[test]
    fn supports_json_pointer_escaped_keys() {
        let content = r#"{"a/b":{"~items":[{"v":1},{"v":2}]}}"#;
        let nodes = ImportService::list_json_array_nodes(content).unwrap();
        assert_eq!(nodes[0].pointer, "/a~1b/~0items");
    }

    #[test]
    fn rejects_non_object_json_items() {
        let error = ImportService::import_json("[1,2]", None, false).unwrap_err();
        assert!(matches!(error, ImportError::JsonItemNotObject { index: 0 }));
    }

    #[test]
    fn rejects_too_few_items_after_cleanup() {
        let error = ImportService::import_text("only one", false).unwrap_err();
        assert!(matches!(
            error,
            ImportError::TooFewItems {
                found: 1,
                required: 2
            }
        ));
    }

    #[test]
    fn rejects_invalid_utf8_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("bad.txt");
        fs::write(&path, [0xFF, 0xFE]).unwrap();
        let error = ImportService::import_file(&path, ImportOptions::default()).unwrap_err();
        assert!(matches!(error, ImportError::InvalidEncoding));
    }

    #[test]
    fn imports_all_supported_formats_from_files() {
        let directory = tempfile::tempdir().unwrap();
        let samples = [
            ("items.txt", "A\nB", ImportSourceType::Text),
            ("items.csv", "name\nA\nB", ImportSourceType::Csv),
            (
                "items.json",
                r#"[{"name":"A"},{"name":"B"}]"#,
                ImportSourceType::Json,
            ),
            ("items.md", "- 第一项\n- 第二项", ImportSourceType::Markdown),
        ];
        for (filename, content, expected_type) in samples {
            let path = directory.path().join(filename);
            fs::write(&path, content).unwrap();
            let result = ImportService::import_file(&path, ImportOptions::default()).unwrap();
            assert_eq!(result.source_type, expected_type);
            assert_eq!(result.items.len(), 2);
        }
    }

    #[test]
    fn reports_when_json_has_no_usable_object_array() {
        let error = ImportService::list_json_array_nodes(r#"{"values":[1,2]}"#).unwrap_err();
        assert!(matches!(error, ImportError::NoUsableJsonArray));
    }
}
