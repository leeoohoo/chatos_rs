use axum::{Json, Router, routing::get, extract::Query};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct FsQuery {
    path: Option<String>,
}

pub fn router() -> Router {
    Router::new().route("/api/fs/list", get(list_dirs))
}

async fn list_dirs(Query(query): Query<FsQuery>) -> (StatusCode, Json<Value>) {
    let raw = query.path.as_ref().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    if raw.is_none() {
        let roots = list_roots();
        return (StatusCode::OK, Json(json!({
            "path": Value::Null,
            "parent": Value::Null,
            "entries": Vec::<Value>::new(),
            "roots": roots
        })));
    }

    let path = PathBuf::from(raw.unwrap());
    if !path.exists() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "路径不存在" })));
    }
    if !path.is_dir() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "路径不是目录" })));
    }

    let entries = match read_dir_entries(&path) {
        Ok(v) => v,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": err }))),
    };
    let parent = path.parent().map(|p| p.to_string_lossy().to_string());

    (StatusCode::OK, Json(json!({
        "path": path.to_string_lossy(),
        "parent": parent,
        "entries": entries,
        "roots": Vec::<Value>::new()
    })))
}

fn read_dir_entries(path: &Path) -> Result<Vec<Value>, String> {
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
        if !meta.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let p = entry.path().to_string_lossy().to_string();
        out.push(json!({
            "name": name,
            "path": p,
            "is_dir": true
        }));
    }
    out.sort_by(|a, b| {
        let an = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let bn = b.get("name").and_then(|v| v.as_str()).unwrap_or("");
        an.to_lowercase().cmp(&bn.to_lowercase())
    });
    Ok(out)
}

fn list_roots() -> Vec<Value> {
    if cfg!(windows) {
        let mut roots = Vec::new();
        for c in b'A'..=b'Z' {
            let drive = format!("{}:\\", c as char);
            if Path::new(&drive).exists() {
                roots.push(json!({
                    "name": drive.clone(),
                    "path": drive,
                    "is_dir": true
                }));
            }
        }
        return roots;
    }
    let mut roots = Vec::new();
    roots.push(json!({
        "name": "/",
        "path": "/",
        "is_dir": true
    }));
    if let Some(home) = home_dir() {
        roots.push(json!({
            "name": home.clone(),
            "path": home,
            "is_dir": true
        }));
    }
    roots
}

fn home_dir() -> Option<String> {
    if let Ok(value) = std::env::var("HOME") {
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    if let Ok(value) = std::env::var("USERPROFILE") {
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    let drive = std::env::var("HOMEDRIVE").ok();
    let path = std::env::var("HOMEPATH").ok();
    if let (Some(d), Some(p)) = (drive, path) {
        let d = d.trim().to_string();
        let p = p.trim().to_string();
        if !d.is_empty() || !p.is_empty() {
            return Some(format!("{}{}", d, p));
        }
    }
    None
}
