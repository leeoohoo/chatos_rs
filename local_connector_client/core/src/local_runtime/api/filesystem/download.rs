// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::Path;

use axum::extract::{Query, State};
use axum::Json;
use base64::Engine;
use serde_json::{json, Value};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::LocalRuntime;

use super::super::error::LocalRuntimeApiError;
use super::super::workspace_path::resolve_local_workspace_path;
use super::queries::{content_type, PathQuery};

const MAX_DOWNLOAD_BYTES: u64 = 256 * 1024 * 1024;

pub(super) async fn download_entry(
    State(runtime): State<LocalRuntime>,
    Query(query): Query<PathQuery>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let resolved = resolve_local_workspace_path(&runtime, query.path.as_str(), false).await?;
    let path = resolved.path.clone();
    let result = tokio::task::spawn_blocking(move || download_bytes(path.as_path()))
        .await
        .map_err(|error| LocalRuntimeApiError::internal(error.to_string()))?
        .map_err(|error| {
            LocalRuntimeApiError::bad_request("local_runtime_fs_download_failed", error)
        })?;
    Ok(Json(json!({
        "filename": result.0,
        "content_type": result.1,
        "data_base64": base64::engine::general_purpose::STANDARD.encode(result.2),
    })))
}

fn download_bytes(path: &Path) -> Result<(String, String, Vec<u8>), String> {
    if path.is_file() {
        let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
        if metadata.len() > MAX_DOWNLOAD_BYTES {
            return Err("Local download is too large".to_string());
        }
        let bytes = fs::read(path).map_err(|error| error.to_string())?;
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("download")
            .to_string();
        return Ok((name, content_type(path, true), bytes));
    }
    if !path.is_dir() {
        return Err("Local download path is not a file or directory".to_string());
    }
    let mut cursor = Cursor::new(Vec::new());
    let mut total = 0u64;
    {
        let mut writer = ZipWriter::new(&mut cursor);
        append_directory_to_zip(&mut writer, path, path, &mut total)?;
        writer.finish().map_err(|error| error.to_string())?;
    }
    let name = format!(
        "{}.zip",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("project")
    );
    Ok((name, "application/zip".to_string(), cursor.into_inner()))
}

fn append_directory_to_zip(
    writer: &mut ZipWriter<&mut Cursor<Vec<u8>>>,
    root: &Path,
    directory: &Path,
    total: &mut u64,
) -> Result<(), String> {
    for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            append_directory_to_zip(writer, root, path.as_path(), total)?;
            continue;
        }
        *total = total.saturating_add(metadata.len());
        if *total > MAX_DOWNLOAD_BYTES {
            return Err("Local download is too large".to_string());
        }
        let name = path
            .strip_prefix(root)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        writer
            .start_file(name, SimpleFileOptions::default())
            .map_err(|error| error.to_string())?;
        let mut file = fs::File::open(path.as_path()).map_err(|error| error.to_string())?;
        let mut buffer = [0u8; 16 * 1024];
        loop {
            let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
            if count == 0 {
                break;
            }
            writer
                .write_all(&buffer[..count])
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}
