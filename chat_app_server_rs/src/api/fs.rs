use axum::http::StatusCode;
use axum::{extract::Query, routing::get, Json, Router};
use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

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

pub fn router() -> Router {
    Router::new()
        .route("/api/fs/list", get(list_dirs))
        .route("/api/fs/entries", get(list_entries))
        .route("/api/fs/search", get(search_entries))
        .route("/api/fs/read", get(read_file))
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
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();
    let is_text_ext = matches!(
        ext.as_str(),
        "rs" | "toml"
            | "lock"
            | "md"
            | "txt"
            | "json"
            | "yaml"
            | "yml"
            | "xml"
            | "html"
            | "htm"
            | "css"
            | "scss"
            | "less"
            | "js"
            | "jsx"
            | "ts"
            | "tsx"
            | "mjs"
            | "cjs"
            | "py"
            | "go"
            | "java"
            | "kt"
            | "swift"
            | "c"
            | "cc"
            | "cpp"
            | "h"
            | "hpp"
            | "cs"
            | "php"
            | "rb"
            | "sh"
            | "bash"
            | "zsh"
            | "ps1"
            | "bat"
            | "ini"
            | "conf"
            | "env"
            | "log"
            | "sql"
            | "vue"
            | "svelte"
            | "astro"
            | "dart"
            | "lua"
            | "r"
            | "m"
            | "mm"
            | "scala"
            | "gradle"
            | "make"
            | "cmake"
            | "dockerfile"
            | "properties"
            | "cfg"
            | "rc"
            | "proto"
            | "graphql"
    );
    let is_text_name = matches!(
        file_name.as_str(),
        "dockerfile"
            | "makefile"
            | "cmakelists.txt"
            | ".gitignore"
            | ".gitattributes"
            | ".editorconfig"
            | ".npmrc"
            | ".yarnrc"
            | ".yarnrc.yml"
            | ".prettierrc"
            | ".eslintrc"
            | ".babelrc"
            | ".env"
            | ".env.local"
            | ".env.development"
            | ".env.production"
    );
    let utf8_ok = std::str::from_utf8(&bytes).is_ok();
    let is_text_mime = content_type.starts_with("text/")
        || content_type == "application/json"
        || content_type == "application/xml"
        || content_type == "application/javascript"
        || content_type == "application/typescript";
    let should_render_text = utf8_ok && (is_text_mime || is_text_ext || is_text_name);

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

fn normalize_search_keyword(value: &str) -> String {
    value.trim().to_lowercase()
}

fn compact_search_text(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !matches!(ch, ' ' | '\t' | '\n' | '\r' | '_' | '-' | '.' | '/' | '\\'))
        .collect()
}

fn fuzzy_match(text: &str, keyword: &str) -> bool {
    if keyword.is_empty() {
        return true;
    }
    if text.contains(keyword) {
        return true;
    }

    let mut keyword_iter = keyword.chars();
    let mut current = match keyword_iter.next() {
        Some(ch) => ch,
        None => return true,
    };

    for ch in text.chars() {
        if ch == current {
            match keyword_iter.next() {
                Some(next) => current = next,
                None => return true,
            }
        }
    }

    false
}

fn is_search_match(name: &str, relative_path: &str, keyword: &str) -> bool {
    if keyword.is_empty() {
        return true;
    }

    let lower_name = name.to_lowercase();
    let lower_path = relative_path.to_lowercase();

    if fuzzy_match(&lower_name, keyword) || fuzzy_match(&lower_path, keyword) {
        return true;
    }

    let compact_keyword = compact_search_text(keyword);
    if compact_keyword.is_empty() {
        return false;
    }

    let compact_name = compact_search_text(&lower_name);
    let compact_path = compact_search_text(&lower_path);
    fuzzy_match(&compact_name, &compact_keyword) || fuzzy_match(&compact_path, &compact_keyword)
}

fn format_system_time(time: SystemTime) -> Option<String> {
    let dt: chrono::DateTime<chrono::Utc> = time.into();
    Some(dt.to_rfc3339())
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
