use std::collections::BTreeSet;
use std::path::Path;

use serde_json::Value;
use tokio::fs;
use uuid::Uuid;

use super::NoteMeta;

pub(super) fn normalize_user_segment(user_id: &str) -> String {
    let raw = user_id.trim();
    if raw.is_empty() {
        return "task_runner".to_string();
    }
    let cleaned = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if cleaned.is_empty() {
        "task_runner".to_string()
    } else {
        cleaned
    }
}

pub(super) fn folder_segments(folder: &str) -> Vec<&str> {
    folder
        .split('/')
        .filter(|segment| !segment.trim().is_empty())
        .collect()
}

pub(super) fn normalize_folder(folder: &str) -> Result<String, String> {
    let raw = folder.trim().replace('\\', "/");
    if raw.is_empty() {
        return Err("folder is required".to_string());
    }
    let mut out = Vec::new();
    for segment in raw.split('/') {
        let normalized = segment.trim();
        if normalized.is_empty() || normalized == "." || normalized == ".." {
            return Err("folder contains invalid path segments".to_string());
        }
        out.push(normalized.to_string());
    }
    Ok(out.join("/"))
}

pub(super) fn normalize_optional_folder(folder: String) -> Result<Option<String>, String> {
    if folder.trim().is_empty() {
        Ok(None)
    } else {
        normalize_folder(folder.as_str()).map(Some)
    }
}

pub(super) fn normalize_required(value: &str, label: &str) -> Result<String, String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        Err(format!("{label} is required"))
    } else {
        Ok(normalized.to_string())
    }
}

pub(super) fn value_string(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_string()
}

pub(super) fn value_string_array(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn optional_non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(super) fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    tags.into_iter()
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .filter(|tag| seen.insert(tag.to_ascii_lowercase()))
        .collect()
}

pub(super) fn derive_title(requested_title: &str, content: &str) -> String {
    if let Some(title) = optional_non_empty(requested_title.to_string()) {
        return title;
    }
    for line in content.lines() {
        let trimmed = line.trim();
        let trimmed = trimmed.strip_prefix('#').unwrap_or(trimmed).trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    "Untitled".to_string()
}

pub(super) fn filter_notes(
    notes: &mut Vec<NoteMeta>,
    folder: Option<&str>,
    recursive: bool,
    tags: &[String],
    match_any: bool,
    query: &str,
) {
    if let Some(folder) = folder {
        let prefix = format!("{folder}/");
        notes.retain(|note| {
            note.folder == folder || (recursive && note.folder.starts_with(prefix.as_str()))
        });
    }
    if !tags.is_empty() {
        let normalized_tags = tags
            .iter()
            .map(|tag| tag.to_ascii_lowercase())
            .collect::<Vec<_>>();
        notes.retain(|note| {
            let note_tags = note
                .tags
                .iter()
                .map(|tag| tag.to_ascii_lowercase())
                .collect::<BTreeSet<_>>();
            if match_any {
                normalized_tags
                    .iter()
                    .any(|tag| note_tags.contains(tag.as_str()))
            } else {
                normalized_tags
                    .iter()
                    .all(|tag| note_tags.contains(tag.as_str()))
            }
        });
    }
    if !query.is_empty() {
        let needle = query.to_ascii_lowercase();
        notes.retain(|note| {
            note.title.to_ascii_lowercase().contains(needle.as_str())
                || note.folder.to_ascii_lowercase().contains(needle.as_str())
        });
    }
}

pub(super) async fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|err| err.to_string())?;
    }
    let tmp = path.with_extension(format!("{}.tmp", Uuid::new_v4().simple()));
    fs::write(&tmp, bytes)
        .await
        .map_err(|err| err.to_string())?;
    fs::rename(&tmp, path).await.map_err(|err| err.to_string())
}

pub(super) fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}
