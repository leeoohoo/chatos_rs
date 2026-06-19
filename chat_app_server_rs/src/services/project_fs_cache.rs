use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::services::project_local_cache::{
    cache_key, read_cache_json, remove_cache_file, write_cache_json,
};

const FS_CACHE_NAMESPACE: &str = "fs";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedFsEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub writable: Option<bool>,
    pub size: Option<u64>,
    pub modified_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CachedFsEntryFingerprint {
    name: String,
    is_dir: bool,
    size: Option<u64>,
    modified_unix_millis: Option<u128>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedDirectoryListing {
    path: String,
    include_files: bool,
    directory_modified_unix_millis: Option<u128>,
    fingerprints: Vec<CachedFsEntryFingerprint>,
    entries: Vec<CachedFsEntry>,
}

pub fn directory_cache_relative_path(directory_path: &Path, include_files: bool) -> String {
    let key = format!(
        "{}|{}",
        normalize_path_string(directory_path),
        if include_files { "entries" } else { "dirs" }
    );
    format!(
        "{FS_CACHE_NAMESPACE}/listing-{}.json",
        cache_key(key.as_str())
    )
}

pub fn read_cached_directory_listing(
    project_root: &str,
    directory_path: &Path,
    include_files: bool,
) -> Result<Option<Vec<Value>>, String> {
    let cache_path = directory_cache_relative_path(directory_path, include_files);
    let Some(cached) =
        read_cache_json::<CachedDirectoryListing>(project_root, cache_path.as_str())?
    else {
        return Ok(None);
    };

    if cached.path != normalize_path_string(directory_path) || cached.include_files != include_files
    {
        return Ok(None);
    }
    if current_directory_modified_millis(directory_path) != cached.directory_modified_unix_millis {
        return Ok(None);
    }

    Ok(Some(
        cached
            .entries
            .into_iter()
            .map(|entry| {
                json!({
                    "name": entry.name,
                    "path": entry.path,
                    "is_dir": entry.is_dir,
                    "writable": entry.writable,
                    "size": entry.size,
                    "modified_at": entry.modified_at,
                })
            })
            .collect(),
    ))
}

pub fn write_cached_directory_listing(
    project_root: &str,
    directory_path: &Path,
    include_files: bool,
    entries: &[Value],
) -> Result<(), String> {
    let cache_entry = CachedDirectoryListing {
        path: normalize_path_string(directory_path),
        include_files,
        directory_modified_unix_millis: current_directory_modified_millis(directory_path),
        fingerprints: collect_directory_fingerprints(directory_path, include_files)?,
        entries: entries
            .iter()
            .map(value_to_cached_entry)
            .collect::<Vec<_>>(),
    };
    write_cache_json(
        project_root,
        directory_cache_relative_path(directory_path, include_files).as_str(),
        &cache_entry,
    )
}

pub fn invalidate_directory_listing_cache_for_path(
    project_root: &str,
    path: &Path,
) -> Result<(), String> {
    for directory in affected_directories(path) {
        remove_cache_file(
            project_root,
            directory_cache_relative_path(directory.as_path(), true).as_str(),
        )?;
        remove_cache_file(
            project_root,
            directory_cache_relative_path(directory.as_path(), false).as_str(),
        )?;
    }
    Ok(())
}

fn affected_directories(path: &Path) -> Vec<PathBuf> {
    let mut directories = Vec::new();
    directories.push(path.to_path_buf());
    if let Some(parent) = path.parent() {
        directories.push(parent.to_path_buf());
    }
    directories.sort();
    directories.dedup();
    directories
}

fn collect_directory_fingerprints(
    directory_path: &Path,
    include_files: bool,
) -> Result<Vec<CachedFsEntryFingerprint>, String> {
    let mut fingerprints = Vec::new();
    let iter = fs::read_dir(directory_path).map_err(|err| err.to_string())?;
    for entry in iter {
        let entry = match entry {
            Ok(value) => value,
            Err(_) => continue,
        };
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(value) => value,
            Err(_) => continue,
        };
        let metadata = match fs::metadata(&path) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let is_dir = metadata.is_dir();
        if !is_dir && !include_files {
            continue;
        }
        if file_type.is_symlink() && fs::canonicalize(&path).is_err() {
            continue;
        }
        fingerprints.push(CachedFsEntryFingerprint {
            name: entry.file_name().to_string_lossy().to_string(),
            is_dir,
            size: if is_dir { None } else { Some(metadata.len()) },
            modified_unix_millis: system_time_to_millis(metadata.modified().ok()),
        });
    }
    fingerprints.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.is_dir.cmp(&right.is_dir))
    });
    Ok(fingerprints)
}

fn current_directory_modified_millis(path: &Path) -> Option<u128> {
    fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis())
}

fn system_time_to_millis(value: Option<SystemTime>) -> Option<u128> {
    value
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis())
}

fn value_to_cached_entry(value: &Value) -> CachedFsEntry {
    CachedFsEntry {
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        path: value
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        is_dir: value
            .get("is_dir")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        writable: value.get("writable").and_then(Value::as_bool),
        size: value.get("size").and_then(Value::as_u64),
        modified_at: value
            .get("modified_at")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
    }
}

fn normalize_path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
