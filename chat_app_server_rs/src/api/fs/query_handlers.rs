use axum::http::StatusCode;
use axum::response::Response;
use axum::{extract::Query, Json};
use base64::Engine;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

use super::contracts::{
    FsContentSearchQuery, FsDownloadQuery, FsQuery, FsReadQuery, FsSearchQuery,
};
use super::helpers::{format_system_time, infer_download_name, read_dir_entries, zip_directory};
use super::read_mode::should_render_text;
use super::response::{binary_download_response, json_error_response};
use super::roots::list_roots;
use super::search::{is_search_match, normalize_search_keyword};
use crate::services::workspace_search::{
    search_text as search_workspace_text, TextSearchRequest, DEFAULT_MAX_FILE_BYTES,
    DEFAULT_MAX_VISITS,
};

const MAX_PREVIEW_BYTES: u64 = 2 * 1024 * 1024;
const DEFAULT_SEARCH_LIMIT: usize = 200;
const MAX_SEARCH_LIMIT: usize = 500;
const MAX_SEARCH_VISITS: usize = 20_000;

pub(super) async fn download_entry(Query(query): Query<FsDownloadQuery>) -> Response {
    let raw = query
        .path
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if raw.is_none() {
        return json_error_response(StatusCode::BAD_REQUEST, "路径不能为空");
    }

    let path = PathBuf::from(raw.unwrap());
    if !path.exists() {
        return json_error_response(StatusCode::BAD_REQUEST, "路径不存在");
    }

    if path.is_file() {
        let data = match fs::read(&path) {
            Ok(data) => data,
            Err(err) => {
                return json_error_response(StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
            }
        };
        let name = infer_download_name(&path);
        let mime = mime_guess::from_path(&path).first_or_octet_stream();
        return binary_download_response(data, mime.essence_str(), &name);
    }

    if path.is_dir() {
        let zip_data = match zip_directory(&path) {
            Ok(data) => data,
            Err(err) => return json_error_response(StatusCode::INTERNAL_SERVER_ERROR, err),
        };
        let base_name = infer_download_name(&path);
        let file_name = if base_name.ends_with(".zip") {
            base_name
        } else {
            format!("{base_name}.zip")
        };
        return binary_download_response(zip_data, "application/zip", &file_name);
    }

    json_error_response(StatusCode::BAD_REQUEST, "路径既不是文件也不是目录")
}

pub(super) async fn list_dirs(Query(query): Query<FsQuery>) -> (StatusCode, Json<Value>) {
    let raw = query
        .path
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if raw.is_none() {
        let roots = list_roots();
        return (
            StatusCode::OK,
            Json(json!({
                "path": Value::Null,
                "parent": Value::Null,
                "entries": Vec::<Value>::new(),
                "roots": roots
            })),
        );
    }

    let path = PathBuf::from(raw.unwrap());
    if !path.exists() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "路径不存在" })),
        );
    }
    if !path.is_dir() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "路径不是目录" })),
        );
    }

    let entries = match read_dir_entries(&path, false) {
        Ok(v) => v,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": err })),
            )
        }
    };
    let parent = path.parent().map(|p| p.to_string_lossy().to_string());

    (
        StatusCode::OK,
        Json(json!({
            "path": path.to_string_lossy(),
            "parent": parent,
            "entries": entries,
            "roots": Vec::<Value>::new()
        })),
    )
}

pub(super) async fn list_entries(Query(query): Query<FsQuery>) -> (StatusCode, Json<Value>) {
    let raw = query
        .path
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if raw.is_none() {
        let roots = list_roots();
        return (
            StatusCode::OK,
            Json(json!({
                "path": Value::Null,
                "parent": Value::Null,
                "entries": Vec::<Value>::new(),
                "roots": roots
            })),
        );
    }

    let path = PathBuf::from(raw.unwrap());
    if !path.exists() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "路径不存在" })),
        );
    }
    if !path.is_dir() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "路径不是目录" })),
        );
    }

    let entries = match read_dir_entries(&path, true) {
        Ok(v) => v,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": err })),
            )
        }
    };
    let parent = path.parent().map(|p| p.to_string_lossy().to_string());

    (
        StatusCode::OK,
        Json(json!({
            "path": path.to_string_lossy(),
            "parent": parent,
            "entries": entries,
            "roots": Vec::<Value>::new()
        })),
    )
}

pub(super) async fn search_entries(
    Query(query): Query<FsSearchQuery>,
) -> (StatusCode, Json<Value>) {
    let raw_path = query
        .path
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if raw_path.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "搜索路径不能为空" })),
        );
    }

    let raw_keyword = query
        .q
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if raw_keyword.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "搜索关键字不能为空" })),
        );
    }

    let path = PathBuf::from(raw_path.unwrap());
    if !path.exists() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "路径不存在" })),
        );
    }
    if !path.is_dir() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "路径不是目录" })),
        );
    }

    let limit = query
        .limit
        .unwrap_or(DEFAULT_SEARCH_LIMIT)
        .clamp(1, MAX_SEARCH_LIMIT);
    let keyword = normalize_search_keyword(&raw_keyword.unwrap());

    let mut stack = vec![path.clone()];
    let mut entries: Vec<Value> = Vec::new();
    let mut visited_dirs = 0usize;
    let mut truncated = false;

    while let Some(dir_path) = stack.pop() {
        if visited_dirs >= MAX_SEARCH_VISITS {
            truncated = true;
            break;
        }
        visited_dirs += 1;

        let iter = match fs::read_dir(&dir_path) {
            Ok(v) => v,
            Err(_) => continue,
        };

        for entry in iter {
            if entries.len() >= limit {
                truncated = true;
                break;
            }

            let entry = match entry {
                Ok(v) => v,
                Err(_) => continue,
            };
            let meta = match entry.metadata() {
                Ok(v) => v,
                Err(_) => continue,
            };

            let full_path = entry.path();
            if meta.is_dir() {
                stack.push(full_path);
                continue;
            }

            let name = entry.file_name().to_string_lossy().to_string();
            let relative_path = full_path
                .strip_prefix(&path)
                .ok()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| full_path.to_string_lossy().to_string());

            if !is_search_match(&name, &relative_path, &keyword) {
                continue;
            }

            let modified_at = meta.modified().ok().and_then(format_system_time);
            entries.push(json!({
                "name": name,
                "path": full_path.to_string_lossy(),
                "relative_path": relative_path,
                "is_dir": false,
                "size": Some(meta.len()),
                "modified_at": modified_at
            }));
        }

        if truncated {
            break;
        }
    }

    entries.sort_by(|a, b| {
        let ap = a
            .get("relative_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();
        let bp = b
            .get("relative_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();
        ap.cmp(&bp)
    });

    (
        StatusCode::OK,
        Json(json!({
            "path": path.to_string_lossy(),
            "query": keyword,
            "entries": entries,
            "truncated": truncated,
            "visited_dirs": visited_dirs
        })),
    )
}

pub(super) async fn search_content(
    Query(query): Query<FsContentSearchQuery>,
) -> (StatusCode, Json<Value>) {
    let raw_path = query
        .path
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if raw_path.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "搜索路径不能为空" })),
        );
    }

    let raw_keyword = query
        .q
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if raw_keyword.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "搜索关键字不能为空" })),
        );
    }

    let path = PathBuf::from(raw_path.unwrap());
    let query_text = raw_keyword.unwrap();
    let limit = query
        .limit
        .unwrap_or(DEFAULT_SEARCH_LIMIT)
        .clamp(1, MAX_SEARCH_LIMIT);

    match search_workspace_text(&TextSearchRequest {
        root: path.clone(),
        query: query_text.clone(),
        max_results: limit,
        max_file_bytes: DEFAULT_MAX_FILE_BYTES,
        max_visits: DEFAULT_MAX_VISITS,
        case_sensitive: query.case_sensitive.unwrap_or(false),
        whole_word: query.whole_word.unwrap_or(false),
    }) {
        Ok(result) => (
            StatusCode::OK,
            Json(json!({
                "path": path.to_string_lossy(),
                "query": query_text,
                "entries": result.entries,
                "truncated": result.truncated,
                "visited_dirs": result.visited_dirs
            })),
        ),
        Err(message) if message == "路径不存在" || message == "路径不是目录" => {
            (StatusCode::BAD_REQUEST, Json(json!({ "error": message })))
        }
        Err(message) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": message })),
        ),
    }
}

pub(super) async fn read_file(Query(query): Query<FsReadQuery>) -> (StatusCode, Json<Value>) {
    let raw = query
        .path
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if raw.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "路径不能为空" })),
        );
    }
    let path = PathBuf::from(raw.unwrap());
    if !path.exists() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "路径不存在" })),
        );
    }
    if !path.is_file() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "路径不是文件" })),
        );
    }

    let meta = match fs::metadata(&path) {
        Ok(m) => m,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": err.to_string() })),
            )
        }
    };
    let size = meta.len();
    if size > MAX_PREVIEW_BYTES {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({
                "error": "文件过大，无法预览",
                "size": size,
                "limit": MAX_PREVIEW_BYTES
            })),
        );
    }

    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": err.to_string() })),
            )
        }
    };

    let mime = mime_guess::from_path(&path).first_or_octet_stream();
    let content_type = mime.essence_str().to_string();
    let should_render = should_render_text(&path, &bytes, &content_type);

    let (is_binary, content) = if should_render {
        (
            false,
            Value::String(String::from_utf8_lossy(&bytes).to_string()),
        )
    } else {
        (
            true,
            Value::String(base64::engine::general_purpose::STANDARD.encode(&bytes)),
        )
    };

    let modified_at = meta.modified().ok().and_then(format_system_time);

    (
        StatusCode::OK,
        Json(json!({
            "path": path.to_string_lossy(),
            "name": path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
            "size": size,
            "content_type": content_type,
            "is_binary": is_binary,
            "modified_at": modified_at,
            "content": content
        })),
    )
}
