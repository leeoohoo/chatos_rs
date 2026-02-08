use crate::fs_ops::FsOps;
use crate::storage::{ChangeLogStore, ChangeQuery};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct AdminServerOptions {
    pub server_name: String,
    pub root: PathBuf,
    pub db_path: String,
    pub allow_writes: bool,
    pub max_file_bytes: i64,
    pub max_write_bytes: i64,
    pub search_limit: usize,
    pub session_id: String,
    pub run_id: String,
    pub host: String,
    pub port: u16,
    pub admin_ui_root: Option<PathBuf>,
}

pub fn run_admin_server(options: AdminServerOptions) -> Result<(), String> {
    let addr = format!("{}:{}", options.host, options.port);
    let listener = TcpListener::bind(&addr).map_err(|err| err.to_string())?;
    eprintln!("[{}] Admin UI listening on http://{}", options.server_name, addr);
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let opts = options.clone();
                std::thread::spawn(move || {
                    let _ = handle_client(stream, &opts);
                });
            }
            Err(err) => {
                eprintln!("[{}] admin accept error: {}", options.server_name, err);
            }
        }
    }
    Ok(())
}

fn handle_client(mut stream: TcpStream, options: &AdminServerOptions) -> Result<(), String> {
    let mut reader = BufReader::new(&mut stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).map_err(|err| err.to_string())? == 0 {
        return Ok(());
    }
    let request_line = request_line.trim_end_matches(['\r', '\n']);
    if request_line.is_empty() {
        return Ok(());
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    let (path, query) = split_path_query(target);

    let mut headers = HashMap::new();
    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line).map_err(|err| err.to_string())?;
        if bytes == 0 {
            break;
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            headers.insert(key.trim().to_lowercase(), value.trim().to_string());
        }
    }

    if method == "OPTIONS" {
        send_empty(&mut stream, 204)?;
        return Ok(());
    }

    let mut body = Vec::new();
    if let Some(len) = headers.get("content-length").and_then(|v| v.parse::<usize>().ok()) {
        let mut buf = vec![0u8; len];
        reader.read_exact(&mut buf).map_err(|err| err.to_string())?;
        body = buf;
    }

    if path.starts_with("/api/") {
        return handle_api(&mut stream, method, path, query, body, options);
    }

    if method != "GET" && method != "HEAD" {
        send_text(&mut stream, 405, "Method Not Allowed")?;
        return Ok(());
    }

    if let Some(root) = &options.admin_ui_root {
        serve_static(&mut stream, root, path, method == "HEAD")?;
        return Ok(());
    }

    send_text(&mut stream, 404, "Not Found")?;
    Ok(())
}

fn handle_api(
    stream: &mut TcpStream,
    method: &str,
    path: &str,
    query: HashMap<String, String>,
    body: Vec<u8>,
    options: &AdminServerOptions,
) -> Result<(), String> {
    if method == "GET" && path == "/api/status" {
        return send_json(
            stream,
            200,
            json!({
                "ok": true,
                "server_name": options.server_name,
                "root": options.root.to_string_lossy(),
                "db_path": options.db_path,
                "allow_writes": options.allow_writes,
                "max_file_bytes": options.max_file_bytes,
                "max_write_bytes": options.max_write_bytes,
                "search_limit": options.search_limit,
                "session_id": options.session_id,
                "run_id": options.run_id
            }),
        );
    }

    if method == "GET" && path == "/api/changes" {
        let store = ChangeLogStore::new(&options.db_path)?;
        let limit = parse_i64(query.get("limit"), 200).clamp(1, 1000);
        let offset = parse_i64(query.get("offset"), 0).max(0);
        let include_diff = query
            .get("include_diff")
            .map(|value| {
                let value = value.to_lowercase();
                value == "1" || value == "true" || value == "yes"
            })
            .unwrap_or(false);
        let path_prefix = query
            .get("path_prefix")
            .or_else(|| query.get("dir"))
            .cloned()
            .filter(|v| !v.is_empty());
        let path = query.get("path").cloned().filter(|v| !v.is_empty());
        let action = query.get("action").cloned().filter(|v| !v.is_empty());
        let session_id = query.get("session_id").cloned().filter(|v| !v.is_empty());
        let run_id = query.get("run_id").cloned().filter(|v| !v.is_empty());
        let records = store.list_changes(ChangeQuery {
            path,
            path_prefix,
            action,
            session_id,
            run_id,
            limit,
            offset,
        }, include_diff)?;
        return send_json(stream, 200, json!({ "ok": true, "changes": records }));
    }

    if method == "GET" && path == "/api/file" {
        let rel_path = query
            .get("path")
            .ok_or("path is required".to_string())?
            .to_string();
        let fs_ops = FsOps::new(
            options.root.clone(),
            false,
            options.max_file_bytes,
            options.max_write_bytes,
            options.search_limit,
        );
        let (path, size, sha256, content) = fs_ops.read_file_raw(&rel_path)?;
        return send_json(
            stream,
            200,
            json!({
                "ok": true,
                "path": path,
                "size_bytes": size,
                "sha256": sha256,
                "content": content
            }),
        );
    }

    let _payload = if !body.is_empty() {
        serde_json::from_slice::<Value>(&body).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };

    send_text(stream, 404, "Not Found")?;
    Ok(())
}

fn parse_i64(value: Option<&String>, fallback: i64) -> i64 {
    value.and_then(|v| v.parse::<i64>().ok()).unwrap_or(fallback)
}

fn split_path_query(target: &str) -> (&str, HashMap<String, String>) {
    if let Some((path, query)) = target.split_once('?') {
        let params = query
            .split('&')
            .filter_map(|pair| {
                if pair.is_empty() {
                    return None;
                }
                let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
                Some((url_decode(k), url_decode(v)))
            })
            .collect::<HashMap<_, _>>();
        (path, params)
    } else {
        (target, HashMap::new())
    }
}

fn url_decode(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.as_bytes().iter().cloned();
    while let Some(ch) = chars.next() {
        match ch {
            b'+' => out.push(' '),
            b'%' => {
                let a = chars.next();
                let b = chars.next();
                if let (Some(a), Some(b)) = (a, b) {
                    if let Ok(hex) = u8::from_str_radix(&format!("{}{}", a as char, b as char), 16) {
                        out.push(hex as char);
                    }
                }
            }
            _ => out.push(ch as char),
        }
    }
    out
}

fn serve_static(stream: &mut TcpStream, root: &Path, path: &str, head_only: bool) -> Result<(), String> {
    let mut rel = path.trim_start_matches('/');
    if rel.is_empty() {
        rel = "index.html";
    }
    let rel_path = Path::new(rel);
    if rel_path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return send_text(stream, 403, "Forbidden");
    }
    let mut full_path = root.join(rel_path);
    if full_path.is_dir() {
        full_path = full_path.join("index.html");
    }
    if !full_path.exists() {
        let fallback = root.join("index.html");
        if fallback.exists() {
            full_path = fallback;
        } else {
            return send_text(stream, 404, "Not Found");
        }
    }
    let contents = fs::read(&full_path).map_err(|err| err.to_string())?;
    let content_type = content_type_for(&full_path);
    send_bytes(stream, 200, &content_type, &contents, head_only)?;
    Ok(())
}

fn content_type_for(path: &Path) -> String {
    match path.extension().and_then(|v| v.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "application/javascript; charset=utf-8",
        "map" => "application/json; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn send_json(stream: &mut TcpStream, code: u16, value: Value) -> Result<(), String> {
    let body = serde_json::to_string(&value).map_err(|err| err.to_string())?;
    send_response(stream, code, "application/json; charset=utf-8", body.as_bytes(), false)
}

fn send_text(stream: &mut TcpStream, code: u16, text: &str) -> Result<(), String> {
    send_response(stream, code, "text/plain; charset=utf-8", text.as_bytes(), false)
}

fn send_empty(stream: &mut TcpStream, code: u16) -> Result<(), String> {
    send_response(stream, code, "text/plain; charset=utf-8", &[], true)
}

fn send_bytes(
    stream: &mut TcpStream,
    code: u16,
    content_type: &str,
    body: &[u8],
    head_only: bool,
) -> Result<(), String> {
    send_response(stream, code, content_type, body, head_only)
}

fn send_response(
    stream: &mut TcpStream,
    code: u16,
    content_type: &str,
    body: &[u8],
    head_only: bool,
) -> Result<(), String> {
    let status_text = match code {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        _ => "OK",
    };
    let header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET,POST,OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\n\r\n",
        code,
        status_text,
        content_type,
        body.len()
    );
    stream.write_all(header.as_bytes()).map_err(|err| err.to_string())?;
    if !head_only {
        stream.write_all(body).map_err(|err| err.to_string())?;
    }
    stream.flush().map_err(|err| err.to_string())?;
    Ok(())
}
