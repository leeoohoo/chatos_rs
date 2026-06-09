use serde_json::{json, Value};

use crate::browser_command_support::{
    browser_command_error_text, browser_command_succeeded, parse_browser_command_eval_payload,
};
use crate::browser_page_insights::{
    inspection_warning_line, is_meaningful_browser_url, page_label_from_response,
    page_state_warning_line, visible_refs_summary_line,
};
use crate::browser_page_state_view::{browser_console_state_view, has_console_observation};
use crate::browser_runtime::{
    new_browser_session, run_browser_command as runtime_run_browser_command, BrowserRuntimeSession,
};

use super::BoundContext;

pub(super) use crate::research_output::normalize_inline_text;

pub(super) async fn enrich_response_with_page_state(
    ctx: &BoundContext,
    conversation_key: &str,
    response: &mut Value,
    full_snapshot: bool,
) {
    enrich_response_with_page_metadata(ctx, conversation_key, response).await;

    let snapshot_args = if full_snapshot {
        Vec::new()
    } else {
        vec!["-c".to_string()]
    };
    let snapshot_result = run_browser_command(
        ctx,
        conversation_key,
        "snapshot",
        snapshot_args,
        ctx.command_timeout_seconds,
    )
    .await;
    match snapshot_result {
        Ok(value) if is_success(&value) => {
            let data = value.get("data").cloned().unwrap_or_else(|| json!({}));
            apply_snapshot_payload(response, &data, ctx.max_snapshot_chars);
        }
        Ok(value) => {
            append_page_state_warning(response, browser_error_message(&value, "snapshot failed"))
        }
        Err(err) => append_page_state_warning(response, err),
    }

    mark_page_state_available(response);
}

pub(super) async fn finalize_browser_action_response(
    ctx: &BoundContext,
    conversation_key: &str,
    mut response: Value,
    action_summary: &str,
    next_hint: Option<&str>,
) -> Value {
    enrich_response_with_page_state(ctx, conversation_key, &mut response, false).await;
    response["_summary_text"] = Value::String(build_browser_action_summary(
        action_summary,
        &response,
        next_hint,
    ));
    response
}

pub(super) async fn run_basic_browser_action(
    ctx: &BoundContext,
    conversation_key: &str,
    command: &str,
    args: Vec<String>,
    timeout_seconds: u64,
    action_error: String,
    response: Value,
    action_summary: &str,
    next_hint: Option<&str>,
) -> Result<Value, String> {
    let result = run_browser_command(ctx, conversation_key, command, args, timeout_seconds).await?;
    if !is_success(&result) {
        return Ok(fail_json(&result, action_error.as_str()));
    }

    Ok(
        finalize_browser_action_response(
            ctx,
            conversation_key,
            response,
            action_summary,
            next_hint,
        )
        .await,
    )
}

pub(super) async fn enrich_response_with_page_metadata(
    ctx: &BoundContext,
    conversation_key: &str,
    response: &mut Value,
) {
    let metadata_result = run_browser_command(
        ctx,
        conversation_key,
        "eval",
        vec![current_page_metadata_expression()],
        ctx.command_timeout_seconds,
    )
    .await;

    match metadata_result {
        Ok(value) if is_success(&value) => {
            let raw = value
                .get("data")
                .and_then(|v| v.get("result"))
                .cloned()
                .unwrap_or(Value::Null);
            let parsed = parse_browser_eval_payload(raw);
            if let Some(url) = parsed.get("url").and_then(|v| v.as_str()) {
                upsert_string_field(response, "url", url);
            }
            if let Some(title) = parsed.get("title").and_then(|v| v.as_str()) {
                upsert_string_field(response, "title", title);
            }
        }
        Ok(value) => append_page_state_warning(
            response,
            browser_error_message(&value, "page metadata unavailable"),
        ),
        Err(err) => append_page_state_warning(response, err),
    }

    mark_page_state_available(response);
}

fn current_page_metadata_expression() -> String {
    r#"JSON.stringify({url: window.location.href, title: document.title})"#.to_string()
}

pub(super) fn parse_browser_eval_payload(raw: Value) -> Value {
    parse_browser_command_eval_payload(raw)
}

pub(super) fn browser_result_data(value: &Value) -> Value {
    value.get("data").cloned().unwrap_or_else(|| json!({}))
}

pub(super) fn apply_snapshot_payload(
    response: &mut Value,
    data: &Value,
    max_snapshot_chars: usize,
) {
    let snapshot = data.get("snapshot").and_then(|v| v.as_str()).unwrap_or("");
    let refs = data.get("refs").and_then(|v| v.as_object());
    upsert_string_field(
        response,
        "snapshot",
        truncate_chars(snapshot, max_snapshot_chars).as_str(),
    );
    if let Some(map) = response.as_object_mut() {
        map.insert(
            "element_count".to_string(),
            json!(refs.map(|v| v.len()).unwrap_or(0)),
        );
    }
}

fn upsert_string_field(response: &mut Value, key: &str, value: &str) {
    if value.trim().is_empty() {
        return;
    }
    if let Some(map) = response.as_object_mut() {
        map.insert(key.to_string(), Value::String(value.to_string()));
    }
}

fn append_page_state_warning(response: &mut Value, warning: String) {
    let warning = warning.trim();
    if warning.is_empty() {
        return;
    }
    if let Some(map) = response.as_object_mut() {
        let merged = match map.get("page_state_warning").and_then(|v| v.as_str()) {
            Some(existing) if !existing.trim().is_empty() => {
                format!("{} | {}", existing.trim(), warning)
            }
            _ => warning.to_string(),
        };
        map.insert("page_state_warning".to_string(), Value::String(merged));
    }
}

pub(super) fn mark_page_state_available(response: &mut Value) {
    if let Some(map) = response.as_object_mut() {
        let available = map
            .get("url")
            .and_then(|v| v.as_str())
            .map(is_meaningful_browser_url)
            .unwrap_or(false)
            || map
                .get("title")
                .and_then(|v| v.as_str())
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false)
            || map
                .get("snapshot")
                .and_then(|v| v.as_str())
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false);
        map.insert("page_state_available".to_string(), Value::Bool(available));
    }
}

fn has_non_empty_snapshot(response: &Value) -> bool {
    response
        .get("snapshot")
        .and_then(|value| value.as_str())
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

pub(super) fn has_meaningful_page_signal(response: &Value) -> bool {
    response
        .get("url")
        .and_then(|value| value.as_str())
        .map(is_meaningful_browser_url)
        .unwrap_or(false)
        || response
            .get("title")
            .and_then(|value| value.as_str())
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
        || has_non_empty_snapshot(response)
}

pub(super) fn has_console_signal(response: &Value) -> bool {
    has_console_observation(response)
}

pub(super) fn copy_response_fields(target: &mut Value, source: &Value, fields: &[&str]) {
    let Some(target_map) = target.as_object_mut() else {
        return;
    };
    for field in fields {
        if let Some(value) = source.get(*field) {
            target_map.insert((*field).to_string(), value.clone());
        }
    }
}

pub(super) fn browser_inspect_warning(source: &str, detail: &str) -> String {
    let normalized = normalize_inline_text(detail, 180);
    if normalized.is_empty() {
        format!("{} unavailable", source)
    } else {
        format!("{}: {}", source, normalized)
    }
}

pub(super) fn summarize_browser_failure(response: &Value, fallback: &str) -> String {
    if let Some(error) = response
        .get("error")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
    {
        return error.to_string();
    }
    response
        .get("_summary_text")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(fallback)
        .to_string()
}

pub(super) fn build_browser_action_summary(
    action: &str,
    response: &Value,
    next_hint: Option<&str>,
) -> String {
    let mut parts = vec![action.trim().to_string()];
    let page_label = page_label_from_response(response);
    if !page_label.is_empty() {
        parts.push(page_label);
    }

    if let Some(line) = visible_refs_summary_line(response) {
        parts.push(line);
    }
    if response
        .get("snapshot")
        .and_then(|v| v.as_str())
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
    {
        parts.push("A snapshot of the current page is included.".to_string());
    }
    if let Some(line) = page_state_warning_line(response) {
        parts.push(line);
    }
    if let Some(hint) = next_hint.map(str::trim).filter(|value| !value.is_empty()) {
        parts.push(hint.to_string());
    }

    parts.join(" ")
}

pub(super) fn build_browser_inspect_summary(response: &Value, vision_requested: bool) -> String {
    let has_page_signal = has_meaningful_page_signal(response);
    let has_console = has_console_signal(response);
    let mut parts = vec![if has_page_signal || has_console {
        "Observed the current browser page.".to_string()
    } else {
        "No active browser page was available.".to_string()
    }];

    let page_label = page_label_from_response(response);
    if !page_label.is_empty() {
        parts.push(page_label);
    }

    if let Some(line) = visible_refs_summary_line(response) {
        parts.push(line);
    }

    let console = browser_console_state_view(response);
    if console.total_messages > 0 || console.total_errors > 0 || console.has_message_count_by_type {
        parts.push(format!(
            "Console summary: {} message(s), {} JavaScript error(s).",
            console.total_messages, console.total_errors
        ));
    }

    if vision_requested {
        if !has_page_signal {
            parts
                .push("Vision inspection was skipped because no active page was open.".to_string());
        } else if let Some(vision) = response.get("vision").and_then(|value| value.as_object()) {
            let enabled = vision
                .get("enabled")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            let mode = vision
                .get("mode")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown");
            let model = vision
                .get("model")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown");
            let transport = vision
                .get("transport")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown");
            if enabled {
                parts.push(format!(
                    "Vision answered the inspection question via {} / {} over {}.",
                    mode, model, transport
                ));
            } else {
                parts.push("Vision was requested but unavailable.".to_string());
            }
        } else {
            parts.push("Vision was requested but no screenshot analysis was returned.".to_string());
        }
    } else {
        parts.push(
            "Use browser_click/browser_type with snapshot refs, or pass question to browser_inspect when visual layout matters."
                .to_string(),
        );
    }

    if let Some(line) = inspection_warning_line(response) {
        parts.push(line);
    }

    parts.join(" ")
}

pub(super) async fn run_browser_command(
    ctx: &BoundContext,
    conversation_key: &str,
    command: &str,
    args: Vec<String>,
    timeout_seconds: u64,
) -> Result<Value, String> {
    let session = get_or_create_session(ctx, conversation_key);
    runtime_run_browser_command(&ctx.workspace_dir, &session, command, args, timeout_seconds).await
}

fn get_or_create_session(ctx: &BoundContext, conversation_key: &str) -> BrowserRuntimeSession {
    let mut sessions = ctx.sessions.lock();
    if let Some(existing) = sessions.get(conversation_key) {
        return existing.clone();
    }

    let session = new_browser_session();
    sessions.insert(conversation_key.to_string(), session.clone());
    session
}

pub(super) fn is_success(value: &Value) -> bool {
    browser_command_succeeded(value)
}

pub(super) fn browser_error_message(value: &Value, fallback: &str) -> String {
    browser_command_error_text(value, fallback)
}

pub(super) fn fail_json(value: &Value, fallback: &str) -> Value {
    let error = browser_error_message(value, fallback);
    json!({
        "_summary_text": format!("Browser action failed: {}.", normalize_inline_text(error.as_str(), 180)),
        "success": false,
        "error": error
    })
}

pub(super) fn normalize_ref(reference: String) -> String {
    let trimmed = reference.trim();
    if trimmed.starts_with('@') {
        trimmed.to_string()
    } else {
        format!("@{}", trimmed)
    }
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let total = text.chars().count();
    if total <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect()
}
