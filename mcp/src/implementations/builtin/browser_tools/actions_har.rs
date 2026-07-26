// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};
use uuid::Uuid;

use super::actions_files::{normalize_workspace_relative_path, remove_regular_file_or_symlink};
use super::actions_network::{
    is_sensitive_name, is_textual_content_type, normalize_header_name, sanitize_body_text,
    sanitize_header_value, sanitize_network_url, sanitize_response_page_url, REDACTED,
};
use super::actions_shared::{
    copy_response_fields, enrich_response_with_page_metadata, fail_json, is_success,
    normalize_inline_text, run_browser_command,
};
use super::BoundContext;

pub(super) const DEFAULT_BROWSER_HAR_MAX_ENTRIES: usize = 500;
pub(super) const MAX_BROWSER_HAR_ENTRIES: usize = 1_000;
const MAX_BROWSER_HAR_RAW_BYTES: u64 = 64 * 1024 * 1024;
const MAX_BROWSER_HAR_OUTPUT_BYTES: usize = 64 * 1024 * 1024;
const MAX_HAR_HEADERS: usize = 128;
const MAX_HAR_COOKIES: usize = 128;
const MAX_HAR_QUERY_ITEMS: usize = 256;
const MAX_HAR_PAGES: usize = 1_000;
const MAX_HAR_TEXT_CHARS: usize = 4_096;

#[derive(Debug)]
struct HarDestination {
    target: PathBuf,
    relative: String,
}

#[derive(Debug)]
struct RawHarCapture {
    directory: PathBuf,
    path: PathBuf,
    cleaned: bool,
}

impl RawHarCapture {
    fn new() -> Result<Self, String> {
        let directory =
            std::env::temp_dir().join(format!("chatos-browser-har-{}", Uuid::new_v4().simple()));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(directory.as_path())
            .map_err(|error| format!("create private HAR capture directory failed: {error}"))?;
        Ok(Self {
            path: directory.join("capture.har"),
            directory,
            cleaned: false,
        })
    }

    fn cleanup(&mut self) -> Result<(), String> {
        remove_regular_file_or_symlink(self.path.as_path());
        fs::remove_dir(self.directory.as_path())
            .map_err(|error| format!("remove private HAR capture directory failed: {error}"))?;
        self.cleaned = true;
        Ok(())
    }
}

impl Drop for RawHarCapture {
    fn drop(&mut self) {
        if self.cleaned {
            return;
        }
        remove_regular_file_or_symlink(self.path.as_path());
        let _ = fs::remove_dir(self.directory.as_path());
    }
}

#[derive(Debug, Default)]
struct HarSanitizationStats {
    original_entries: usize,
    exported_entries: usize,
    omitted_entries: usize,
    redacted_header_values: usize,
    redacted_query_values: usize,
    redacted_cookie_values: usize,
    available_request_bodies: usize,
    included_request_bodies: usize,
    available_response_bodies: usize,
    included_response_bodies: usize,
    truncated_bodies: usize,
    redacted_bodies: usize,
}

impl HarSanitizationStats {
    fn to_json(&self) -> Value {
        json!({
            "original_entries": self.original_entries,
            "exported_entries": self.exported_entries,
            "omitted_entries": self.omitted_entries,
            "redacted_header_values": self.redacted_header_values,
            "redacted_query_values": self.redacted_query_values,
            "redacted_cookie_values": self.redacted_cookie_values,
            "available_request_bodies": self.available_request_bodies,
            "included_request_bodies": self.included_request_bodies,
            "available_response_bodies": self.available_response_bodies,
            "included_response_bodies": self.included_response_bodies,
            "truncated_bodies": self.truncated_bodies,
            "redacted_bodies": self.redacted_bodies,
        })
    }
}

struct SanitizedHar {
    value: Value,
    stats: HarSanitizationStats,
}

pub(super) async fn browser_har_start_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
) -> Result<Value, String> {
    let session = super::super::context::conversation_key(conversation_id);
    let result = run_browser_command(
        &ctx,
        session.as_str(),
        "network",
        vec!["har".to_string(), "start".to_string()],
        ctx.command_timeout_seconds,
    )
    .await?;
    if !is_success(&result) {
        return Ok(fail_json(&result, "HAR capture start failed"));
    }

    let mut response = json!({
        "success": true,
        "capture": "har",
        "status": "recording",
        "raw_capture_private": true,
        "query_values_redacted_on_export": true,
        "credentials_redacted_on_export": true,
        "bodies_included_by_default": false,
    });
    copy_response_fields(&mut response, &result, &["browser_session"]);
    enrich_response_with_page_metadata(&ctx, session.as_str(), &mut response).await;
    sanitize_response_page_url(&mut response);
    response["_summary_text"] = Value::String(
        "Started HAR capture for the current browser session. Stop it with browser_har_stop to publish a sanitized workspace file."
            .to_string(),
    );
    Ok(response)
}

pub(super) async fn browser_har_stop_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    path: String,
    include_request_bodies: bool,
    include_response_bodies: bool,
    max_body_chars: usize,
    max_entries: usize,
) -> Result<Value, String> {
    let destination = prepare_har_destination(ctx.workspace_dir.as_path(), path.as_str())?;
    let max_body_chars =
        max_body_chars.clamp(1, super::actions_network::MAX_BROWSER_NETWORK_BODY_CHARS);
    let max_entries = max_entries.clamp(1, MAX_BROWSER_HAR_ENTRIES);
    let session = super::super::context::conversation_key(conversation_id);
    let mut capture = RawHarCapture::new()?;
    let result = run_browser_command(
        &ctx,
        session.as_str(),
        "network",
        vec![
            "har".to_string(),
            "stop".to_string(),
            capture.path.to_string_lossy().to_string(),
        ],
        ctx.command_timeout_seconds.max(120),
    )
    .await?;
    if !is_success(&result) {
        let mut response = json!({
            "success": false,
            "error": "HAR capture stop failed",
            "_summary_text": "HAR capture stop failed before a sanitized workspace file was published.",
        });
        copy_response_fields(&mut response, &result, &["browser_session"]);
        return Ok(response);
    }

    let raw = read_raw_har(capture.path.as_path())?;
    capture.cleanup()?;
    let sanitized = sanitize_har(
        raw,
        include_request_bodies,
        include_response_bodies,
        max_body_chars,
        max_entries,
    )?;
    let stats = sanitized.stats.to_json();
    let bytes = match publish_sanitized_har(&destination, sanitized.value) {
        Ok(bytes) => bytes,
        Err(error) => {
            remove_regular_file_or_symlink(destination.target.as_path());
            let mut response = json!({
                "success": false,
                "error": error,
                "path": destination.relative,
                "raw_capture_deleted": true,
            });
            copy_response_fields(&mut response, &result, &["browser_session"]);
            response["_summary_text"] = Value::String(format!(
                "HAR export failed before publication: {}.",
                response
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error")
            ));
            return Ok(response);
        }
    };

    let mut response = json!({
        "success": true,
        "capture": "har",
        "status": "stopped",
        "path": destination.relative,
        "bytes": bytes,
        "overwrote_existing": false,
        "raw_capture_deleted": true,
        "request_bodies_included": include_request_bodies,
        "response_bodies_included": include_response_bodies,
        "max_body_chars": max_body_chars,
        "max_entries": max_entries,
        "query_values_redacted": true,
        "credentials_redacted": true,
        "cookie_values_redacted": true,
        "header_values_policy": "allowlist_or_redacted",
        "sanitization": stats,
    });
    copy_response_fields(&mut response, &result, &["browser_session"]);
    enrich_response_with_page_metadata(&ctx, session.as_str(), &mut response).await;
    sanitize_response_page_url(&mut response);
    response["_summary_text"] = Value::String(format!(
        "Stopped HAR capture and wrote {} sanitized request(s) to {}. Raw capture data was deleted; query, cookie, credential, and unknown header values were redacted.",
        response
            .pointer("/sanitization/exported_entries")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        destination.relative,
    ));
    Ok(response)
}

fn prepare_har_destination(
    workspace_root: &Path,
    requested: &str,
) -> Result<HarDestination, String> {
    let (relative_path, relative) = normalize_workspace_relative_path(requested)?;
    let file_name = relative_path
        .file_name()
        .ok_or_else(|| "HAR path must include a file name".to_string())?;
    let file_name_text = file_name.to_string_lossy();
    if !file_name_text.to_ascii_lowercase().ends_with(".har") {
        return Err("HAR export path must end with .har".to_string());
    }
    if file_name_text.starts_with(".chatos-browser-") {
        return Err("HAR file name uses a reserved ChatOS prefix".to_string());
    }
    let parent_relative = relative_path.parent().unwrap_or_else(|| Path::new(""));
    let parent = workspace_root
        .join(parent_relative)
        .canonicalize()
        .map_err(|error| format!("canonicalize HAR parent failed: {error}"))?;
    if !parent.starts_with(workspace_root) || !parent.is_dir() {
        return Err("HAR parent must be an existing workspace directory".to_string());
    }
    let target = parent.join(file_name);
    if fs::symlink_metadata(target.as_path()).is_ok() {
        return Err(format!("HAR target already exists: {relative}"));
    }
    Ok(HarDestination { target, relative })
}

fn read_raw_har(path: &Path) -> Result<Value, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("browser did not create the expected HAR capture: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err("raw HAR capture is not a regular non-symlink file".to_string());
    }
    if metadata.len() > MAX_BROWSER_HAR_RAW_BYTES {
        return Err(format!(
            "raw HAR capture exceeds {MAX_BROWSER_HAR_RAW_BYTES} bytes"
        ));
    }
    let file = fs::File::open(path).map_err(|error| format!("open raw HAR failed: {error}"))?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_BROWSER_HAR_RAW_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("read raw HAR failed: {error}"))?;
    if bytes.len() as u64 > MAX_BROWSER_HAR_RAW_BYTES {
        return Err(format!(
            "raw HAR capture exceeds {MAX_BROWSER_HAR_RAW_BYTES} bytes"
        ));
    }
    serde_json::from_slice(bytes.as_slice())
        .map_err(|error| format!("parse raw HAR failed: {error}"))
}

fn publish_sanitized_har(destination: &HarDestination, value: Value) -> Result<u64, String> {
    let mut bytes = serde_json::to_vec_pretty(&value)
        .map_err(|error| format!("serialize sanitized HAR failed: {error}"))?;
    bytes.push(b'\n');
    if bytes.len() > MAX_BROWSER_HAR_OUTPUT_BYTES {
        return Err(format!(
            "sanitized HAR exceeds {MAX_BROWSER_HAR_OUTPUT_BYTES} bytes"
        ));
    }
    let write_result = (|| -> Result<(), String> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination.target.as_path())
            .map_err(|error| format!("create HAR target without overwrite failed: {error}"))?;
        file.write_all(bytes.as_slice())
            .map_err(|error| format!("write sanitized HAR failed: {error}"))?;
        file.flush()
            .map_err(|error| format!("flush sanitized HAR failed: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("sync sanitized HAR failed: {error}"))?;
        Ok(())
    })();
    if let Err(error) = write_result {
        remove_regular_file_or_symlink(destination.target.as_path());
        return Err(error);
    }
    let metadata = fs::symlink_metadata(destination.target.as_path())
        .map_err(|error| format!("verify sanitized HAR failed: {error}"))?;
    if metadata.file_type().is_symlink()
        || !metadata.file_type().is_file()
        || metadata.len() != bytes.len() as u64
    {
        remove_regular_file_or_symlink(destination.target.as_path());
        return Err("sanitized HAR verification failed".to_string());
    }
    Ok(metadata.len())
}

fn sanitize_har(
    raw: Value,
    include_request_bodies: bool,
    include_response_bodies: bool,
    max_body_chars: usize,
    max_entries: usize,
) -> Result<SanitizedHar, String> {
    let log = raw
        .get("log")
        .and_then(Value::as_object)
        .ok_or_else(|| "raw HAR is missing log object".to_string())?;
    let raw_entries = log
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| "raw HAR is missing entries array".to_string())?;
    let mut stats = HarSanitizationStats {
        original_entries: raw_entries.len(),
        ..HarSanitizationStats::default()
    };
    let start = raw_entries.len().saturating_sub(max_entries);
    stats.omitted_entries = start;
    let entries = raw_entries[start..]
        .iter()
        .filter_map(|entry| {
            sanitize_har_entry(
                entry,
                include_request_bodies,
                include_response_bodies,
                max_body_chars,
                &mut stats,
            )
        })
        .collect::<Vec<_>>();
    stats.exported_entries = entries.len();
    stats.omitted_entries += raw_entries.len().saturating_sub(start + entries.len());

    let mut sanitized_log = Map::new();
    sanitized_log.insert(
        "version".to_string(),
        Value::String(safe_text(log.get("version"), 32, "1.2")),
    );
    sanitized_log.insert(
        "creator".to_string(),
        sanitize_product(log.get("creator"), "ChatOS Browser HAR sanitizer"),
    );
    if log.get("browser").is_some() {
        sanitized_log.insert(
            "browser".to_string(),
            sanitize_product(log.get("browser"), "Browser"),
        );
    }
    let pages = log
        .get("pages")
        .and_then(Value::as_array)
        .map(|pages| {
            pages
                .iter()
                .take(MAX_HAR_PAGES)
                .filter_map(sanitize_har_page)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !pages.is_empty() {
        sanitized_log.insert("pages".to_string(), Value::Array(pages));
    }
    sanitized_log.insert("entries".to_string(), Value::Array(entries));
    sanitized_log.insert(
        "_chatosSanitization".to_string(),
        json!({
            "credentialsRedacted": true,
            "queryValuesRedacted": true,
            "cookieValuesRedacted": true,
            "headerValuesPolicy": "allowlist_or_redacted",
            "requestBodiesIncluded": include_request_bodies,
            "responseBodiesIncluded": include_response_bodies,
            "maxBodyChars": max_body_chars,
            "stats": stats.to_json(),
        }),
    );
    Ok(SanitizedHar {
        value: json!({ "log": sanitized_log }),
        stats,
    })
}

fn sanitize_har_entry(
    entry: &Value,
    include_request_bodies: bool,
    include_response_bodies: bool,
    max_body_chars: usize,
    stats: &mut HarSanitizationStats,
) -> Option<Value> {
    let entry = entry.as_object()?;
    let request = sanitize_har_request(
        entry.get("request")?,
        include_request_bodies,
        max_body_chars,
        stats,
    )?;
    let response = sanitize_har_response(
        entry.get("response")?,
        include_response_bodies,
        max_body_chars,
        stats,
    )?;
    let mut output = Map::new();
    copy_safe_string(entry, &mut output, "pageref", 256);
    copy_safe_string(entry, &mut output, "startedDateTime", 64);
    copy_number(entry, &mut output, "time");
    output.insert("request".to_string(), request);
    output.insert("response".to_string(), response);
    output.insert("cache".to_string(), json!({}));
    output.insert(
        "timings".to_string(),
        sanitize_timings(entry.get("timings")),
    );
    copy_safe_string(entry, &mut output, "serverIPAddress", 128);
    copy_safe_string(entry, &mut output, "connection", 128);
    copy_safe_string(entry, &mut output, "_resourceType", 64);
    Some(Value::Object(output))
}

fn sanitize_har_request(
    request: &Value,
    include_body: bool,
    max_body_chars: usize,
    stats: &mut HarSanitizationStats,
) -> Option<Value> {
    let request = request.as_object()?;
    let raw_url = request.get("url").and_then(Value::as_str)?;
    let url = sanitize_network_url(raw_url).unwrap_or_else(|| REDACTED.to_string());
    let mut output = Map::new();
    output.insert(
        "method".to_string(),
        Value::String(safe_text(request.get("method"), 16, "GET")),
    );
    output.insert("url".to_string(), Value::String(url));
    output.insert(
        "httpVersion".to_string(),
        Value::String(safe_text(request.get("httpVersion"), 32, "")),
    );
    output.insert(
        "headers".to_string(),
        sanitize_har_headers(request.get("headers"), stats),
    );
    output.insert(
        "queryString".to_string(),
        sanitize_har_query(request.get("queryString"), stats),
    );
    output.insert(
        "cookies".to_string(),
        sanitize_har_cookies(request.get("cookies"), stats),
    );
    copy_number(request, &mut output, "headersSize");
    copy_number(request, &mut output, "bodySize");
    if let Some(post_data) =
        sanitize_har_post_data(request.get("postData"), include_body, max_body_chars, stats)
    {
        output.insert("postData".to_string(), post_data);
    }
    Some(Value::Object(output))
}

fn sanitize_har_response(
    response: &Value,
    include_body: bool,
    max_body_chars: usize,
    stats: &mut HarSanitizationStats,
) -> Option<Value> {
    let response = response.as_object()?;
    let mut output = Map::new();
    copy_number(response, &mut output, "status");
    output.insert(
        "statusText".to_string(),
        Value::String(safe_text(response.get("statusText"), 256, "")),
    );
    output.insert(
        "httpVersion".to_string(),
        Value::String(safe_text(response.get("httpVersion"), 32, "")),
    );
    output.insert(
        "headers".to_string(),
        sanitize_har_headers(response.get("headers"), stats),
    );
    output.insert(
        "cookies".to_string(),
        sanitize_har_cookies(response.get("cookies"), stats),
    );
    output.insert(
        "content".to_string(),
        sanitize_har_content(response.get("content"), include_body, max_body_chars, stats),
    );
    let redirect = response
        .get("redirectURL")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .and_then(sanitize_network_url)
        .unwrap_or_default();
    output.insert("redirectURL".to_string(), Value::String(redirect));
    copy_number(response, &mut output, "headersSize");
    copy_number(response, &mut output, "bodySize");
    Some(Value::Object(output))
}

fn sanitize_har_headers(value: Option<&Value>, stats: &mut HarSanitizationStats) -> Value {
    let headers = value
        .and_then(Value::as_array)
        .map(|headers| {
            headers
                .iter()
                .take(MAX_HAR_HEADERS)
                .filter_map(|header| {
                    let header = header.as_object()?;
                    let name = normalize_header_name(header.get("name")?.as_str()?);
                    if name.is_empty() {
                        return None;
                    }
                    let raw_value = header.get("value").and_then(Value::as_str).unwrap_or("");
                    let (value, redacted) = sanitize_header_value(name.as_str(), raw_value);
                    stats.redacted_header_values += usize::from(redacted);
                    Some(json!({ "name": name, "value": value }))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Value::Array(headers)
}

fn sanitize_har_query(value: Option<&Value>, stats: &mut HarSanitizationStats) -> Value {
    let query = value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .take(MAX_HAR_QUERY_ITEMS)
                .filter_map(|item| {
                    let item = item.as_object()?;
                    let name = safe_text(item.get("name"), 256, "");
                    if name.is_empty() {
                        return None;
                    }
                    stats.redacted_query_values += 1;
                    Some(json!({ "name": name, "value": REDACTED }))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Value::Array(query)
}

fn sanitize_har_cookies(value: Option<&Value>, stats: &mut HarSanitizationStats) -> Value {
    let cookies = value
        .and_then(Value::as_array)
        .map(|cookies| {
            cookies
                .iter()
                .take(MAX_HAR_COOKIES)
                .filter_map(|cookie| {
                    let cookie = cookie.as_object()?;
                    let name = safe_text(cookie.get("name"), 256, "");
                    if name.is_empty() {
                        return None;
                    }
                    let mut output = Map::new();
                    output.insert("name".to_string(), Value::String(name));
                    output.insert("value".to_string(), Value::String(REDACTED.to_string()));
                    stats.redacted_cookie_values += 1;
                    for key in ["path", "domain", "expires", "sameSite"] {
                        copy_safe_string(cookie, &mut output, key, 512);
                    }
                    for key in ["httpOnly", "secure"] {
                        copy_bool(cookie, &mut output, key);
                    }
                    Some(Value::Object(output))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Value::Array(cookies)
}

fn sanitize_har_post_data(
    value: Option<&Value>,
    include_body: bool,
    max_body_chars: usize,
    stats: &mut HarSanitizationStats,
) -> Option<Value> {
    let data = value?.as_object()?;
    let mime_type = safe_text(data.get("mimeType"), 256, "");
    let text = data.get("text").and_then(Value::as_str);
    let params = data.get("params").and_then(Value::as_array);
    let available = text.is_some() || params.is_some_and(|items| !items.is_empty());
    if available {
        stats.available_request_bodies += 1;
    }
    let mut output = Map::new();
    output.insert("mimeType".to_string(), Value::String(mime_type.clone()));
    let mut included = false;
    let mut truncated = false;
    let mut redacted = false;
    if include_body && is_textual_content_type(mime_type.as_str()) {
        if let Some(text) = text {
            let (safe, body_redacted) = sanitize_body_text(text, mime_type.as_str());
            let original_chars = safe.chars().count();
            output.insert(
                "text".to_string(),
                Value::String(safe.chars().take(max_body_chars).collect()),
            );
            included = true;
            truncated = original_chars > max_body_chars;
            redacted |= body_redacted;
        }
        if let Some(params) = params {
            let safe_params = params
                .iter()
                .take(MAX_HAR_QUERY_ITEMS)
                .filter_map(|param| sanitize_har_post_param(param, max_body_chars, &mut redacted))
                .collect::<Vec<_>>();
            if !safe_params.is_empty() {
                output.insert("params".to_string(), Value::Array(safe_params));
                included = true;
            }
        }
    }
    if included {
        stats.included_request_bodies += 1;
    }
    stats.truncated_bodies += usize::from(truncated);
    stats.redacted_bodies += usize::from(redacted);
    output.insert(
        "_chatosBody".to_string(),
        json!({
            "available": available,
            "included": included,
            "truncated": truncated,
            "redactionApplied": redacted,
            "omittedReason": if !available {
                Value::Null
            } else if !include_body {
                Value::String("not_requested".to_string())
            } else if !is_textual_content_type(mime_type.as_str()) {
                Value::String("non_text_body".to_string())
            } else {
                Value::Null
            },
        }),
    );
    Some(Value::Object(output))
}

fn sanitize_har_post_param(
    value: &Value,
    max_body_chars: usize,
    redacted: &mut bool,
) -> Option<Value> {
    let value = value.as_object()?;
    let name = safe_text(value.get("name"), 256, "");
    if name.is_empty() {
        return None;
    }
    let mut output = Map::new();
    output.insert("name".to_string(), Value::String(name.clone()));
    if let Some(raw) = value.get("value").and_then(Value::as_str) {
        let safe = if is_sensitive_name(name.as_str()) {
            *redacted = true;
            REDACTED.to_string()
        } else {
            let (safe, body_redacted) = sanitize_body_text(raw, "text/plain");
            *redacted |= body_redacted;
            safe.chars().take(max_body_chars).collect()
        };
        output.insert("value".to_string(), Value::String(safe));
    }
    for key in ["fileName", "contentType"] {
        copy_safe_string(value, &mut output, key, 512);
    }
    Some(Value::Object(output))
}

fn sanitize_har_content(
    value: Option<&Value>,
    include_body: bool,
    max_body_chars: usize,
    stats: &mut HarSanitizationStats,
) -> Value {
    let Some(content) = value.and_then(Value::as_object) else {
        return json!({});
    };
    let mime_type = safe_text(content.get("mimeType"), 256, "");
    let encoding = safe_text(content.get("encoding"), 32, "");
    let text = content.get("text").and_then(Value::as_str);
    let available = text.is_some();
    if available {
        stats.available_response_bodies += 1;
    }
    let binary = encoding.eq_ignore_ascii_case("base64")
        || (!mime_type.is_empty() && !is_textual_content_type(mime_type.as_str()));
    let mut output = Map::new();
    copy_number(content, &mut output, "size");
    copy_number(content, &mut output, "compression");
    output.insert("mimeType".to_string(), Value::String(mime_type.clone()));
    if !encoding.is_empty() {
        output.insert("encoding".to_string(), Value::String(encoding));
    }
    let mut included = false;
    let mut truncated = false;
    let mut redacted = false;
    if include_body && !binary {
        if let Some(text) = text {
            let (safe, body_redacted) = sanitize_body_text(text, mime_type.as_str());
            let original_chars = safe.chars().count();
            output.insert(
                "text".to_string(),
                Value::String(safe.chars().take(max_body_chars).collect()),
            );
            included = true;
            truncated = original_chars > max_body_chars;
            redacted = body_redacted;
        }
    }
    if included {
        stats.included_response_bodies += 1;
    }
    stats.truncated_bodies += usize::from(truncated);
    stats.redacted_bodies += usize::from(redacted);
    output.insert(
        "_chatosBody".to_string(),
        json!({
            "available": available,
            "included": included,
            "truncated": truncated,
            "redactionApplied": redacted,
            "omittedReason": if !available {
                Value::Null
            } else if !include_body {
                Value::String("not_requested".to_string())
            } else if binary {
                Value::String("binary_or_base64_body".to_string())
            } else {
                Value::Null
            },
        }),
    );
    Value::Object(output)
}

fn sanitize_har_page(value: &Value) -> Option<Value> {
    let page = value.as_object()?;
    let mut output = Map::new();
    copy_safe_string(page, &mut output, "startedDateTime", 64);
    copy_safe_string(page, &mut output, "id", 256);
    copy_safe_string(page, &mut output, "title", MAX_HAR_TEXT_CHARS);
    output.insert(
        "pageTimings".to_string(),
        sanitize_page_timings(page.get("pageTimings")),
    );
    Some(Value::Object(output))
}

fn sanitize_product(value: Option<&Value>, fallback_name: &str) -> Value {
    let product = value.and_then(Value::as_object);
    json!({
        "name": safe_text(product.and_then(|value| value.get("name")), 256, fallback_name),
        "version": safe_text(product.and_then(|value| value.get("version")), 128, ""),
    })
}

fn sanitize_page_timings(value: Option<&Value>) -> Value {
    let Some(timings) = value.and_then(Value::as_object) else {
        return json!({});
    };
    let mut output = Map::new();
    copy_number(timings, &mut output, "onContentLoad");
    copy_number(timings, &mut output, "onLoad");
    Value::Object(output)
}

fn sanitize_timings(value: Option<&Value>) -> Value {
    let Some(timings) = value.and_then(Value::as_object) else {
        return json!({});
    };
    let mut output = Map::new();
    for key in [
        "blocked", "dns", "connect", "send", "wait", "receive", "ssl",
    ] {
        copy_number(timings, &mut output, key);
    }
    Value::Object(output)
}

fn safe_text(value: Option<&Value>, max_chars: usize, fallback: &str) -> String {
    let raw = value.and_then(Value::as_str).unwrap_or(fallback);
    let (safe, _) = sanitize_body_text(raw, "text/plain");
    normalize_inline_text(safe.as_str(), max_chars)
}

fn copy_safe_string(
    source: &Map<String, Value>,
    destination: &mut Map<String, Value>,
    key: &str,
    max_chars: usize,
) {
    if let Some(value) = source.get(key).and_then(Value::as_str) {
        destination.insert(
            key.to_string(),
            Value::String(safe_text(
                Some(&Value::String(value.to_string())),
                max_chars,
                "",
            )),
        );
    }
}

fn copy_number(source: &Map<String, Value>, destination: &mut Map<String, Value>, key: &str) {
    if let Some(value) = source.get(key).filter(|value| {
        value.is_i64() || value.is_u64() || value.as_f64().is_some_and(|number| number.is_finite())
    }) {
        destination.insert(key.to_string(), value.clone());
    }
}

fn copy_bool(source: &Map<String, Value>, destination: &mut Map<String, Value>, key: &str) {
    if let Some(value) = source.get(key).and_then(Value::as_bool) {
        destination.insert(key.to_string(), Value::Bool(value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_har() -> Value {
        json!({
            "log": {
                "version": "1.2",
                "creator": {"name": "agent-browser", "version": "0.31.2"},
                "entries": [{
                    "startedDateTime": "2026-07-23T00:00:00Z",
                    "time": 12.0,
                    "request": {
                        "method": "POST",
                        "url": "https://user:secret@example.com/api?token=query-secret&safe=value",
                        "httpVersion": "HTTP/2",
                        "headers": [
                            {"name": "Content-Type", "value": "application/json"},
                            {"name": "Authorization", "value": "Bearer header-secret"},
                            {"name": "X-Debug", "value": "unknown-secret"}
                        ],
                        "queryString": [{"name": "token", "value": "query-secret"}],
                        "cookies": [{"name": "session", "value": "cookie-secret", "httpOnly": true}],
                        "postData": {
                            "mimeType": "application/json",
                            "text": "{\"password\":\"body-secret\",\"safe\":\"visible-request\"}"
                        },
                        "headersSize": -1,
                        "bodySize": 50
                    },
                    "response": {
                        "status": 200,
                        "statusText": "OK",
                        "httpVersion": "HTTP/2",
                        "headers": [{"name": "Set-Cookie", "value": "session=response-cookie-secret"}],
                        "cookies": [{"name": "session", "value": "response-cookie-secret"}],
                        "content": {
                            "size": 70,
                            "mimeType": "application/json",
                            "text": "{\"refresh_token\":\"response-secret\",\"result\":\"visible-response\"}"
                        },
                        "redirectURL": "https://example.com/next?secret=redirect-secret",
                        "headersSize": -1,
                        "bodySize": 70
                    },
                    "cache": {},
                    "timings": {"send": 1, "wait": 10, "receive": 1}
                }]
            }
        })
    }

    #[test]
    fn har_sanitization_redacts_urls_headers_cookies_and_bodies() {
        let sanitized = sanitize_har(sample_har(), true, true, 4096, 10).expect("sanitize HAR");
        let serialized = serde_json::to_string(&sanitized.value).expect("serialize HAR");
        assert!(serialized.contains("visible-request"));
        assert!(serialized.contains("visible-response"));
        for secret in [
            "query-secret",
            "header-secret",
            "unknown-secret",
            "cookie-secret",
            "body-secret",
            "response-secret",
            "redirect-secret",
            "user:secret",
        ] {
            assert!(!serialized.contains(secret), "HAR leaked {secret}");
        }
        assert!(serialized.contains("%5BREDACTED%5D"));
        assert_eq!(sanitized.stats.exported_entries, 1);
        assert!(sanitized.stats.redacted_header_values >= 3);
        assert!(sanitized.stats.redacted_cookie_values >= 2);
        assert_eq!(sanitized.stats.included_request_bodies, 1);
        assert_eq!(sanitized.stats.included_response_bodies, 1);
    }

    #[test]
    fn har_bodies_are_omitted_by_default_and_entries_are_bounded() {
        let mut raw = sample_har();
        let entry = raw.pointer("/log/entries/0").cloned().unwrap();
        raw.pointer_mut("/log/entries")
            .and_then(Value::as_array_mut)
            .unwrap()
            .extend([entry.clone(), entry]);
        let sanitized = sanitize_har(raw, false, false, 1024, 2).expect("sanitize HAR");
        let serialized = serde_json::to_string(&sanitized.value).expect("serialize HAR");
        assert!(!serialized.contains("visible-request"));
        assert!(!serialized.contains("visible-response"));
        assert_eq!(sanitized.stats.original_entries, 3);
        assert_eq!(sanitized.stats.exported_entries, 2);
        assert_eq!(sanitized.stats.omitted_entries, 1);
    }

    #[test]
    fn har_destination_rejects_traversal_wrong_extension_and_overwrite() {
        let root = std::env::temp_dir().join(format!(
            "chatos_browser_har_test_{}",
            Uuid::new_v4().simple()
        ));
        fs::create_dir_all(root.as_path()).expect("create HAR workspace");
        let root = root.canonicalize().expect("canonical HAR workspace");
        assert!(prepare_har_destination(root.as_path(), "../capture.har").is_err());
        assert!(prepare_har_destination(root.as_path(), "capture.json").is_err());
        fs::write(root.join("existing.har"), b"existing").expect("write existing HAR");
        assert!(prepare_har_destination(root.as_path(), "existing.har").is_err());
        let _ = fs::remove_dir_all(root);
    }
}
