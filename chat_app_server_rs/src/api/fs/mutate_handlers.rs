use crate::services::code_nav::symbol_index::invalidate_project_symbol_indexes_for_path;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use super::contracts::{FsCreateFileRequest, FsDeleteRequest, FsMkdirRequest, FsMoveRequest};
use super::helpers::is_valid_entry_name;

pub(super) async fn create_dir(Json(req): Json<FsMkdirRequest>) -> (StatusCode, Json<Value>) {
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
    if !is_valid_entry_name(&name) {
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
    invalidate_project_symbol_indexes_for_path(target.as_path());

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

pub(super) async fn create_file(Json(req): Json<FsCreateFileRequest>) -> (StatusCode, Json<Value>) {
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
    invalidate_project_symbol_indexes_for_path(target.as_path());
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

pub(super) async fn delete_entry(Json(req): Json<FsDeleteRequest>) -> (StatusCode, Json<Value>) {
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
    invalidate_project_symbol_indexes_for_path(path.as_path());

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

pub(super) async fn move_entry(Json(req): Json<FsMoveRequest>) -> (StatusCode, Json<Value>) {
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
        invalidate_project_symbol_indexes_for_path(target_path.as_path());
        replaced = true;
    }

    if let Err(err) = fs::rename(&source_path, &target_path) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": err.to_string() })),
        );
    }
    invalidate_project_symbol_indexes_for_path(source_path.as_path());
    invalidate_project_symbol_indexes_for_path(target_path.as_path());

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
