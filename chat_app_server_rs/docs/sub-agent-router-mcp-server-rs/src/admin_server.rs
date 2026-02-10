use crate::catalog::SubAgentCatalog;
use crate::config_store::{ConfigStore, ModelConfig, ModelConfigRecord, RuntimeConfig};
use crate::job_store::JobStore;
use crate::types::McpServerConfig;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Clone)]
pub struct AdminServerOptions {
    pub host: String,
    pub port: u16,
    pub db_path: String,
    pub marketplace_path: String,
    pub plugins_root: String,
    pub registry_path: String,
    pub default_session_id: String,
    pub default_run_id: String,
    pub catalog: Arc<Mutex<SubAgentCatalog>>,
    pub admin_ui_root: Option<PathBuf>,
}

#[derive(Clone, Serialize)]
struct EntrySummary {
    id: String,
    title: String,
    path: String,
    exists: bool,
}

#[derive(Clone, Serialize)]
struct EntryCounts {
    total: usize,
    available: usize,
}

#[derive(Clone, Serialize)]
struct PluginCounts {
    agents: EntryCounts,
    skills: EntryCounts,
    commands: EntryCounts,
}

#[derive(Clone, Serialize)]
struct PluginSummary {
    name: String,
    source: String,
    category: String,
    description: String,
    version: String,
    repository: String,
    homepage: String,
    exists: bool,
    counts: PluginCounts,
    agents: Vec<EntrySummary>,
    skills: Vec<EntrySummary>,
    commands: Vec<EntrySummary>,
}

#[derive(Clone, Serialize)]
struct MarketplaceSummary {
    plugins: Vec<PluginSummary>,
    counts: PluginCounts,
}

#[derive(Clone, Serialize)]
struct InstallOutcome {
    ok: bool,
    installed: bool,
    method: Option<String>,
    error: Option<String>,
}

pub fn start_admin_server(options: AdminServerOptions) {
    let addr = format!("{}:{}", options.host, options.port);
    let listener = match TcpListener::bind(&addr) {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!("[sub_agent_router] admin server bind error: {err}");
            return;
        }
    };
    eprintln!("[sub_agent_router] Admin UI running at http://{addr}");
    if let Some(root) = options.admin_ui_root.as_ref() {
        eprintln!("[sub_agent_router] Serving admin UI from {}", root.display());
    }
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let opts = options.clone();
                thread::spawn(move || handle_connection(stream, opts));
            }
            Err(err) => {
                eprintln!("[sub_agent_router] admin server accept error: {err}");
            }
        }
    }
}

fn handle_connection(mut stream: TcpStream, options: AdminServerOptions) {
    let request = match read_request(&mut stream) {
        Ok(req) => req,
        Err(err) => {
            let _ = write_response(&mut stream, 400, "text/plain", err.as_bytes());
            return;
        }
    };
    let method = request.method.as_str();
    let path = request.path.as_str();

    if method == "GET" && !path.starts_with("/api/") {
        if let Some(root) = options.admin_ui_root.as_ref() {
            if let Some((content_type, body)) = load_static_asset(root, path) {
                let _ = write_response(&mut stream, 200, &content_type, &body);
                return;
            }
            if !looks_like_asset(path) {
                if let Some(body) = load_spa_fallback(root) {
                    let _ = write_response(&mut stream, 200, "text/html; charset=utf-8", &body);
                    return;
                }
            }
        }
        if path == "/" {
            let html = render_page();
            let _ = write_response(&mut stream, 200, "text/html; charset=utf-8", html.as_bytes());
            return;
        }
    }

    if method == "GET" && path == "/api/status" {
        let response = match build_status(&options) {
            Ok(value) => value,
            Err(err) => json!({ "ok": false, "error": err }),
        };
        let _ = write_json(&mut stream, 200, &response);
        return;
    }

    if method == "POST" && path == "/api/settings" {
        let body = parse_json(&request.body);
        let response = match update_settings(&options, &body) {
            Ok(_) => json!({ "ok": true }),
            Err(err) => json!({ "ok": false, "error": err }),
        };
        let _ = write_json(&mut stream, 200, &response);
        return;
    }

    if method == "POST" && path == "/api/model_settings" {
        let body = parse_json(&request.body);
        let response = match update_model_settings(&options, &body) {
            Ok(_) => json!({ "ok": true }),
            Err(err) => json!({ "ok": false, "error": err }),
        };
        let _ = write_json(&mut stream, 200, &response);
        return;
    }

    if method == "POST" && path == "/api/runtime_settings" {
        let body = parse_json(&request.body);
        let response = match update_runtime_settings(&options, &body) {
            Ok(_) => json!({ "ok": true }),
            Err(err) => json!({ "ok": false, "error": err }),
        };
        let _ = write_json(&mut stream, 200, &response);
        return;
    }

    if method == "POST" && path == "/api/allow_prefixes" {
        let body = parse_json(&request.body);
        let response = match update_allow_prefixes(&options, &body) {
            Ok(_) => json!({ "ok": true }),
            Err(err) => json!({ "ok": false, "error": err }),
        };
        let _ = write_json(&mut stream, 200, &response);
        return;
    }

    if method == "GET" && path == "/api/jobs" {
        let response = match list_jobs(&options, &request.query) {
            Ok(value) => value,
            Err(err) => json!({ "ok": false, "error": err }),
        };
        let _ = write_json(&mut stream, 200, &response);
        return;
    }

    if method == "GET" && path == "/api/job_sessions" {
        let response = match list_sessions(&options) {
            Ok(value) => value,
            Err(err) => json!({ "ok": false, "error": err }),
        };
        let _ = write_json(&mut stream, 200, &response);
        return;
    }

    if method == "GET" && path == "/api/job_events" {
        let response = match list_events(&options, &request.query) {
            Ok(value) => value,
            Err(err) => json!({ "ok": false, "error": err }),
        };
        let _ = write_json(&mut stream, 200, &response);
        return;
    }

    if method == "GET" && path == "/api/mcp_servers" {
        let response = match list_mcp_servers(&options) {
            Ok(value) => value,
            Err(err) => json!({ "ok": false, "error": err }),
        };
        let _ = write_json(&mut stream, 200, &response);
        return;
    }

    if method == "POST" && path == "/api/mcp_servers/save" {
        let body = parse_json(&request.body);
        let response = match save_mcp_server(&options, &body) {
            Ok(value) => json!({ "ok": true, "id": value }),
            Err(err) => json!({ "ok": false, "error": err }),
        };
        let _ = write_json(&mut stream, 200, &response);
        return;
    }

    if method == "POST" && path == "/api/mcp_servers/delete" {
        let body = parse_json(&request.body);
        let response = match delete_mcp_server(&options, &body) {
            Ok(_) => json!({ "ok": true }),
            Err(err) => json!({ "ok": false, "error": err }),
        };
        let _ = write_json(&mut stream, 200, &response);
        return;
    }

    if method == "POST" && path == "/api/marketplace" {
        let body = parse_json(&request.body);
        let response = match save_marketplace(&options, &body) {
            Ok(value) => json!({ "ok": true, "id": value }),
            Err(err) => json!({ "ok": false, "error": err }),
        };
        let _ = write_json(&mut stream, 200, &response);
        return;
    }

    if method == "GET" && path == "/api/marketplace/summary" {
        let response = match build_marketplace_summary(&options) {
            Ok(summary) => json!(summary),
            Err(err) => json!({ "ok": false, "error": err }),
        };
        let _ = write_json(&mut stream, 200, &response);
        return;
    }

    if method == "POST" && path == "/api/marketplace/activate" {
        let body = parse_json(&request.body);
        let response = match activate_marketplace(&options, &body) {
            Ok(_) => json!({ "ok": true }),
            Err(err) => json!({ "ok": false, "error": err }),
        };
        let _ = write_json(&mut stream, 200, &response);
        return;
    }

    if method == "POST" && path == "/api/marketplace/delete" {
        let body = parse_json(&request.body);
        let response = match delete_marketplace(&options, &body) {
            Ok(_) => json!({ "ok": true }),
            Err(err) => json!({ "ok": false, "error": err }),
        };
        let _ = write_json(&mut stream, 200, &response);
        return;
    }

    if method == "POST" && path == "/api/plugins/install" {
        let body = parse_json(&request.body);
        let response = match install_single_plugin(&options, &body) {
            Ok(value) => value,
            Err(err) => json!({ "ok": false, "error": err }),
        };
        let _ = write_json(&mut stream, 200, &response);
        return;
    }

    if method == "POST" && path == "/api/plugins/install_missing" {
        let response = match install_missing_plugins(&options) {
            Ok(value) => value,
            Err(err) => json!({ "ok": false, "error": err }),
        };
        let _ = write_json(&mut stream, 200, &response);
        return;
    }

    let _ = write_response(&mut stream, 404, "application/json", br#"{"ok":false,"error":"not found"}"#);
}

struct Request {
    method: String,
    path: String,
    query: HashMap<String, String>,
    body: Vec<u8>,
}

fn read_request(stream: &mut TcpStream) -> Result<Request, String> {
    let mut buffer = Vec::new();
    let mut temp = [0u8; 1024];
    loop {
        let n = stream.read(&mut temp).map_err(|err| err.to_string())?;
        if n == 0 {
            break;
        }
        buffer.extend_from_slice(&temp[..n]);
        if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        if buffer.len() > 1024 * 1024 {
            return Err("request header too large".to_string());
        }
    }
    let header_end = buffer
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("invalid request")?;
    let headers_bytes = &buffer[..header_end + 2];
    let headers_text = String::from_utf8_lossy(headers_bytes);
    let mut lines = headers_text.lines();
    let request_line = lines.next().ok_or("missing request line")?;
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        return Err("invalid request line".to_string());
    }
    let method = parts[0].to_string();
    let full_path = parts[1];
    let (path, query) = split_path_query(full_path);
    let mut content_length = 0usize;
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            if key.trim().eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse::<usize>().unwrap_or(0);
            }
        }
    }

    let mut body = buffer[(header_end + 4)..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut temp).map_err(|err| err.to_string())?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&temp[..n]);
    }
    body.truncate(content_length);
    Ok(Request {
        method,
        path,
        query,
        body,
    })
}

fn split_path_query(path: &str) -> (String, HashMap<String, String>) {
    if let Some((base, query)) = path.split_once('?') {
        (base.to_string(), parse_query(query))
    } else {
        (path.to_string(), HashMap::new())
    }
}

fn parse_query(query: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let mut iter = pair.splitn(2, '=');
        let key = iter.next().unwrap_or("").trim();
        let value = iter.next().unwrap_or("").trim();
        if key.is_empty() {
            continue;
        }
        params.insert(url_decode(key), url_decode(value));
    }
    params
}

fn url_decode(input: &str) -> String {
    let mut out = String::new();
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '+' {
            out.push(' ');
        } else if ch == '%' {
            let hi = chars.next();
            let lo = chars.next();
            if let (Some(h), Some(l)) = (hi, lo) {
                let hex = format!("{h}{l}");
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    out.push(byte as char);
                }
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn write_response(stream: &mut TcpStream, status: u16, content_type: &str, body: &[u8]) -> std::io::Result<()> {
    let status_text = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        _ => "OK",
    };
    let header = format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    Ok(())
}

fn write_json(stream: &mut TcpStream, status: u16, value: &Value) -> std::io::Result<()> {
    let body = serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string());
    write_response(stream, status, "application/json", body.as_bytes())
}

fn parse_json(body: &[u8]) -> Value {
    serde_json::from_slice(body).unwrap_or_else(|_| json!({}))
}

fn load_static_asset(root: &Path, request_path: &str) -> Option<(String, Vec<u8>)> {
    if !root.exists() {
        return None;
    }
    let resolved = resolve_static_path(root, request_path)?;
    let path = if resolved.is_dir() {
        resolved.join("index.html")
    } else {
        resolved
    };
    if !path.exists() || !path.is_file() {
        return None;
    }
    let body = fs::read(&path).ok()?;
    let content_type = content_type_for(&path).to_string();
    Some((content_type, body))
}

fn load_spa_fallback(root: &Path) -> Option<Vec<u8>> {
    let index_path = root.join("index.html");
    fs::read(index_path).ok()
}

fn looks_like_asset(path: &str) -> bool {
    let last = path.rsplit('/').next().unwrap_or("");
    last.contains('.')
}

fn resolve_static_path(root: &Path, request_path: &str) -> Option<PathBuf> {
    let trimmed = request_path.trim_start_matches('/');
    let mut resolved = PathBuf::from(root);
    if trimmed.is_empty() {
        return Some(resolved.join("index.html"));
    }
    for part in trimmed.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return None;
        }
        resolved.push(part);
    }
    Some(resolved)
}

fn content_type_for(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "ico" => "image/x-icon",
        "map" => "application/json; charset=utf-8",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        _ => "application/octet-stream",
    }
}

fn build_status(options: &AdminServerOptions) -> Result<Value, String> {
    let store = ConfigStore::new(&options.db_path, Some(PathBuf::from(&options.marketplace_path)))?;
    let model_config = store.get_model_config();
    let model_configs = store.get_model_configs();
    let active_model_id = store.get_active_model_id();
    let runtime_config = store.get_runtime_config();
    let marketplaces = store.list_marketplaces();
    let active_marketplaces = store
        .get_active_marketplaces()
        .into_iter()
        .map(|entry| entry.id)
        .collect::<Vec<_>>();
    let runtime_payload = json!({
        "aiTimeoutMs": runtime_config.ai_timeout_ms,
        "aiMaxOutputBytes": runtime_config.ai_max_output_bytes,
        "aiToolMaxTurns": runtime_config.ai_tool_max_turns,
        "aiMaxRetries": runtime_config.ai_max_retries,
        "commandTimeoutMs": runtime_config.command_timeout_ms,
        "commandMaxOutputBytes": runtime_config.command_max_output_bytes
    });
    let payload = json!({
        "allow_prefixes": store.get_allow_prefixes(),
        "marketplaces": marketplaces,
        "active_marketplaces": active_marketplaces,
        "marketplace_path": options.marketplace_path,
        "plugins_root": if store.get_plugins_root().is_empty() { options.plugins_root.clone() } else { store.get_plugins_root() },
        "plugins_source_root": store.get_plugins_source_root(),
        "registry_path": if store.get_registry_path().is_empty() { options.registry_path.clone() } else { store.get_registry_path() },
        "db_path": if store.get_db_path().is_empty() { options.db_path.clone() } else { store.get_db_path() },
        "model_config": {
            "api_key": model_config.api_key,
            "base_url": model_config.base_url,
            "model": model_config.model,
            "reasoning_enabled": model_config.reasoning_enabled,
            "responses_enabled": model_config.responses_enabled
        },
        "model_configs": model_configs,
        "active_model_id": active_model_id,
        "runtime_config": runtime_payload
    });
    Ok(payload)
}

fn update_settings(options: &AdminServerOptions, body: &Value) -> Result<(), String> {
    let store = ConfigStore::new(&options.db_path, Some(PathBuf::from(&options.marketplace_path)))?;
    if let Some(root) = body.get("plugins_root").and_then(|v| v.as_str()) {
        let trimmed = root.trim();
        store.set_plugins_root(trimmed)?;
        if let Ok(mut catalog) = options.catalog.lock() {
            if trimmed.is_empty() {
                catalog.set_plugins_root(None);
            } else {
                catalog.set_plugins_root(Some(PathBuf::from(trimmed)));
            }
        }
    }
    if let Some(root) = body.get("plugins_source_root").and_then(|v| v.as_str()) {
        store.set_plugins_source_root(root.trim())?;
    }
    if let Some(prefixes) = body.get("mcp_allow_prefixes").and_then(|v| v.as_str()) {
        let list = prefixes
            .split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>();
        store.set_allow_prefixes(list)?;
    }
    Ok(())
}

fn update_model_settings(options: &AdminServerOptions, body: &Value) -> Result<(), String> {
    let store = ConfigStore::new(&options.db_path, Some(PathBuf::from(&options.marketplace_path)))?;
    if let Some(models) = body.get("models").and_then(|v| v.as_array()) {
        let mut list = Vec::new();
        for entry in models {
            let id = entry.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let api_key = entry
                .get("api_key")
                .or_else(|| entry.get("apiKey"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let base_url = entry
                .get("base_url")
                .or_else(|| entry.get("baseUrl"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let model = entry.get("model").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let reasoning_enabled = entry
                .get("reasoning_enabled")
                .or_else(|| entry.get("reasoningEnabled"))
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let responses_enabled = entry
                .get("responses_enabled")
                .or_else(|| entry.get("responsesEnabled"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            list.push(ModelConfigRecord {
                id,
                name,
                api_key,
                base_url,
                model,
                reasoning_enabled,
                responses_enabled,
            });
        }
        store.set_model_configs(list)?;
        if let Some(active_id) = body
            .get("active_model_id")
            .or_else(|| body.get("activeModelId"))
            .and_then(|v| v.as_str())
        {
            store.set_active_model_id(active_id)?;
        }
        return Ok(());
    }
    let api_key = body
        .get("api_key")
        .or_else(|| body.get("apiKey"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let base_url = body
        .get("base_url")
        .or_else(|| body.get("baseUrl"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let model = body.get("model").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let reasoning_enabled = body
        .get("reasoning_enabled")
        .or_else(|| body.get("reasoningEnabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let responses_enabled = body
        .get("responses_enabled")
        .or_else(|| body.get("responsesEnabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    store.set_model_config(ModelConfig {
        api_key,
        base_url,
        model,
        reasoning_enabled,
        responses_enabled,
    })?;
    Ok(())
}

fn update_runtime_settings(options: &AdminServerOptions, body: &Value) -> Result<(), String> {
    let store = ConfigStore::new(&options.db_path, Some(PathBuf::from(&options.marketplace_path)))?;
    let config = RuntimeConfig {
        ai_timeout_ms: body.get("ai_timeout_ms").and_then(|v| v.as_i64()),
        ai_max_output_bytes: body.get("ai_max_output_bytes").and_then(|v| v.as_i64()),
        ai_tool_max_turns: body.get("ai_tool_max_turns").and_then(|v| v.as_i64()),
        ai_max_retries: body.get("ai_max_retries").and_then(|v| v.as_i64()),
        command_timeout_ms: body.get("command_timeout_ms").and_then(|v| v.as_i64()),
        command_max_output_bytes: body.get("command_max_output_bytes").and_then(|v| v.as_i64()),
    };
    store.set_runtime_config(config)
}

fn update_allow_prefixes(options: &AdminServerOptions, body: &Value) -> Result<(), String> {
    let store = ConfigStore::new(&options.db_path, Some(PathBuf::from(&options.marketplace_path)))?;
    let prefixes = body
        .get("prefixes")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    store.set_allow_prefixes(prefixes)
}

fn list_jobs(options: &AdminServerOptions, query: &HashMap<String, String>) -> Result<Value, String> {
    let job_store = JobStore::new(&options.db_path, options.default_session_id.clone(), options.default_run_id.clone())?;
    let status = query.get("status").map(|s| s.as_str());
    let session_id = query.get("session_id").map(|s| s.as_str());
    let limit = query.get("limit").and_then(|s| s.parse::<i64>().ok());
    let all_sessions = matches!(query.get("all_sessions").map(|v| v.as_str()), Some("1" | "true"));
    let jobs = job_store.list_jobs(session_id, status, limit, all_sessions)?;
    Ok(json!({ "ok": true, "jobs": jobs }))
}

fn list_sessions(options: &AdminServerOptions) -> Result<Value, String> {
    let job_store = JobStore::new(&options.db_path, options.default_session_id.clone(), options.default_run_id.clone())?;
    let sessions = job_store
        .list_sessions(50)?
        .into_iter()
        .map(|(session_id, count, last_created_at)| {
            json!({ "sessionId": session_id, "count": count, "lastCreatedAt": last_created_at })
        })
        .collect::<Vec<_>>();
    Ok(json!({ "ok": true, "sessions": sessions }))
}

fn list_events(options: &AdminServerOptions, query: &HashMap<String, String>) -> Result<Value, String> {
    let job_id = query.get("job_id").ok_or("missing job_id")?;
    let limit = query.get("limit").and_then(|s| s.parse::<i64>().ok()).unwrap_or(200);
    let job_store = JobStore::new(&options.db_path, options.default_session_id.clone(), options.default_run_id.clone())?;
    let events = job_store.list_events(job_id, limit)?;
    Ok(json!({ "ok": true, "events": events }))
}

fn list_mcp_servers(options: &AdminServerOptions) -> Result<Value, String> {
    let store = ConfigStore::new(&options.db_path, Some(PathBuf::from(&options.marketplace_path)))?;
    let servers = store.list_mcp_servers();
    Ok(json!({ "servers": servers }))
}

fn save_mcp_server(options: &AdminServerOptions, body: &Value) -> Result<String, String> {
    let store = ConfigStore::new(&options.db_path, Some(PathBuf::from(&options.marketplace_path)))?;
    let id = body.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if name.trim().is_empty() {
        return Err("missing name".to_string());
    }
    let transport = body.get("transport").and_then(|v| v.as_str()).unwrap_or("stdio").to_string();
    let endpoint_url = body.get("endpoint_url").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let headers_json = body.get("headers_json").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let mut command = body.get("command").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let args = match body.get("args") {
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect::<Vec<_>>(),
        Some(Value::String(text)) => text
            .split_whitespace()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    if transport != "stdio" {
        if endpoint_url.trim().is_empty() {
            return Err("missing endpoint url".to_string());
        }
        command = endpoint_url.clone();
    } else if command.trim().is_empty() {
        return Err("missing command".to_string());
    }
    let enabled = body.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
    let now = chrono::Utc::now().to_rfc3339();
    let record = McpServerConfig {
        id,
        name,
        transport,
        command,
        args,
        endpoint_url,
        headers_json,
        enabled,
        created_at: now.clone(),
        updated_at: now,
    };
    store.save_mcp_server(record)
}

fn delete_mcp_server(options: &AdminServerOptions, body: &Value) -> Result<(), String> {
    let store = ConfigStore::new(&options.db_path, Some(PathBuf::from(&options.marketplace_path)))?;
    let id = body.get("id").and_then(|v| v.as_str()).unwrap_or("");
    if id.trim().is_empty() {
        return Err("missing id".to_string());
    }
    store.delete_mcp_server(id)
}

fn save_marketplace(options: &AdminServerOptions, body: &Value) -> Result<String, String> {
    let json_text = body.get("json").and_then(|v| v.as_str()).unwrap_or("");
    if json_text.trim().is_empty() {
        return Err("missing json".to_string());
    }
    let name = body.get("name").and_then(|v| v.as_str());
    let activate = body.get("activate").and_then(|v| v.as_bool()).unwrap_or(true);
    let store = ConfigStore::new(&options.db_path, Some(PathBuf::from(&options.marketplace_path)))?;
    let id = store.save_marketplace(name, json_text, activate)?;
    store.ensure_marketplace_file()?;
    reload_catalog(options);
    Ok(id)
}

fn activate_marketplace(options: &AdminServerOptions, body: &Value) -> Result<(), String> {
    let id = body.get("id").and_then(|v| v.as_str()).unwrap_or("");
    if id.trim().is_empty() {
        return Err("missing id".to_string());
    }
    let active = body.get("active").and_then(|v| v.as_bool()).unwrap_or(true);
    let store = ConfigStore::new(&options.db_path, Some(PathBuf::from(&options.marketplace_path)))?;
    store.set_marketplace_active(id, active)?;
    reload_catalog(options);
    Ok(())
}

fn delete_marketplace(options: &AdminServerOptions, body: &Value) -> Result<(), String> {
    let id = body.get("id").and_then(|v| v.as_str()).unwrap_or("");
    if id.trim().is_empty() {
        return Err("missing id".to_string());
    }
    let store = ConfigStore::new(&options.db_path, Some(PathBuf::from(&options.marketplace_path)))?;
    store.delete_marketplace(id)?;
    reload_catalog(options);
    Ok(())
}

fn install_single_plugin(options: &AdminServerOptions, body: &Value) -> Result<Value, String> {
    let source = body.get("source").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if source.is_empty() {
        return Err("missing source".to_string());
    }
    let summary = build_marketplace_summary(options)?;
    let plugin = summary
        .plugins
        .iter()
        .find(|entry| entry.source == source)
        .cloned()
        .ok_or_else(|| "plugin not found".to_string())?;
    let outcome = install_plugin(options, &plugin);
    if outcome.ok {
        reload_catalog(options);
    }
    Ok(json!(outcome))
}

fn install_missing_plugins(options: &AdminServerOptions) -> Result<Value, String> {
    let summary = build_marketplace_summary(options)?;
    let missing = summary.plugins.iter().filter(|entry| !entry.exists).cloned().collect::<Vec<_>>();
    let mut results = Vec::new();
    for plugin in missing {
        let outcome = install_plugin(options, &plugin);
        let entry = json!({
            "source": plugin.source,
            "name": plugin.name,
            "ok": outcome.ok,
            "installed": outcome.installed,
            "method": outcome.method,
            "error": outcome.error
        });
        results.push(entry);
    }
    reload_catalog(options);
    Ok(json!({ "ok": true, "count": results.len(), "results": results }))
}

fn build_marketplace_summary(options: &AdminServerOptions) -> Result<MarketplaceSummary, String> {
    let store = ConfigStore::new(&options.db_path, Some(PathBuf::from(&options.marketplace_path)))?;
    let raw = load_marketplace_json(options, &store);
    let plugins = raw
        .get("plugins")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let marketplace_dir = resolve_marketplace_dir(&options.marketplace_path);
    let plugins_root = resolve_plugins_root(&store, &options.plugins_root, &marketplace_dir);
    let mut summary_plugins = Vec::new();
    for plugin in plugins {
        let source = plugin.get("source").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        let plugin_root = resolve_plugin_root(&source, &plugins_root);
        let exists = !source.is_empty() && plugin_root.exists();
        let agents = map_entries(&plugin_root, value_to_list(plugin.get("agents")));
        let skills = map_entries(&plugin_root, value_to_list(plugin.get("skills")));
        let commands = map_entries(&plugin_root, value_to_list(plugin.get("commands")));
        let counts = PluginCounts {
            agents: count_availability(&agents),
            skills: count_availability(&skills),
            commands: count_availability(&commands),
        };
        summary_plugins.push(PluginSummary {
            name: plugin.get("name").and_then(|v| v.as_str()).unwrap_or(&source).to_string(),
            source,
            category: plugin.get("category").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            description: plugin.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            version: plugin.get("version").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            repository: plugin.get("repository").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            homepage: plugin.get("homepage").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            exists,
            counts,
            agents,
            skills,
            commands,
        });
    }
    let counts = PluginCounts {
        agents: aggregate_counts(&summary_plugins, "agents"),
        skills: aggregate_counts(&summary_plugins, "skills"),
        commands: aggregate_counts(&summary_plugins, "commands"),
    };
    Ok(MarketplaceSummary {
        plugins: summary_plugins,
        counts,
    })
}

fn install_plugin(options: &AdminServerOptions, plugin: &PluginSummary) -> InstallOutcome {
    let store = match ConfigStore::new(&options.db_path, Some(PathBuf::from(&options.marketplace_path))) {
        Ok(store) => store,
        Err(err) => {
            return InstallOutcome {
                ok: false,
                installed: false,
                method: None,
                error: Some(err),
            };
        }
    };
    let marketplace_dir = resolve_marketplace_dir(&options.marketplace_path);
    let dest_root = resolve_plugins_root(&store, &options.plugins_root, &marketplace_dir);
    if dest_root.as_os_str().is_empty() {
        return InstallOutcome {
            ok: false,
            installed: false,
            method: None,
            error: Some("plugins_root not set".to_string()),
        };
    }
    let source = plugin.source.trim();
    if source.is_empty() {
        return InstallOutcome {
            ok: false,
            installed: false,
            method: None,
            error: Some("missing source".to_string()),
        };
    }
    let dest_path = resolve_plugin_root(source, &dest_root);
    if dest_path.exists() {
        return InstallOutcome {
            ok: true,
            installed: true,
            method: Some("exists".to_string()),
            error: None,
        };
    }
    let source_root = resolve_plugins_source_root(&store, &marketplace_dir);
    let src_path = resolve_plugin_root(source, &source_root);
    if src_path.exists() {
        if let Err(err) = copy_dir(&src_path, &dest_path) {
            return InstallOutcome {
                ok: false,
                installed: false,
                method: None,
                error: Some(err),
            };
        }
        return InstallOutcome {
            ok: true,
            installed: true,
            method: Some("local".to_string()),
            error: None,
        };
    }
    let repo_url = if !plugin.repository.trim().is_empty() {
        plugin.repository.trim().to_string()
    } else {
        plugin.homepage.trim().to_string()
    };
    if repo_url.is_empty() {
        return InstallOutcome {
            ok: false,
            installed: false,
            method: None,
            error: Some(format!(
                "source plugin not found: {}. Set plugins_source_root or provide repository in marketplace.json.",
                src_path.display()
            )),
        };
    }
    let git_root = resolve_git_cache_root(&store, &marketplace_dir);
    match ensure_git_repo(&repo_url, &git_root) {
        Ok(repo_path) => {
            let repo_plugin_path = resolve_plugin_root(source, &repo_path);
            if !repo_plugin_path.exists() {
                return InstallOutcome {
                    ok: false,
                    installed: false,
                    method: None,
                    error: Some(format!("plugin path not found in repo: {}", repo_plugin_path.display())),
                };
            }
            if let Err(err) = copy_dir(&repo_plugin_path, &dest_path) {
                return InstallOutcome {
                    ok: false,
                    installed: false,
                    method: None,
                    error: Some(err),
                };
            }
            InstallOutcome {
                ok: true,
                installed: true,
                method: Some("git".to_string()),
                error: None,
            }
        }
        Err(err) => InstallOutcome {
            ok: false,
            installed: false,
            method: None,
            error: Some(err),
        },
    }
}

fn load_marketplace_json(options: &AdminServerOptions, store: &ConfigStore) -> Value {
    let active = store.get_active_marketplaces();
    if !active.is_empty() {
        return merge_marketplaces(&active);
    }
    let path = options.marketplace_path.trim();
    if !path.is_empty() {
        let path_buf = PathBuf::from(path);
        if path_buf.exists() {
            if let Ok(text) = fs::read_to_string(&path_buf) {
                if let Ok(parsed) = serde_json::from_str::<Value>(&text) {
                    return parsed;
                }
            }
        }
    }
    json!({})
}

fn merge_marketplaces(entries: &[crate::config_store::MarketplaceEntry]) -> Value {
    let mut plugins: Vec<Value> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut sources = Vec::new();
    for entry in entries {
        let parsed: Value = serde_json::from_str(&entry.json).unwrap_or_else(|_| json!({}));
        let list = parsed
            .get("plugins")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        sources.push(json!({
            "id": entry.id,
            "name": if entry.name.trim().is_empty() { entry.id.clone() } else { entry.name.clone() },
            "plugins": list.len()
        }));
        for plugin in list {
            let key = build_plugin_key(&plugin);
            if seen.insert(key) {
                plugins.push(plugin);
            }
        }
    }
    json!({
        "name": "merged-marketplace",
        "metadata": {
            "merged": true,
            "sources": sources
        },
        "plugins": plugins
    })
}

fn build_plugin_key(plugin: &Value) -> String {
    let source = plugin.get("source").and_then(|v| v.as_str()).unwrap_or("").trim();
    if !source.is_empty() {
        return format!("source:{source}");
    }
    let name = plugin.get("name").and_then(|v| v.as_str()).unwrap_or("").trim();
    if !name.is_empty() {
        return format!("name:{name}");
    }
    plugin.to_string()
}

fn resolve_marketplace_dir(marketplace_path: &str) -> PathBuf {
    let trimmed = marketplace_path.trim();
    if !trimmed.is_empty() {
        if let Some(parent) = Path::new(trimmed).parent() {
            if !parent.as_os_str().is_empty() {
                return parent.to_path_buf();
            }
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn resolve_plugins_root(store: &ConfigStore, fallback: &str, marketplace_dir: &Path) -> PathBuf {
    let stored = store.get_plugins_root();
    if !stored.trim().is_empty() {
        return PathBuf::from(stored);
    }
    if !fallback.trim().is_empty() {
        return PathBuf::from(fallback);
    }
    marketplace_dir.to_path_buf()
}

fn resolve_plugins_source_root(store: &ConfigStore, marketplace_dir: &Path) -> PathBuf {
    let stored = store.get_plugins_source_root();
    if !stored.trim().is_empty() {
        return PathBuf::from(stored);
    }
    marketplace_dir.to_path_buf()
}

fn resolve_plugin_root(source: &str, root: &Path) -> PathBuf {
    if source.trim().is_empty() {
        return root.to_path_buf();
    }
    let candidate = Path::new(source);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(candidate)
    }
}

fn value_to_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn map_entries(plugin_root: &Path, entries: Vec<String>) -> Vec<EntrySummary> {
    let mut list = Vec::new();
    for entry in entries {
        let resolved = resolve_markdown_path(plugin_root, &entry);
        let exists = !resolved.as_os_str().is_empty() && resolved.exists();
        let title = if exists { read_markdown_title(&resolved) } else { String::new() };
        list.push(EntrySummary {
            id: derive_id(&resolved, &entry),
            title,
            path: if resolved.as_os_str().is_empty() {
                entry.clone()
            } else {
                resolved.to_string_lossy().to_string()
            },
            exists,
        });
    }
    list
}

fn resolve_markdown_path(root: &Path, raw_path: &str) -> PathBuf {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return PathBuf::new();
    }
    let candidate = Path::new(trimmed);
    let resolved = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(candidate)
    };
    if resolved.exists() {
        return resolved;
    }
    if resolved.extension().is_none() {
        let with_md = resolved.with_extension("md");
        if with_md.exists() {
            return with_md;
        }
        let with_skill = resolved.join("SKILL.md");
        if with_skill.exists() {
            return with_skill;
        }
        let with_index = resolved.join("index.md");
        if with_index.exists() {
            return with_index;
        }
    }
    resolved
}

fn read_markdown_title(path: &Path) -> String {
    let text = fs::read_to_string(path).unwrap_or_default();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            return trimmed.trim_start_matches('#').trim().to_string();
        }
    }
    String::new()
}

fn derive_id(path: &Path, fallback: &str) -> String {
    if path.as_os_str().is_empty() {
        return slugify(fallback);
    }
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let lower = file_name.to_lowercase();
    let raw = if lower == "skill.md" || lower == "index.md" {
        path.parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string()
    } else {
        path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string()
    };
    slugify(&raw)
}

fn slugify(value: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in value.trim().to_lowercase().chars() {
        let valid = ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-';
        if valid {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn count_availability(items: &[EntrySummary]) -> EntryCounts {
    let total = items.len();
    let available = items.iter().filter(|item| item.exists).count();
    EntryCounts { total, available }
}

fn aggregate_counts(plugins: &[PluginSummary], key: &str) -> EntryCounts {
    let mut total = 0;
    let mut available = 0;
    for plugin in plugins {
        let counts = match key {
            "agents" => &plugin.counts.agents,
            "skills" => &plugin.counts.skills,
            "commands" => &plugin.counts.commands,
            _ => &plugin.counts.agents,
        };
        total += counts.total;
        available += counts.available;
    }
    EntryCounts { total, available }
}

fn copy_dir(src: &Path, dest: &Path) -> Result<(), String> {
    if dest.exists() {
        return Ok(());
    }
    if !src.exists() {
        return Err(format!("source not found: {}", src.display()));
    }
    if src.is_file() {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        fs::copy(src, dest).map_err(|err| err.to_string())?;
        return Ok(());
    }
    fs::create_dir_all(dest).map_err(|err| err.to_string())?;
    for entry in fs::read_dir(src).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        let dest_path = dest.join(entry.file_name());
        let file_type = entry.file_type().map_err(|err| err.to_string())?;
        if file_type.is_dir() {
            copy_dir(&path, &dest_path)?;
        } else if file_type.is_file() {
            fs::copy(&path, &dest_path).map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

fn resolve_git_cache_root(store: &ConfigStore, marketplace_dir: &Path) -> PathBuf {
    let db_path = store.get_db_path();
    if !db_path.trim().is_empty() {
        if let Some(parent) = Path::new(&db_path).parent() {
            return parent.join("git-cache");
        }
    }
    marketplace_dir.join("git-cache")
}

fn ensure_git_repo(repo_url: &str, cache_root: &Path) -> Result<PathBuf, String> {
    let safe_name = sanitize_repo_name(repo_url);
    let repo_path = cache_root.join(safe_name);
    fs::create_dir_all(cache_root).map_err(|err| err.to_string())?;
    let git_dir = repo_path.join(".git");
    if git_dir.exists() {
        if update_git_repo(&repo_path).is_ok() {
            return Ok(repo_path);
        }
        let _ = fs::remove_dir_all(&repo_path);
    } else if repo_path.exists() {
        let _ = fs::remove_dir_all(&repo_path);
    }
    run_git(&["clone", "--depth", "1", repo_url, repo_path.to_string_lossy().as_ref()])?;
    Ok(repo_path)
}

fn update_git_repo(repo_path: &Path) -> Result<(), String> {
    if run_git(&["-C", repo_path.to_string_lossy().as_ref(), "pull", "--ff-only"]).is_ok() {
        return Ok(());
    }
    run_git(&["-C", repo_path.to_string_lossy().as_ref(), "fetch", "--all", "--prune"])?;
    if let Some(origin_head) = resolve_origin_head(repo_path) {
        let ref_name = origin_head.to_string_lossy();
        run_git(&["-C", repo_path.to_string_lossy().as_ref(), "reset", "--hard", ref_name.as_ref()])?;
        return Ok(());
    }
    if run_git(&["-C", repo_path.to_string_lossy().as_ref(), "reset", "--hard", "origin/main"]).is_ok() {
        return Ok(());
    }
    run_git(&["-C", repo_path.to_string_lossy().as_ref(), "reset", "--hard", "origin/master"])
}

fn resolve_origin_head(repo_path: &Path) -> Option<PathBuf> {
    let result = run_git_capture(&[
        "-C",
        repo_path.to_string_lossy().as_ref(),
        "rev-parse",
        "--abbrev-ref",
        "origin/HEAD",
    ]);
    match result {
        Ok(text) => {
            let trimmed = text.trim();
            if !trimmed.is_empty() && trimmed != "origin/HEAD" {
                Some(PathBuf::from(trimmed))
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

fn run_git(args: &[&str]) -> Result<(), String> {
    let output = Command::new("git").args(args).output().map_err(|err| format!("git failed: {err}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(format!("git exited with code {}: {}", output.status.code().unwrap_or(-1), stderr))
}

fn run_git_capture(args: &[&str]) -> Result<String, String> {
    let output = Command::new("git").args(args).output().map_err(|err| format!("git failed: {err}"))?;
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        return Ok(stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(format!("git exited with code {}: {}", output.status.code().unwrap_or(-1), stderr))
}

fn sanitize_repo_name(value: &str) -> String {
    let mut raw = value.trim().to_string();
    if let Some(stripped) = raw.strip_prefix("https://") {
        raw = stripped.to_string();
    } else if let Some(stripped) = raw.strip_prefix("http://") {
        raw = stripped.to_string();
    }
    if let Some(stripped) = raw.strip_prefix("git@") {
        raw = stripped.to_string();
    }
    raw = raw.replace([':', '/'], "-");
    if raw.ends_with(".git") {
        raw.truncate(raw.len().saturating_sub(4));
    }
    let mut cleaned = String::new();
    let mut last_dash = false;
    for ch in raw.chars() {
        let valid = ch.is_ascii_alphanumeric() || ch == '_' || ch == '-';
        if valid {
            cleaned.push(ch);
            last_dash = false;
        } else if !last_dash {
            cleaned.push('-');
            last_dash = true;
        }
    }
    let trimmed = cleaned.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "repo".to_string()
    } else {
        trimmed
    }
}

fn reload_catalog(options: &AdminServerOptions) {
    if let Ok(mut catalog) = options.catalog.lock() {
        catalog.reload();
    }
}

fn render_page() -> String {
    r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <title>Sub-agent Router Admin</title>
    <style>
      body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; padding: 32px; color: #0f172a; }
      .card { max-width: 720px; padding: 20px; border-radius: 12px; background: #f8fafc; border: 1px solid #e2e8f0; }
      code { background: #e2e8f0; padding: 2px 6px; border-radius: 6px; }
    </style>
  </head>
  <body>
    <div class="card">
      <h1>Sub-agent Router Admin</h1>
      <p>This is a minimal admin page for the Rust port.</p>
      <p>API status: <code>/api/status</code></p>
      <p>Endpoints: <code>/api/jobs</code>, <code>/api/job_sessions</code>, <code>/api/job_events</code>, <code>/api/mcp_servers</code></p>
    </div>
  </body>
</html>"#
        .to_string()
}
