use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use walkdir::DirEntry;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ProjectSymbolIndexSnapshot {
    pub(super) files: Vec<ProjectSymbolFileFingerprint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ProjectSymbolFileFingerprint {
    pub(super) relative_path: String,
    pub(super) size: u64,
    pub(super) modified_unix_nanos: u128,
}

pub(super) fn fingerprint_symbol_file(
    root: &Path,
    path: &Path,
) -> Option<ProjectSymbolFileFingerprint> {
    let metadata = fs::metadata(path).ok()?;
    let normalized_path = normalize_path(path);
    let relative_path = pathdiff::diff_paths(normalized_path.as_path(), root)
        .unwrap_or_else(|| normalized_path.clone())
        .to_string_lossy()
        .replace('\\', "/");
    Some(ProjectSymbolFileFingerprint {
        relative_path,
        size: metadata.len(),
        modified_unix_nanos: metadata
            .modified()
            .ok()
            .map(system_time_to_unix_nanos)
            .unwrap_or(0),
    })
}

pub(super) fn read_line_preview(path: &Path, line: usize) -> Result<String, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    Ok(content
        .lines()
        .nth(line.saturating_sub(1))
        .unwrap_or("")
        .trim_end_matches('\r')
        .chars()
        .take(400)
        .collect())
}

pub(super) fn should_visit_path(entry: &DirEntry, ignored_dirs: &[&str]) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    let Some(name) = entry.file_name().to_str() else {
        return true;
    };
    !ignored_dirs.contains(&name)
}

pub(super) fn extension_matches(path: &Path, extensions: &[&str]) -> bool {
    let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
        return false;
    };
    extensions
        .iter()
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
}

pub(super) fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn system_time_to_unix_nanos(value: SystemTime) -> u128 {
    value
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_nanos()
}
