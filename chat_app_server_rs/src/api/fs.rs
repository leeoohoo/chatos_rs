use axum::http::StatusCode;
use axum::response::Response;
use axum::{
    extract::Query,
    routing::{get, post},
    Json, Router,
};
use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use walkdir::WalkDir;
use zip::write::FileOptions;

mod read_mode;
mod response;
mod roots;
mod search;

use self::read_mode::should_render_text;
use self::response::{binary_download_response, json_error_response};
use self::roots::list_roots;
use self::search::{is_search_match, normalize_search_keyword};

const MAX_PREVIEW_BYTES: u64 = 2 * 1024 * 1024;
const DEFAULT_SEARCH_LIMIT: usize = 200;
const MAX_SEARCH_LIMIT: usize = 500;
const MAX_SEARCH_VISITS: usize = 20_000;

#[derive(Debug, Deserialize)]
struct FsQuery {
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FsReadQuery {
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FsSearchQuery {
    path: Option<String>,
    q: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct FsMkdirRequest {
    parent_path: Option<String>,
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FsCreateFileRequest {
    parent_path: Option<String>,
    name: Option<String>,
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FsDeleteRequest {
    path: Option<String>,
    recursive: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct FsMoveRequest {
    source_path: Option<String>,
    target_parent_path: Option<String>,
    target_name: Option<String>,
    replace_existing: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct FsDownloadQuery {
    path: Option<String>,
}

pub fn router() -> Router {
    Router::new()
        .route("/api/fs/list", get(list_dirs))
        .route("/api/fs/entries", get(list_entries))
        .route("/api/fs/search", get(search_entries))
        .route("/api/fs/mkdir", post(create_dir))
        .route("/api/fs/touch", post(create_file))
        .route("/api/fs/delete", post(delete_entry))
        .route("/api/fs/move", post(move_entry))
        .route("/api/fs/download", get(download_entry))
        .route("/api/fs/read", get(read_file))
}

async fn create_dir(Json(req): Json<FsMkdirRequest>) -> (StatusCode, Json<Value>) {
    let parent_raw = req
        .parent_path
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if parent_raw.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "父目录不能为空" })),
        );
    }

    let name_raw = req
        .name
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if name_raw.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "目录名称不能为空" })),
        );
    }

    let name = name_raw.unwrap();
    if name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.contains('\0')
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "目录名称不合法" })),
        );
    }

    let parent = PathBuf::from(parent_raw.unwrap());
    if !parent.exists() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "父目录不存在" })),
        );
    }
    if !parent.is_dir() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "父路径不是目录" })),
        );
    }

    let target = parent.join(&name);
    if target.exists() {
        if target.is_dir() {
            return (
                StatusCode::OK,
                Json(json!({
                    "path": target.to_string_lossy(),
                    "parent": parent.to_string_lossy(),
                    "name": name,
                    "created": false
                })),
            );
        }
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "同名文件已存在" })),
        );
    }

    if let Err(err) = fs::create_dir(&target) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        );
    }

    (
        StatusCode::CREATED,
        Json(json!({
            "path": target.to_string_lossy(),
            "parent": parent.to_string_lossy(),
            "name": name,
            "created": true
        })),
    )
}

async fn create_file(Json(req): Json<FsCreateFileRequest>) -> (StatusCode, Json<Value>) {
    let parent_raw = req
        .parent_path
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if parent_raw.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "父目录不能为空" })),
        );
    }

    let name_raw = req
        .name
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if name_raw.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "文件名称不能为空" })),
        );
    }

    let name = name_raw.unwrap();
    if !is_valid_entry_name(&name) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "文件名称不合法" })),
        );
    }

    let parent = PathBuf::from(parent_raw.unwrap());
    if !parent.exists() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "父目录不存在" })),
        );
    }
    if !parent.is_dir() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "父路径不是目录" })),
        );
    }

    let target = parent.join(&name);
    if target.exists() {
        if target.is_file() {
            let size = fs::metadata(&target).map(|meta| meta.len()).unwrap_or(0);
            return (
                StatusCode::OK,
                Json(json!({
                    "path": target.to_string_lossy(),
                    "parent": parent.to_string_lossy(),
                    "name": name,
                    "size": size,
                    "created": false
                })),
            );
        }
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "同名目录已存在" })),
        );
    }

    let mut file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
    {
        Ok(file) => file,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": err.to_string() })),
            )
        }
    };

    if let Some(content) = req.content {
        if let Err(err) = file.write_all(content.as_bytes()) {
            let _ = fs::remove_file(&target);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": err.to_string() })),
            );
        }
    }

    let size = fs::metadata(&target).map(|meta| meta.len()).unwrap_or(0);
    (
        StatusCode::CREATED,
        Json(json!({
            "path": target.to_string_lossy(),
            "parent": parent.to_string_lossy(),
            "name": name,
            "size": size,
            "created": true
        })),
    )
}

async fn delete_entry(Json(req): Json<FsDeleteRequest>) -> (StatusCode, Json<Value>) {
    let raw = req
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

    let recursive = req.recursive.unwrap_or(false);
    let is_dir = path.is_dir();

    let result = if is_dir {
        if recursive {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_dir(&path)
        }
    } else {
        fs::remove_file(&path)
    };

    if let Err(err) = result {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": err.to_string() })),
        );
    }

    (
        StatusCode::OK,
        Json(json!({
            "path": path.to_string_lossy(),
            "is_dir": is_dir,
            "recursive": recursive,
            "deleted": true
        })),
    )
}

async fn move_entry(Json(req): Json<FsMoveRequest>) -> (StatusCode, Json<Value>) {
    let source_raw = req
        .source_path
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if source_raw.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "源路径不能为空" })),
        );
    }

    let target_parent_raw = req
        .target_parent_path
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if target_parent_raw.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "目标目录不能为空" })),
        );
    }

    let source_path = PathBuf::from(source_raw.unwrap());
    if !source_path.exists() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "源路径不存在" })),
        );
    }

    let target_parent = PathBuf::from(target_parent_raw.unwrap());
    if !target_parent.exists() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "目标目录不存在" })),
        );
    }
    if !target_parent.is_dir() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "目标路径不是目录" })),
        );
    }

    let source_name = source_path
        .file_name()
        .and_then(|v| v.to_str())
        .map(|v| v.to_string())
        .filter(|v| !v.is_empty());
    if source_name.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "源路径名称不合法" })),
        );
    }

    let target_name = req
        .target_name
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| source_name.unwrap());
    if !is_valid_entry_name(&target_name) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "目标名称不合法" })),
        );
    }

    let target_path = target_parent.join(&target_name);
    let source_norm = source_path.to_string_lossy().to_string();
    let target_norm = target_path.to_string_lossy().to_string();
    if source_norm == target_norm {
        return (
            StatusCode::OK,
            Json(json!({
                "from_path": source_norm,
                "to_path": target_norm,
                "replaced": false,
                "moved": false
            })),
        );
    }

    if source_path.is_dir() {
        let source_canonical = match source_path.canonicalize() {
            Ok(path) => path,
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": err.to_string() })),
                )
            }
        };
        let target_parent_canonical = match target_parent.canonicalize() {
            Ok(path) => path,
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": err.to_string() })),
                )
            }
        };
        if target_parent_canonical.starts_with(&source_canonical) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "不支持把目录移动到其子目录中" })),
            );
        }
    }

    let replace_existing = req.replace_existing.unwrap_or(false);
    let mut replaced = false;
    if target_path.exists() {
        if !replace_existing {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "目标目录已存在同名文件或目录" })),
            );
        }
        let remove_result = if target_path.is_dir() {
            fs::remove_dir_all(&target_path)
        } else {
            fs::remove_file(&target_path)
        };
        if let Err(err) = remove_result {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("覆盖目标失败: {}", err) })),
            );
        }
        replaced = true;
    }

    if let Err(err) = fs::rename(&source_path, &target_path) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": err.to_string() })),
        );
    }

    (
        StatusCode::OK,
        Json(json!({
            "from_path": source_norm,
            "to_path": target_norm,
            "name": target_name,
            "replaced": replaced,
            "is_dir": target_path.is_dir(),
            "moved": true
        })),
    )
}

async fn download_entry(Query(query): Query<FsDownloadQuery>) -> Response {
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

async fn list_dirs(Query(query): Query<FsQuery>) -> (StatusCode, Json<Value>) {
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

async fn list_entries(Query(query): Query<FsQuery>) -> (StatusCode, Json<Value>) {
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

async fn search_entries(Query(query): Query<FsSearchQuery>) -> (StatusCode, Json<Value>) {
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

async fn read_file(Query(query): Query<FsReadQuery>) -> (StatusCode, Json<Value>) {
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
    let should_render_text = should_render_text(&path, &bytes, &content_type);

    let (is_binary, content) = if should_render_text {
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

fn read_dir_entries(path: &Path, include_files: bool) -> Result<Vec<Value>, String> {
    let mut out = Vec::new();
    let iter = fs::read_dir(path).map_err(|e| e.to_string())?;
    for entry in iter {
        let entry = match entry {
            Ok(v) => v,
            Err(_) => continue,
        };
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let is_dir = meta.is_dir();
        if !is_dir && !include_files {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let p = entry.path().to_string_lossy().to_string();
        let size = if is_dir { None } else { Some(meta.len()) };
        let modified_at = meta.modified().ok().and_then(format_system_time);
        out.push(json!({
            "name": name,
            "path": p,
            "is_dir": is_dir,
            "size": size,
            "modified_at": modified_at
        }));
    }
    out.sort_by(|a, b| {
        let ad = a.get("is_dir").and_then(|v| v.as_bool()).unwrap_or(false);
        let bd = b.get("is_dir").and_then(|v| v.as_bool()).unwrap_or(false);
        if ad != bd {
            return bd.cmp(&ad);
        }
        let an = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let bn = b.get("name").and_then(|v| v.as_str()).unwrap_or("");
        an.to_lowercase().cmp(&bn.to_lowercase())
    });
    Ok(out)
}

fn is_valid_entry_name(name: &str) -> bool {
    !(name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.contains('\0'))
}

fn infer_download_name(path: &Path) -> String {
    let base = path
        .file_name()
        .and_then(|v| v.to_str())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "download".to_string());
    base
}

fn zip_directory(path: &Path) -> Result<Vec<u8>, String> {
    let root_name = infer_download_name(path);
    let writer = Cursor::new(Vec::<u8>::new());
    let mut zip = zip::ZipWriter::new(writer);
    let dir_options = FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755);
    let file_options = FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    let root_dir = format!("{}/", path_to_zip_name(Path::new(&root_name)));
    zip.add_directory(root_dir.clone(), dir_options)
        .map_err(|err| err.to_string())?;

    for entry in WalkDir::new(path).follow_links(false) {
        let entry = entry.map_err(|err| err.to_string())?;
        let current = entry.path();
        if current == path {
            continue;
        }
        let relative = current.strip_prefix(path).map_err(|err| err.to_string())?;
        let relative_zip_path = path_to_zip_name(relative);
        if relative_zip_path.is_empty() {
            continue;
        }
        let zip_path = format!("{root_name}/{relative_zip_path}");
        if entry.file_type().is_dir() {
            zip.add_directory(format!("{zip_path}/"), dir_options)
                .map_err(|err| err.to_string())?;
            continue;
        }
        if entry.file_type().is_file() {
            zip.start_file(zip_path, file_options)
                .map_err(|err| err.to_string())?;
            let mut file = fs::File::open(current).map_err(|err| err.to_string())?;
            std::io::copy(&mut file, &mut zip).map_err(|err| err.to_string())?;
        }
    }

    let writer = zip.finish().map_err(|err| err.to_string())?;
    Ok(writer.into_inner())
}

fn path_to_zip_name(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

fn format_system_time(time: SystemTime) -> Option<String> {
    let dt: chrono::DateTime<chrono::Utc> = time.into();
    Some(dt.to_rfc3339())
}
