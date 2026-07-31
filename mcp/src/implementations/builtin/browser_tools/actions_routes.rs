// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::{SecondsFormat, TimeZone, Utc};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use tokio::time::{sleep, Duration};
use uuid::Uuid;

use super::actions_shared::{
    copy_response_fields, fail_json, get_or_create_session, is_success, run_browser_command,
};
use super::BoundContext;
use crate::browser_runtime::run_browser_command as runtime_run_browser_command;

pub(super) const MAX_BROWSER_ROUTES: usize = 32;
pub(super) const MAX_BROWSER_ROUTE_PATTERN_CHARS: usize = 512;
pub(super) const MAX_BROWSER_ROUTE_BODY_BYTES: usize = 16 * 1024;
pub(super) const BROWSER_ROUTE_TTL_SECONDS: u64 = 30 * 60;

static ROUTE_EXPIRY_RUNTIME: Lazy<tokio::runtime::Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .thread_name("chatos-browser-route-expiry")
        .build()
        .expect("build browser route expiry runtime")
});
static CLOSED_MANAGED_BROWSER_SESSIONS: Lazy<Mutex<HashSet<String>>> =
    Lazy::new(|| Mutex::new(HashSet::new()));

#[derive(Debug, Clone)]
pub(crate) struct BrowserRouteRecord {
    route_id: String,
    pattern: String,
    action: &'static str,
    body_bytes: usize,
    body_sha256: Option<String>,
    created_at_unix: i64,
    expires_at_unix: i64,
}

#[derive(Debug, Clone)]
struct BrowserRouteSpec {
    pattern: String,
    action: &'static str,
    body_json: Option<String>,
    body_bytes: usize,
    body_sha256: Option<String>,
}

pub(super) fn browser_route_approval_command(
    arguments: &Value,
) -> Result<(String, Vec<String>), String> {
    let spec = parse_route_spec(arguments)?;
    let mut args = vec![
        "--pattern".to_string(),
        spec.pattern,
        "--action".to_string(),
        spec.action.to_string(),
    ];
    if let Some(body) = spec.body_json {
        args.push("--body".to_string());
        args.push(body);
    }
    Ok(("browser_route_add".to_string(), args))
}

pub(super) async fn browser_route_add_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    arguments: Value,
) -> Result<Value, String> {
    let spec = parse_route_spec(&arguments)?;
    let conversation_key = super::super::context::conversation_key(conversation_id);
    let _mutation_guard = ctx.route_mutation_lock.lock().await;
    prune_expired_routes_locked(&ctx, conversation_key.as_str()).await;

    {
        let routes = ctx.routes.lock();
        let current = routes
            .get(conversation_key.as_str())
            .map(Vec::as_slice)
            .unwrap_or_default();
        if current.len() >= MAX_BROWSER_ROUTES {
            return Err(format!(
                "browser route limit reached for this session ({MAX_BROWSER_ROUTES})"
            ));
        }
        if current.iter().any(|route| route.pattern == spec.pattern) {
            return Err("an interception rule already exists for this exact pattern".to_string());
        }
    }

    let session_name = ensure_managed_session(&ctx, conversation_key.as_str())?;
    CLOSED_MANAGED_BROWSER_SESSIONS
        .lock()
        .remove(session_name.as_str());
    let mut command_args = vec!["route".to_string(), spec.pattern.clone()];
    match spec.action {
        "abort" => command_args.push("--abort".to_string()),
        "mock_json" => {
            command_args.push("--body".to_string());
            command_args.push(spec.body_json.clone().unwrap_or_default());
        }
        _ => return Err("unsupported browser route action".to_string()),
    }
    let result = run_browser_command(
        &ctx,
        conversation_key.as_str(),
        "network",
        command_args,
        ctx.command_timeout_seconds,
    )
    .await?;
    if !is_success(&result) {
        return Ok(fail_json(
            &result,
            "Failed to add browser interception rule",
        ));
    }

    let created_at_unix = Utc::now().timestamp();
    let record = BrowserRouteRecord {
        route_id: format!("r_{}", Uuid::new_v4().simple()),
        pattern: spec.pattern,
        action: spec.action,
        body_bytes: spec.body_bytes,
        body_sha256: spec.body_sha256,
        created_at_unix,
        expires_at_unix: created_at_unix.saturating_add(BROWSER_ROUTE_TTL_SECONDS as i64),
    };
    ctx.routes
        .lock()
        .entry(conversation_key.clone())
        .or_default()
        .push(record.clone());
    spawn_route_expiry(ctx.clone(), conversation_key, record.route_id.clone());

    let mut response = json!({
        "success": true,
        "route": route_json(&record),
        "route_count": route_count(&ctx, conversation_id),
        "limit": MAX_BROWSER_ROUTES,
        "ttl_seconds": BROWSER_ROUTE_TTL_SECONDS,
        "approval_required": true,
        "scope": "managed_browser_session",
        "_summary_text": "Added one explicitly approved, session-scoped browser interception rule. It will expire automatically after 30 minutes."
    });
    copy_response_fields(&mut response, &result, &["browser_session"]);
    Ok(response)
}

pub(super) async fn browser_route_list_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
) -> Result<Value, String> {
    let conversation_key = super::super::context::conversation_key(conversation_id);
    let _mutation_guard = ctx.route_mutation_lock.lock().await;
    prune_expired_routes_locked(&ctx, conversation_key.as_str()).await;
    let mut routes = ctx
        .routes
        .lock()
        .get(conversation_key.as_str())
        .cloned()
        .unwrap_or_default();
    routes.sort_by_key(|route| route.created_at_unix);
    let route_values = routes.iter().map(route_json).collect::<Vec<_>>();
    Ok(json!({
        "success": true,
        "routes": route_values,
        "route_count": routes.len(),
        "limit": MAX_BROWSER_ROUTES,
        "ttl_seconds": BROWSER_ROUTE_TTL_SECONDS,
        "scope": "managed_browser_session",
        "mock_bodies_included": false,
        "_summary_text": format!("Listed {} active browser interception rule(s).", routes.len())
    }))
}

pub(super) async fn browser_route_remove_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    route_id: String,
) -> Result<Value, String> {
    validate_route_id(route_id.as_str())?;
    let conversation_key = super::super::context::conversation_key(conversation_id);
    let _mutation_guard = ctx.route_mutation_lock.lock().await;
    prune_expired_routes_locked(&ctx, conversation_key.as_str()).await;
    let route = ctx
        .routes
        .lock()
        .get(conversation_key.as_str())
        .and_then(|routes| routes.iter().find(|route| route.route_id == route_id))
        .cloned()
        .ok_or_else(|| "browser interception route_id was not found in this session".to_string())?;
    ensure_managed_session(&ctx, conversation_key.as_str())?;
    let result = run_browser_command(
        &ctx,
        conversation_key.as_str(),
        "network",
        vec!["unroute".to_string(), route.pattern.clone()],
        ctx.command_timeout_seconds.min(10),
    )
    .await?;
    if !is_success(&result) {
        return Ok(fail_json(
            &result,
            "Failed to remove browser interception rule",
        ));
    }
    remove_route_record(&ctx, conversation_key.as_str(), route.route_id.as_str());
    let mut response = json!({
        "success": true,
        "removed_route_id": route.route_id,
        "route_count": route_count_for_key(&ctx, conversation_key.as_str()),
        "_summary_text": "Removed the selected browser interception rule."
    });
    copy_response_fields(&mut response, &result, &["browser_session"]);
    Ok(response)
}

pub(super) async fn browser_route_clear_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
) -> Result<Value, String> {
    let conversation_key = super::super::context::conversation_key(conversation_id);
    let _mutation_guard = ctx.route_mutation_lock.lock().await;
    prune_expired_routes_locked(&ctx, conversation_key.as_str()).await;
    let routes = ctx
        .routes
        .lock()
        .get(conversation_key.as_str())
        .cloned()
        .unwrap_or_default();
    if routes.is_empty() {
        return Ok(json!({
            "success": true,
            "cleared_count": 0,
            "route_count": 0,
            "_summary_text": "No browser interception rules were active."
        }));
    }
    ensure_managed_session(&ctx, conversation_key.as_str())?;
    let mut cleared = Vec::new();
    let mut failures = Vec::new();
    let mut last_result = None;
    for route in routes {
        let result = run_browser_command(
            &ctx,
            conversation_key.as_str(),
            "network",
            vec!["unroute".to_string(), route.pattern.clone()],
            ctx.command_timeout_seconds.min(10),
        )
        .await?;
        if is_success(&result) {
            remove_route_record(&ctx, conversation_key.as_str(), route.route_id.as_str());
            cleared.push(route.route_id);
        } else {
            failures.push(route.route_id);
        }
        last_result = Some(result);
    }
    let success = failures.is_empty();
    let mut response = json!({
        "success": success,
        "cleared_route_ids": cleared,
        "cleared_count": cleared.len(),
        "failed_route_ids": failures,
        "route_count": route_count_for_key(&ctx, conversation_key.as_str()),
        "_summary_text": if success {
            "Cleared all ChatOS-owned browser interception rules."
        } else {
            "Some browser interception rules could not be removed and remain tracked."
        }
    });
    if let Some(result) = last_result.as_ref() {
        copy_response_fields(&mut response, result, &["browser_session"]);
    }
    Ok(response)
}

pub(super) fn discard_browser_routes(ctx: &BoundContext, conversation_key: &str) {
    ctx.routes.lock().remove(conversation_key);
}

pub(super) fn mark_browser_session_closed(session_name: &str) {
    if !session_name.trim().is_empty() {
        CLOSED_MANAGED_BROWSER_SESSIONS
            .lock()
            .insert(session_name.to_string());
    }
}

fn parse_route_spec(arguments: &Value) -> Result<BrowserRouteSpec, String> {
    let pattern = arguments
        .get("pattern")
        .and_then(Value::as_str)
        .ok_or_else(|| "pattern is required".to_string())?;
    let pattern = normalize_route_pattern(pattern)?;
    let action = arguments
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| "action is required".to_string())?;
    let body = arguments.get("body");
    match action {
        "abort" => {
            if body.is_some() {
                return Err("body is not allowed when action is abort".to_string());
            }
            Ok(BrowserRouteSpec {
                pattern,
                action: "abort",
                body_json: None,
                body_bytes: 0,
                body_sha256: None,
            })
        }
        "mock_json" => {
            let body =
                body.ok_or_else(|| "body is required when action is mock_json".to_string())?;
            let body_json = serde_json::to_string(body)
                .map_err(|_| "mock JSON body could not be serialized".to_string())?;
            let body_bytes = body_json.len();
            if body_bytes > MAX_BROWSER_ROUTE_BODY_BYTES {
                return Err(format!(
                    "mock JSON body exceeds {MAX_BROWSER_ROUTE_BODY_BYTES} serialized bytes"
                ));
            }
            let body_sha256 = hex::encode(Sha256::digest(body_json.as_bytes()));
            Ok(BrowserRouteSpec {
                pattern,
                action: "mock_json",
                body_json: Some(body_json),
                body_bytes,
                body_sha256: Some(body_sha256),
            })
        }
        _ => Err("action must be abort or mock_json".to_string()),
    }
}

fn normalize_route_pattern(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > MAX_BROWSER_ROUTE_PATTERN_CHARS {
        return Err(format!(
            "pattern must contain 1-{MAX_BROWSER_ROUTE_PATTERN_CHARS} characters"
        ));
    }
    if !value.is_ascii()
        || value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
        || value.contains(['?', '#', '\\', '@', '[', ']', '{', '}'])
    {
        return Err(
            "pattern must be an ASCII HTTP(S) URL glob without credentials, query, fragment, whitespace, backslashes, or advanced glob syntax"
                .to_string(),
        );
    }
    let remainder = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
        .ok_or_else(|| "pattern must start with http:// or https://".to_string())?;
    let (authority, path) = remainder
        .split_once('/')
        .ok_or_else(|| "pattern must include an explicit / path".to_string())?;
    validate_route_authority(authority)?;
    validate_route_path(path)?;
    Ok(value.to_string())
}

fn validate_route_authority(authority: &str) -> Result<(), String> {
    if authority.is_empty() || authority.len() > 253 {
        return Err("pattern host is invalid".to_string());
    }
    let (hostname, port) = match authority.rsplit_once(':') {
        Some((hostname, port)) if !hostname.contains(':') => (hostname, Some(port)),
        Some(_) => return Err("IPv6 and ambiguous route authorities are not supported".to_string()),
        None => (authority, None),
    };
    if let Some(port) = port {
        let port = port
            .parse::<u16>()
            .ok()
            .filter(|port| *port > 0)
            .ok_or_else(|| "pattern port must be an integer from 1 to 65535".to_string())?;
        let _ = port;
    }
    let labels = hostname.split('.').collect::<Vec<_>>();
    if labels.is_empty() || labels.iter().all(|label| *label == "*") {
        return Err("pattern host must contain at least one literal label".to_string());
    }
    for label in labels {
        if label == "*" {
            continue;
        }
        if label.is_empty()
            || label.len() > 63
            || label.starts_with('-')
            || label.ends_with('-')
            || !label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err("pattern host contains an invalid label".to_string());
        }
    }
    Ok(())
}

fn validate_route_path(path: &str) -> Result<(), String> {
    if path.len() > MAX_BROWSER_ROUTE_PATTERN_CHARS
        || !path.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'/' | b'-'
                        | b'.'
                        | b'_'
                        | b'~'
                        | b'!'
                        | b'$'
                        | b'&'
                        | b'\''
                        | b'('
                        | b')'
                        | b'*'
                        | b'+'
                        | b','
                        | b';'
                        | b'='
                        | b':'
                        | b'%'
                )
        })
    {
        return Err("pattern path contains unsupported characters".to_string());
    }
    Ok(())
}

fn validate_route_id(value: &str) -> Result<(), String> {
    if value.len() == 34
        && value.starts_with("r_")
        && value[2..].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        Ok(())
    } else {
        Err("route_id is invalid".to_string())
    }
}

fn ensure_managed_session(ctx: &BoundContext, conversation_key: &str) -> Result<String, String> {
    let (session, _) = get_or_create_session(ctx, conversation_key);
    if session.cdp_url.is_some() {
        return Err(
            "browser route interception is limited to managed Local Connector sessions".to_string(),
        );
    }
    Ok(session.session_name)
}

fn route_json(route: &BrowserRouteRecord) -> Value {
    json!({
        "route_id": route.route_id,
        "pattern": route.pattern,
        "action": route.action,
        "body_bytes": route.body_bytes,
        "body_sha256": route.body_sha256,
        "body_included": false,
        "created_at": timestamp_text(route.created_at_unix),
        "expires_at": timestamp_text(route.expires_at_unix),
        "ttl_seconds": BROWSER_ROUTE_TTL_SECONDS,
    })
}

fn timestamp_text(timestamp: i64) -> String {
    Utc.timestamp_opt(timestamp, 0)
        .single()
        .unwrap_or_else(Utc::now)
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn route_count(ctx: &BoundContext, conversation_id: Option<&str>) -> usize {
    let conversation_key = super::super::context::conversation_key(conversation_id);
    route_count_for_key(ctx, conversation_key.as_str())
}

fn route_count_for_key(ctx: &BoundContext, conversation_key: &str) -> usize {
    ctx.routes
        .lock()
        .get(conversation_key)
        .map(Vec::len)
        .unwrap_or(0)
}

fn remove_route_record(ctx: &BoundContext, conversation_key: &str, route_id: &str) {
    let mut routes = ctx.routes.lock();
    if let Some(entries) = routes.get_mut(conversation_key) {
        entries.retain(|route| route.route_id != route_id);
        if entries.is_empty() {
            routes.remove(conversation_key);
        }
    }
}

async fn prune_expired_routes_locked(ctx: &BoundContext, conversation_key: &str) {
    let closed = ctx
        .sessions
        .lock()
        .get(conversation_key)
        .map(|session| {
            CLOSED_MANAGED_BROWSER_SESSIONS
                .lock()
                .contains(session.session_name.as_str())
        })
        .unwrap_or(false);
    if closed {
        ctx.routes.lock().remove(conversation_key);
        ctx.sessions.lock().remove(conversation_key);
        return;
    }
    let now = Utc::now().timestamp();
    let expired = ctx
        .routes
        .lock()
        .get(conversation_key)
        .map(|routes| {
            routes
                .iter()
                .filter(|route| route.expires_at_unix <= now)
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for route in expired {
        expire_route_locked(ctx, conversation_key, route).await;
    }
}

fn spawn_route_expiry(ctx: BoundContext, conversation_key: String, route_id: String) {
    ROUTE_EXPIRY_RUNTIME.spawn(async move {
        sleep(Duration::from_secs(BROWSER_ROUTE_TTL_SECONDS)).await;
        let _mutation_guard = ctx.route_mutation_lock.lock().await;
        let route = ctx
            .routes
            .lock()
            .get(conversation_key.as_str())
            .and_then(|routes| routes.iter().find(|route| route.route_id == route_id))
            .cloned();
        if let Some(route) = route {
            expire_route_locked(&ctx, conversation_key.as_str(), route).await;
        }
    });
}

async fn expire_route_locked(
    ctx: &BoundContext,
    conversation_key: &str,
    route: BrowserRouteRecord,
) {
    let session = ctx.sessions.lock().get(conversation_key).cloned();
    let Some(session) = session else {
        remove_route_record(ctx, conversation_key, route.route_id.as_str());
        return;
    };
    if CLOSED_MANAGED_BROWSER_SESSIONS
        .lock()
        .contains(session.session_name.as_str())
    {
        ctx.routes.lock().remove(conversation_key);
        ctx.sessions.lock().remove(conversation_key);
        return;
    }
    let result = runtime_run_browser_command(
        ctx.workspace_dir.as_path(),
        &session,
        "network",
        vec!["unroute".to_string(), route.pattern.clone()],
        ctx.command_timeout_seconds.min(10),
    )
    .await;
    if result.as_ref().is_ok_and(is_success) {
        remove_route_record(ctx, conversation_key, route.route_id.as_str());
        return;
    }

    // Fail closed: if an expired interception cannot be removed reliably, close
    // the managed browser session so the rule cannot outlive its approved TTL.
    let _ = runtime_run_browser_command(
        ctx.workspace_dir.as_path(),
        &session,
        "close",
        Vec::new(),
        ctx.command_timeout_seconds.min(10),
    )
    .await;
    ctx.routes.lock().remove(conversation_key);
    ctx.sessions.lock().remove(conversation_key);
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{browser_route_approval_command, normalize_route_pattern, parse_route_spec};

    #[test]
    fn route_patterns_require_bounded_http_urls() {
        assert_eq!(
            normalize_route_pattern("https://*.example.com/api/**").unwrap(),
            "https://*.example.com/api/**"
        );
        assert!(normalize_route_pattern("https://*/**").is_err());
        assert!(normalize_route_pattern("file:///tmp/secret").is_err());
        assert!(normalize_route_pattern("https://user@example.com/**").is_err());
        assert!(normalize_route_pattern("https://example.com/**?token=secret").is_err());
        assert!(normalize_route_pattern("https://example.com/{a,b}").is_err());
    }

    #[test]
    fn route_specs_are_exact_and_bounded_for_approval() {
        let args = json!({
            "pattern": "https://example.com/api/**",
            "action": "mock_json",
            "body": {"ok": true}
        });
        let spec = parse_route_spec(&args).unwrap();
        assert_eq!(spec.body_json.as_deref(), Some("{\"ok\":true}"));
        let (command, approval_args) = browser_route_approval_command(&args).unwrap();
        assert_eq!(command, "browser_route_add");
        assert_eq!(
            approval_args.last().map(String::as_str),
            Some("{\"ok\":true}")
        );
        assert!(parse_route_spec(&json!({
            "pattern": "https://example.com/**",
            "action": "abort",
            "body": null
        }))
        .is_err());
    }
}
