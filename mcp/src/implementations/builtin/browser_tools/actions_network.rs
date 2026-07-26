// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{json, Map, Value};
use url::Url;

use super::actions_shared::{
    browser_result_data, build_browser_action_summary, copy_response_fields,
    enrich_response_with_page_metadata, fail_json, is_success, normalize_inline_text,
    run_browser_command,
};
use super::BoundContext;

pub(super) const DEFAULT_BROWSER_NETWORK_LIMIT: usize = 100;
pub(super) const MAX_BROWSER_NETWORK_LIMIT: usize = 200;
pub(super) const DEFAULT_BROWSER_NETWORK_BODY_CHARS: usize = 16 * 1024;
pub(super) const MAX_BROWSER_NETWORK_BODY_CHARS: usize = 64 * 1024;
const MAX_NETWORK_REQUEST_COUNT: u64 = 100_000;
const MAX_NETWORK_URL_CHARS: usize = 4_096;
const MAX_NETWORK_FILTER_CHARS: usize = 256;
const MAX_NETWORK_HEADER_NAME_CHARS: usize = 128;
const MAX_NETWORK_HEADER_VALUE_CHARS: usize = 256;
const MAX_NETWORK_LIST_HEADERS: usize = 16;
const MAX_NETWORK_DETAIL_HEADERS: usize = 64;
pub(super) const REDACTED: &str = "[REDACTED]";

static SENSITIVE_ASSIGNMENT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?i)(authorization|proxy-authorization|cookie|set-cookie|x-api-key|api[_-]?key|access[_-]?token|refresh[_-]?token|password|passwd|secret|client[_-]?secret)(\s*[:=]\s*)(?:\"[^\"]*\"|'[^']*'|[^\s,;&]+)"#,
    )
    .expect("valid sensitive assignment regex")
});
static BEARER_TOKEN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)bearer\s+[A-Za-z0-9._~+/=-]{8,}").expect("valid bearer token regex")
});

#[derive(Debug)]
struct SanitizedHeaders {
    values: Map<String, Value>,
    total_count: usize,
    omitted_count: usize,
    redacted_count: usize,
}

#[derive(Debug)]
struct BodyView {
    available: bool,
    included: bool,
    content_type: String,
    text: Option<String>,
    truncated: bool,
    redaction_applied: bool,
    omitted_reason: Option<String>,
}

pub(super) async fn browser_network_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    clear: bool,
    limit: usize,
    filter: Option<String>,
    resource_types: Vec<String>,
    method: Option<String>,
    status: Option<String>,
) -> Result<Value, String> {
    let filter = normalize_filter(filter)?;
    let resource_types = normalize_resource_types(resource_types)?;
    let method = normalize_method(method)?;
    let status = normalize_status(status)?;
    let session = super::super::context::conversation_key(conversation_id);
    let limit = limit.clamp(1, MAX_BROWSER_NETWORK_LIMIT);
    let args = network_requests_args(
        filter.as_deref(),
        resource_types.as_slice(),
        method.as_deref(),
        status.as_deref(),
    );
    let result = run_browser_command(
        &ctx,
        session.as_str(),
        "network",
        args,
        ctx.command_timeout_seconds,
    )
    .await?;
    if !is_success(&result) {
        return Ok(fail_json(&result, "CDP network request observation failed"));
    }

    let data = browser_result_data(&result);
    let raw_requests = data
        .get("requests")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let reported_count = raw_requests.len().min(MAX_NETWORK_REQUEST_COUNT as usize) as u64;
    let requests = normalize_network_requests(raw_requests.as_slice(), limit);
    let clear_result = if clear {
        Some(
            run_browser_command(
                &ctx,
                session.as_str(),
                "network",
                vec!["requests".to_string(), "--clear".to_string()],
                ctx.command_timeout_seconds,
            )
            .await,
        )
    } else {
        None
    };
    let clear_applied = matches!(clear_result, Some(Ok(ref value)) if is_success(value));
    let clear_error = match clear_result {
        Some(Ok(value)) if !is_success(&value) => value
            .get("error")
            .and_then(Value::as_str)
            .map(|value| normalize_inline_text(value, 180)),
        Some(Err(error)) => Some(normalize_inline_text(error.as_str(), 180)),
        _ => None,
    };

    let mut response = json!({
        "success": true,
        "source": "cdp_network_log",
        "request_count": reported_count,
        "returned_count": requests.len(),
        "omitted_count": reported_count.saturating_sub(requests.len() as u64),
        "truncated": reported_count > requests.len() as u64,
        "clear_requested": clear,
        "clear_applied": clear_applied,
        "clear_error": clear_error,
        "query_values_redacted": true,
        "credentials_redacted": true,
        "header_values_policy": "allowlist_or_redacted",
        "request_bodies_included": false,
        "response_bodies_included": false,
        "filters": {
            "url": filter,
            "resource_types": resource_types,
            "method": method,
            "status": status,
        },
        "requests": requests,
    });
    copy_response_fields(&mut response, &result, &["browser_session"]);
    enrich_response_with_page_metadata(&ctx, session.as_str(), &mut response).await;
    sanitize_response_page_url(&mut response);
    response["_summary_text"] = Value::String(build_browser_network_summary(&response));
    Ok(response)
}

pub(super) async fn browser_network_request_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    request_id: String,
    include_request_body: bool,
    include_response_body: bool,
    max_body_chars: usize,
) -> Result<Value, String> {
    let request_id = normalize_request_id(request_id.as_str())?;
    let max_body_chars = max_body_chars.clamp(1, MAX_BROWSER_NETWORK_BODY_CHARS);
    let session = super::super::context::conversation_key(conversation_id);
    let result = run_browser_command(
        &ctx,
        session.as_str(),
        "network",
        vec!["request".to_string(), request_id.clone()],
        ctx.command_timeout_seconds,
    )
    .await?;
    if !is_success(&result) {
        return Ok(fail_json(&result, "CDP network request detail failed"));
    }

    let data = browser_result_data(&result);
    let request = normalize_network_request_detail(
        &data,
        include_request_body,
        include_response_body,
        max_body_chars,
    )?;
    let mut response = json!({
        "success": true,
        "source": "cdp_network_log",
        "request": request,
        "credentials_redacted": true,
        "query_values_redacted": true,
        "header_values_policy": "allowlist_or_redacted",
        "body_redaction_policy": "sensitive_keys_and_common_credentials",
    });
    copy_response_fields(&mut response, &result, &["browser_session"]);
    enrich_response_with_page_metadata(&ctx, session.as_str(), &mut response).await;
    sanitize_response_page_url(&mut response);
    response["_summary_text"] = Value::String(build_browser_network_request_summary(&response));
    Ok(response)
}

fn network_requests_args(
    filter: Option<&str>,
    resource_types: &[String],
    method: Option<&str>,
    status: Option<&str>,
) -> Vec<String> {
    let mut args = vec!["requests".to_string()];
    if let Some(filter) = filter {
        args.push("--filter".to_string());
        args.push(filter.to_string());
    }
    if !resource_types.is_empty() {
        args.push("--type".to_string());
        args.push(resource_types.join(","));
    }
    if let Some(method) = method {
        args.push("--method".to_string());
        args.push(method.to_string());
    }
    if let Some(status) = status {
        args.push("--status".to_string());
        args.push(status.to_string());
    }
    args
}

fn normalize_network_requests(requests: &[Value], limit: usize) -> Vec<Value> {
    let start = requests.len().saturating_sub(limit);
    requests[start..]
        .iter()
        .filter_map(|request| normalize_network_request(request, MAX_NETWORK_LIST_HEADERS))
        .collect()
}

fn normalize_network_request(request: &Value, header_limit: usize) -> Option<Value> {
    let request_id = normalize_request_id(request.get("requestId")?.as_str()?).ok()?;
    let url = sanitize_network_url(request.get("url")?.as_str()?)?;
    let request_headers = sanitize_headers(request.get("headers"), header_limit);
    let response_headers = sanitize_headers(request.get("responseHeaders"), header_limit);
    Some(json!({
        "request_id": request_id,
        "url": url,
        "method": bounded_token(request.get("method"), "GET", 16),
        "status": bounded_status(request.get("status")),
        "resource_type": bounded_token(request.get("resourceType"), "Other", 48),
        "mime_type": bounded_text(request.get("mimeType"), "", 128),
        "timestamp_ms": bounded_non_negative_number(request.get("timestamp")),
        "request_headers": request_headers.values,
        "request_header_count": request_headers.total_count,
        "request_headers_omitted": request_headers.omitted_count,
        "request_headers_redacted": request_headers.redacted_count,
        "response_headers": response_headers.values,
        "response_header_count": response_headers.total_count,
        "response_headers_omitted": response_headers.omitted_count,
        "response_headers_redacted": response_headers.redacted_count,
        "request_body_available": request.get("postData").and_then(Value::as_str).is_some_and(|value| !value.is_empty()),
        "response_body_may_be_available": request.get("status").and_then(Value::as_u64).is_some(),
    }))
}

fn normalize_network_request_detail(
    data: &Value,
    include_request_body: bool,
    include_response_body: bool,
    max_body_chars: usize,
) -> Result<Value, String> {
    let mut request = normalize_network_request(data, MAX_NETWORK_DETAIL_HEADERS)
        .ok_or_else(|| "network request detail is missing a valid request id or URL".to_string())?;
    let request_content_type = header_value(data.get("headers"), "content-type");
    let response_content_type =
        header_value(data.get("responseHeaders"), "content-type").or_else(|| {
            data.get("mimeType")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        });
    let request_body = body_view(
        data.get("postData"),
        request_content_type.as_deref(),
        include_request_body,
        max_body_chars,
        false,
    );
    let response_body = body_view(
        data.get("responseBody"),
        response_content_type.as_deref(),
        include_response_body,
        max_body_chars,
        data.get("base64Encoded")
            .or_else(|| data.get("responseBodyBase64Encoded"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
    );
    request["request_body"] = body_view_json(request_body);
    request["response_body"] = body_view_json(response_body);
    Ok(request)
}

fn body_view(
    raw: Option<&Value>,
    content_type: Option<&str>,
    include: bool,
    max_chars: usize,
    base64_encoded: bool,
) -> BodyView {
    let content_type = content_type.unwrap_or_default().trim().to_ascii_lowercase();
    let raw = raw
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    if raw.is_none() {
        return BodyView {
            available: false,
            included: false,
            content_type,
            text: None,
            truncated: false,
            redaction_applied: false,
            omitted_reason: None,
        };
    }
    if !include {
        return BodyView {
            available: true,
            included: false,
            content_type,
            text: None,
            truncated: false,
            redaction_applied: false,
            omitted_reason: Some("not_requested".to_string()),
        };
    }
    if base64_encoded || !is_textual_content_type(content_type.as_str()) {
        return BodyView {
            available: true,
            included: false,
            content_type,
            text: None,
            truncated: false,
            redaction_applied: false,
            omitted_reason: Some("non_text_or_base64_body".to_string()),
        };
    }

    let (sanitized, redaction_applied) =
        sanitize_body_text(raw.unwrap_or_default(), content_type.as_str());
    let original_chars = sanitized.chars().count();
    let text = sanitized.chars().take(max_chars).collect::<String>();
    BodyView {
        available: true,
        included: true,
        content_type,
        text: Some(text),
        truncated: original_chars > max_chars,
        redaction_applied,
        omitted_reason: None,
    }
}

fn body_view_json(view: BodyView) -> Value {
    json!({
        "available": view.available,
        "included": view.included,
        "content_type": view.content_type,
        "text": view.text,
        "truncated": view.truncated,
        "redaction_applied": view.redaction_applied,
        "omitted_reason": view.omitted_reason,
    })
}

pub(super) fn sanitize_body_text(raw: &str, content_type: &str) -> (String, bool) {
    let looks_like_json = raw.trim_start().starts_with(['{', '[']);
    if content_type.contains("json") || looks_like_json {
        if let Ok(mut value) = serde_json::from_str::<Value>(raw) {
            let mut redacted = false;
            redact_json_value(&mut value, &mut redacted);
            let serialized =
                serde_json::to_string_pretty(&value).unwrap_or_else(|_| REDACTED.to_string());
            return (serialized, redacted);
        }
    }
    if content_type.contains("application/x-www-form-urlencoded") {
        let mut redacted = false;
        let pairs = url::form_urlencoded::parse(raw.as_bytes())
            .map(|(key, value)| {
                if is_sensitive_name(key.as_ref()) {
                    redacted = true;
                    (key.into_owned(), REDACTED.to_string())
                } else {
                    (key.into_owned(), value.into_owned())
                }
            })
            .collect::<Vec<_>>();
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        serializer.extend_pairs(pairs);
        return (serializer.finish(), redacted);
    }

    let assignment_redacted = SENSITIVE_ASSIGNMENT_RE.is_match(raw);
    let bearer_redacted = BEARER_TOKEN_RE.is_match(raw);
    let text = SENSITIVE_ASSIGNMENT_RE.replace_all(raw, format!("$1$2{REDACTED}"));
    let text = BEARER_TOKEN_RE.replace_all(text.as_ref(), "Bearer [REDACTED]");
    (text.into_owned(), assignment_redacted || bearer_redacted)
}

fn redact_json_value(value: &mut Value, redacted: &mut bool) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if is_sensitive_name(key.as_str()) {
                    *value = Value::String(REDACTED.to_string());
                    *redacted = true;
                } else {
                    redact_json_value(value, redacted);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                redact_json_value(value, redacted);
            }
        }
        _ => {}
    }
}

fn sanitize_headers(value: Option<&Value>, limit: usize) -> SanitizedHeaders {
    let Some(headers) = value.and_then(Value::as_object) else {
        return SanitizedHeaders {
            values: Map::new(),
            total_count: 0,
            omitted_count: 0,
            redacted_count: 0,
        };
    };
    let total_count = headers.len();
    let mut values = Map::new();
    let mut redacted_count = 0;
    for (name, value) in headers.iter().take(limit) {
        let name = normalize_header_name(name.as_str());
        if name.is_empty() {
            continue;
        }
        let raw = value
            .as_str()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| value.to_string());
        let (safe_value, redacted) = sanitize_header_value(name.as_str(), raw.as_str());
        redacted_count += usize::from(redacted);
        values.insert(name, Value::String(safe_value));
    }
    SanitizedHeaders {
        values,
        total_count,
        omitted_count: total_count.saturating_sub(limit),
        redacted_count,
    }
}

pub(super) fn sanitize_header_value(name: &str, value: &str) -> (String, bool) {
    if is_sensitive_name(name) || !is_safe_header_value_name(name) {
        return (REDACTED.to_string(), true);
    }
    if matches!(name, "origin" | "referer" | "access-control-allow-origin") {
        if value == "*" || value.eq_ignore_ascii_case("null") {
            return (value.to_string(), false);
        }
        return sanitize_network_url(value)
            .map(|value| (value, false))
            .unwrap_or_else(|| (REDACTED.to_string(), true));
    }
    (
        value
            .chars()
            .filter(|character| !character.is_control())
            .take(MAX_NETWORK_HEADER_VALUE_CHARS)
            .collect(),
        false,
    )
}

pub(super) fn normalize_header_name(name: &str) -> String {
    name.trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        .take(MAX_NETWORK_HEADER_NAME_CHARS)
        .collect()
}

fn is_safe_header_value_name(name: &str) -> bool {
    matches!(
        name,
        "accept"
            | "accept-language"
            | "content-type"
            | "content-length"
            | "content-encoding"
            | "cache-control"
            | "etag"
            | "last-modified"
            | "origin"
            | "referer"
            | "referrer-policy"
            | "server"
            | "vary"
            | "x-frame-options"
            | "access-control-allow-origin"
            | "sec-fetch-dest"
            | "sec-fetch-mode"
            | "sec-fetch-site"
    )
}

pub(super) fn is_sensitive_name(name: &str) -> bool {
    let normalized = name.trim().to_ascii_lowercase().replace(['_', ' '], "-");
    normalized.contains("authorization")
        || normalized.contains("cookie")
        || normalized.contains("password")
        || normalized.contains("passwd")
        || normalized.contains("secret")
        || normalized.contains("credential")
        || normalized.contains("private-key")
        || normalized.contains("api-key")
        || normalized.contains("apikey")
        || normalized.contains("access-key")
        || normalized.contains("access-token")
        || normalized.contains("refresh-token")
        || normalized.contains("session-token")
        || normalized.ends_with("-token")
        || normalized.ends_with("-password")
        || normalized.ends_with("-secret")
        || normalized == "token"
}

fn header_value(headers: Option<&Value>, name: &str) -> Option<String> {
    headers
        .and_then(Value::as_object)?
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .and_then(|(_, value)| value.as_str())
        .map(ToOwned::to_owned)
}

pub(super) fn is_textual_content_type(content_type: &str) -> bool {
    content_type.is_empty()
        || content_type.starts_with("text/")
        || content_type.contains("json")
        || content_type.contains("xml")
        || content_type.contains("javascript")
        || content_type.contains("x-www-form-urlencoded")
        || content_type.contains("graphql")
}

pub(super) fn sanitize_network_url(value: &str) -> Option<String> {
    let mut parsed = Url::parse(value).ok()?;
    if !matches!(parsed.scheme(), "http" | "https" | "ws" | "wss") {
        return None;
    }
    let _ = parsed.set_username("");
    let _ = parsed.set_password(None);
    let query_names = parsed
        .query_pairs()
        .map(|(key, _)| key.into_owned())
        .take(32)
        .collect::<Vec<_>>();
    parsed.set_query(None);
    if !query_names.is_empty() {
        let mut query = parsed.query_pairs_mut();
        for key in query_names {
            query.append_pair(key.as_str(), REDACTED);
        }
    }
    parsed.set_fragment(None);
    Some(
        parsed
            .to_string()
            .chars()
            .take(MAX_NETWORK_URL_CHARS)
            .collect(),
    )
}

pub(super) fn sanitize_response_page_url(response: &mut Value) {
    let Some(map) = response.as_object_mut() else {
        return;
    };
    let Some(url) = map
        .get("url")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
    else {
        return;
    };
    match sanitize_network_url(url.as_str()) {
        Some(sanitized) => {
            map.insert("url".to_string(), Value::String(sanitized));
        }
        None => {
            map.remove("url");
        }
    }
}

fn normalize_filter(value: Option<String>) -> Result<Option<String>, String> {
    value
        .map(|value| {
            let value = value.trim();
            if value.is_empty() {
                return Ok(None);
            }
            if value.chars().count() > MAX_NETWORK_FILTER_CHARS
                || value.chars().any(char::is_control)
            {
                return Err("network filter is too long or contains control characters".to_string());
            }
            Ok(Some(value.to_string()))
        })
        .unwrap_or(Ok(None))
}

fn normalize_resource_types(values: Vec<String>) -> Result<Vec<String>, String> {
    const ALLOWED: &[&str] = &[
        "document",
        "stylesheet",
        "image",
        "media",
        "font",
        "script",
        "xhr",
        "fetch",
        "websocket",
        "other",
    ];
    if values.len() > ALLOWED.len() {
        return Err("too many network resource types".to_string());
    }
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for value in values {
        let value = value.trim().to_ascii_lowercase();
        if !ALLOWED.contains(&value.as_str()) {
            return Err(format!("unsupported network resource type: {value}"));
        }
        if seen.insert(value.clone()) {
            normalized.push(value);
        }
    }
    Ok(normalized)
}

fn normalize_method(value: Option<String>) -> Result<Option<String>, String> {
    value
        .map(|value| {
            let value = value.trim().to_ascii_uppercase();
            if value.is_empty() {
                return Ok(None);
            }
            if value.len() > 16 || !value.bytes().all(|byte| byte.is_ascii_uppercase()) {
                return Err("network method must contain only ASCII letters".to_string());
            }
            Ok(Some(value))
        })
        .unwrap_or(Ok(None))
}

fn normalize_status(value: Option<String>) -> Result<Option<String>, String> {
    value
        .map(|value| {
            let value = value.trim().to_ascii_lowercase();
            if value.is_empty() {
                return Ok(None);
            }
            let bytes = value.as_bytes();
            let valid = (value.len() == 3 && bytes.iter().all(u8::is_ascii_digit))
                || (value.len() == 3
                    && matches!(bytes[0], b'1'..=b'5')
                    && bytes[1] == b'x'
                    && bytes[2] == b'x')
                || (value.len() == 7
                    && bytes[3] == b'-'
                    && value[..3].bytes().all(|byte| byte.is_ascii_digit())
                    && value[4..].bytes().all(|byte| byte.is_ascii_digit()));
            if !valid {
                return Err(
                    "network status must be a code, class such as 2xx, or range such as 400-499"
                        .to_string(),
                );
            }
            Ok(Some(value))
        })
        .unwrap_or(Ok(None))
}

pub(super) fn normalize_request_id(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
    {
        return Err("network request_id is invalid".to_string());
    }
    Ok(value.to_string())
}

fn bounded_text(value: Option<&Value>, fallback: &str, max_chars: usize) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or(fallback)
        .chars()
        .filter(|character| !character.is_control())
        .take(max_chars)
        .collect()
}

fn bounded_token(value: Option<&Value>, fallback: &str, max_chars: usize) -> String {
    let value = bounded_text(value, fallback, max_chars);
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value
    }
}

fn bounded_status(value: Option<&Value>) -> u64 {
    value.and_then(Value::as_u64).unwrap_or(0).min(999)
}

fn bounded_non_negative_number(value: Option<&Value>) -> f64 {
    value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| value.min(10_000_000_000_000.0))
        .unwrap_or(0.0)
}

fn build_browser_network_summary(response: &Value) -> String {
    let request_count = response
        .get("request_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let returned_count = response
        .get("returned_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let mut action = format!(
        "Observed {returned_count} of {request_count} captured CDP network request(s). Query values and credential-like headers were redacted; bodies were omitted."
    );
    if response
        .get("clear_applied")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        action.push_str(" The captured request log was cleared after reading.");
    }
    build_browser_action_summary(action.as_str(), response, None)
}

fn build_browser_network_request_summary(response: &Value) -> String {
    let request = response.get("request").unwrap_or(&Value::Null);
    let method = request
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("HTTP");
    let url = request
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or("request");
    let request_body = request
        .pointer("/request_body/included")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let response_body = request
        .pointer("/response_body/included")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    build_browser_action_summary(
        format!(
            "Inspected captured CDP request {method} {url}. Credential-like headers and sensitive body fields were redacted. Request body included: {request_body}; response body included: {response_body}."
        )
        .as_str(),
        response,
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_urls_keep_query_shape_but_redact_values_and_credentials() {
        assert_eq!(
            sanitize_network_url(
                "https://user:secret@example.com/api?token=secret&safe=value#part"
            )
            .as_deref(),
            Some("https://example.com/api?token=%5BREDACTED%5D&safe=%5BREDACTED%5D")
        );
        assert!(sanitize_network_url("data:text/plain,secret").is_none());
        assert!(sanitize_network_url("javascript:alert(1)").is_none());
        assert_eq!(
            sanitize_network_url("wss://example.com/socket?access_token=secret").as_deref(),
            Some("wss://example.com/socket?access_token=%5BREDACTED%5D")
        );
    }

    #[test]
    fn network_request_list_redacts_unknown_headers_and_omits_bodies() {
        let payload = json!([{
            "requestId": "7253.2",
            "url": "https://example.com/submit?access_token=query-secret",
            "method": "POST",
            "status": 200,
            "resourceType": "Fetch",
            "mimeType": "application/json",
            "headers": {
                "Content-Type": "application/json",
                "Authorization": "Bearer top-secret",
                "X-Debug": "possibly-sensitive",
            },
            "responseHeaders": {
                "Content-Type": "application/json",
                "Set-Cookie": "session=secret",
            },
            "postData": "{\"password\":\"body-secret\"}",
        }]);
        let requests = normalize_network_requests(payload.as_array().unwrap(), 10);
        let serialized = serde_json::to_string(&requests).unwrap();
        assert_eq!(
            requests[0]["request_headers"]["content-type"],
            "application/json"
        );
        assert_eq!(requests[0]["request_headers"]["authorization"], REDACTED);
        assert_eq!(requests[0]["request_headers"]["x-debug"], REDACTED);
        assert_eq!(requests[0]["response_headers"]["set-cookie"], REDACTED);
        assert!(requests[0]["request_body_available"].as_bool().unwrap());
        assert!(!serialized.contains("top-secret"));
        assert!(!serialized.contains("possibly-sensitive"));
        assert!(!serialized.contains("body-secret"));
        assert!(!serialized.contains("query-secret"));
    }

    #[test]
    fn network_request_detail_redacts_sensitive_json_and_allows_safe_fields() {
        let detail = json!({
            "requestId": "7253.2",
            "url": "https://example.com/submit",
            "method": "POST",
            "status": 200,
            "resourceType": "Fetch",
            "mimeType": "application/json",
            "headers": {"Content-Type": "application/json", "X-Api-Key": "header-secret"},
            "responseHeaders": {"Content-Type": "application/json"},
            "postData": "{\"password\":\"body-secret\",\"safe\":\"visible-body\"}",
            "responseBody": "{\"refresh_token\":\"response-secret\",\"result\":\"visible-result\"}",
        });
        let request = normalize_network_request_detail(&detail, true, true, 4096).unwrap();
        let serialized = serde_json::to_string(&request).unwrap();
        assert!(serialized.contains("visible-body"));
        assert!(serialized.contains("visible-result"));
        assert!(!serialized.contains("body-secret"));
        assert!(!serialized.contains("response-secret"));
        assert!(!serialized.contains("header-secret"));
        assert!(request["request_body"]["redaction_applied"]
            .as_bool()
            .unwrap());
        assert!(request["response_body"]["redaction_applied"]
            .as_bool()
            .unwrap());
    }

    #[test]
    fn network_filters_and_request_ids_are_strictly_bounded() {
        assert_eq!(
            normalize_method(Some("post".to_string()))
                .unwrap()
                .as_deref(),
            Some("POST")
        );
        assert_eq!(
            normalize_status(Some("2xx".to_string()))
                .unwrap()
                .as_deref(),
            Some("2xx")
        );
        assert!(normalize_method(Some("POST --clear".to_string())).is_err());
        assert!(normalize_status(Some("200 OR 500".to_string())).is_err());
        assert!(normalize_status(Some("2x0".to_string())).is_err());
        assert!(normalize_request_id("7253.2").is_ok());
        assert!(normalize_request_id("../../secret").is_err());
    }

    #[test]
    fn network_response_page_url_never_leaks_query_values() {
        let mut response = json!({
            "url": "https://example.com/page?token=page-secret&safe=value"
        });
        sanitize_response_page_url(&mut response);
        let serialized = serde_json::to_string(&response).unwrap();
        assert!(!serialized.contains("page-secret"));
        assert!(!serialized.contains("value"));
        assert_eq!(
            response["url"],
            "https://example.com/page?token=%5BREDACTED%5D&safe=%5BREDACTED%5D"
        );
    }
}
