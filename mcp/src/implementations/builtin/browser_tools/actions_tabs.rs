// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use serde_json::{json, Value};

use super::actions_network::{sanitize_network_url, sanitize_response_page_url};
use super::actions_shared::{
    browser_result_data, build_browser_action_summary, copy_response_fields, fail_json,
    finalize_browser_action_response, is_success, normalize_inline_text, run_browser_command,
};
use super::BoundContext;

const MAX_BROWSER_TABS: usize = 64;
const MAX_REPORTED_BROWSER_TABS: usize = 10_000;
const MAX_BROWSER_TAB_TITLE_CHARS: usize = 240;
const MAX_BROWSER_TAB_ID_CHARS: usize = 32;

pub(super) async fn browser_tabs_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
) -> Result<Value, String> {
    let session = super::super::context::conversation_key(conversation_id);
    let result = run_tab_list(&ctx, session.as_str()).await?;
    if !is_success(&result) {
        return Ok(fail_json(&result, "Failed to list browser tabs"));
    }

    Ok(finalize_tab_response(
        &ctx,
        session.as_str(),
        tab_list_response(&result),
        "Listed open browser tabs.",
    )
    .await)
}

pub(super) async fn browser_tab_new_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    url: Option<String>,
) -> Result<Value, String> {
    let session = super::super::context::conversation_key(conversation_id);
    let mut args = vec!["new".to_string()];
    if let Some(url) = url {
        args.push(url);
    }
    let result = run_browser_command(
        &ctx,
        session.as_str(),
        "tab",
        args,
        ctx.command_timeout_seconds.max(60),
    )
    .await?;
    if !is_success(&result) {
        return Ok(fail_json(&result, "Failed to open a new browser tab"));
    }

    let created_tab_id = browser_result_data(&result)
        .get("tabId")
        .and_then(Value::as_str)
        .filter(|value| is_valid_tab_id(value))
        .map(ToOwned::to_owned);
    let listed = run_tab_list(&ctx, session.as_str()).await?;
    if !is_success(&listed) {
        return Ok(fail_json(
            &listed,
            "New browser tab opened, but refreshing the tab list failed",
        ));
    }

    let mut response = tab_list_response(&listed);
    response["created_tab_id"] = created_tab_id.map(Value::String).unwrap_or(Value::Null);
    copy_response_fields(&mut response, &result, &["browser_session"]);
    Ok(finalize_tab_response(
        &ctx,
        session.as_str(),
        response,
        "Opened a new browser tab.",
    )
    .await)
}

pub(super) async fn browser_tab_switch_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    tab_id: String,
) -> Result<Value, String> {
    let tab_id = normalize_tab_id(tab_id)?;
    let session = super::super::context::conversation_key(conversation_id);
    let result = run_browser_command(
        &ctx,
        session.as_str(),
        "tab",
        vec![tab_id.clone()],
        ctx.command_timeout_seconds,
    )
    .await?;
    if !is_success(&result) {
        return Ok(fail_json(
            &result,
            format!("Failed to switch to browser tab {tab_id}").as_str(),
        ));
    }

    let listed = run_tab_list(&ctx, session.as_str()).await?;
    if !is_success(&listed) {
        return Ok(fail_json(
            &listed,
            "Browser tab switched, but refreshing the tab list failed",
        ));
    }

    let mut response = tab_list_response(&listed);
    response["switched_tab_id"] = Value::String(tab_id);
    copy_response_fields(&mut response, &result, &["browser_session"]);
    Ok(finalize_tab_response(
        &ctx,
        session.as_str(),
        response,
        "Switched the active browser tab.",
    )
    .await)
}

pub(super) async fn browser_tab_close_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    tab_id: String,
) -> Result<Value, String> {
    let tab_id = normalize_tab_id(tab_id)?;
    let session = super::super::context::conversation_key(conversation_id);
    let before = run_tab_list(&ctx, session.as_str()).await?;
    if !is_success(&before) {
        return Ok(fail_json(
            &before,
            "Failed to inspect browser tabs before close",
        ));
    }
    let before_data = browser_result_data(&before);
    let raw_tabs = before_data
        .get("tabs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let valid_ids = valid_tab_ids(raw_tabs.as_slice());
    if !valid_ids.contains(tab_id.as_str()) {
        let mut response = json!({
            "_summary_text": format!("Browser tab close failed because {tab_id} is not open."),
            "success": false,
            "error": format!("browser tab {tab_id} is not open")
        });
        copy_response_fields(&mut response, &before, &["browser_session"]);
        return Ok(response);
    }
    if valid_ids.len() <= 1 {
        let mut response = json!({
            "_summary_text": "Browser tab close failed because the last tab must remain open.",
            "success": false,
            "error": "cannot close the last browser tab"
        });
        copy_response_fields(&mut response, &before, &["browser_session"]);
        return Ok(response);
    }

    let result = run_browser_command(
        &ctx,
        session.as_str(),
        "tab",
        vec!["close".to_string(), tab_id.clone()],
        ctx.command_timeout_seconds,
    )
    .await?;
    if !is_success(&result) {
        return Ok(fail_json(
            &result,
            format!("Failed to close browser tab {tab_id}").as_str(),
        ));
    }

    let listed = run_tab_list(&ctx, session.as_str()).await?;
    if !is_success(&listed) {
        return Ok(fail_json(
            &listed,
            "Browser tab closed, but refreshing the tab list failed",
        ));
    }

    let mut response = tab_list_response(&listed);
    response["closed_tab_id"] = Value::String(tab_id);
    copy_response_fields(&mut response, &result, &["browser_session"]);
    Ok(finalize_tab_response(
        &ctx,
        session.as_str(),
        response,
        "Closed the selected browser tab.",
    )
    .await)
}

async fn run_tab_list(ctx: &BoundContext, session: &str) -> Result<Value, String> {
    run_browser_command(
        ctx,
        session,
        "tab",
        vec!["list".to_string()],
        ctx.command_timeout_seconds,
    )
    .await
}

async fn finalize_tab_response(
    ctx: &BoundContext,
    session: &str,
    response: Value,
    action_summary: &str,
) -> Value {
    let mut response = finalize_browser_action_response(
        ctx,
        session,
        response,
        action_summary,
        Some("Use stable tab IDs with browser_tab_switch or browser_tab_close."),
    )
    .await;
    sanitize_response_page_url(&mut response);
    response["_summary_text"] = Value::String(build_browser_action_summary(
        action_summary,
        &response,
        Some("Use stable tab IDs with browser_tab_switch or browser_tab_close."),
    ));
    response
}

fn tab_list_response(result: &Value) -> Value {
    let data = browser_result_data(result);
    let raw_tabs = data
        .get("tabs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let reported_count = valid_tab_ids(raw_tabs.as_slice())
        .len()
        .min(MAX_REPORTED_BROWSER_TABS);
    let tabs = normalize_tabs(raw_tabs.as_slice());
    let returned_count = tabs.len();
    let active_tab_id = tabs
        .iter()
        .find(|tab| tab.get("active").and_then(Value::as_bool).unwrap_or(false))
        .and_then(|tab| tab.get("tab_id"))
        .cloned()
        .unwrap_or(Value::Null);
    let mut response = json!({
        "success": true,
        "tabs": tabs,
        "tab_count": reported_count,
        "returned_count": returned_count,
        "omitted_count": reported_count.saturating_sub(returned_count),
        "truncated": reported_count > returned_count,
        "active_tab_id": active_tab_id,
        "stable_tab_ids": true,
        "url_query_values_redacted": true,
        "non_web_urls_omitted": true,
    });
    copy_response_fields(&mut response, result, &["browser_session"]);
    response
}

fn normalize_tabs(raw_tabs: &[Value]) -> Vec<Value> {
    let mut seen = HashSet::new();
    let mut tabs = Vec::new();
    let mut omitted_active_tab = None;
    for tab in raw_tabs {
        if tab.get("type").and_then(Value::as_str) != Some("page") {
            continue;
        }
        let Some(tab_id) = tab.get("tabId").and_then(Value::as_str) else {
            continue;
        };
        if !is_valid_tab_id(tab_id) || !seen.insert(tab_id.to_string()) {
            continue;
        }
        let active = tab.get("active").and_then(Value::as_bool).unwrap_or(false);
        let normalized = json!({
            "tab_id": tab_id,
            "active": active,
            "title": tab.get("title").and_then(Value::as_str).and_then(sanitize_tab_title),
            "url": tab.get("url").and_then(Value::as_str).and_then(sanitize_tab_url),
        });
        if tabs.len() < MAX_BROWSER_TABS {
            tabs.push(normalized);
        } else if active {
            omitted_active_tab = Some(normalized);
        }
    }
    if let Some(active_tab) = omitted_active_tab {
        let active_tab_id = active_tab
            .get("tab_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let active_already_returned = tabs
            .iter()
            .any(|tab| tab.get("tab_id").and_then(Value::as_str) == Some(active_tab_id));
        if !active_already_returned {
            tabs.pop();
            tabs.push(active_tab);
        }
    }
    tabs
}

fn valid_tab_ids(raw_tabs: &[Value]) -> HashSet<String> {
    raw_tabs
        .iter()
        .filter(|tab| tab.get("type").and_then(Value::as_str) == Some("page"))
        .filter_map(|tab| tab.get("tabId").and_then(Value::as_str))
        .filter(|tab_id| is_valid_tab_id(tab_id))
        .map(ToOwned::to_owned)
        .collect()
}

fn sanitize_tab_url(value: &str) -> Option<String> {
    if value == "about:blank" {
        return Some(value.to_string());
    }
    let sanitized = sanitize_network_url(value)?;
    if sanitized.starts_with("http://") || sanitized.starts_with("https://") {
        Some(sanitized)
    } else {
        None
    }
}

fn sanitize_tab_title(value: &str) -> Option<String> {
    let title = normalize_inline_text(value, MAX_BROWSER_TAB_TITLE_CHARS);
    if title.is_empty() {
        return None;
    }
    if url::Url::parse(title.as_str()).is_ok() {
        return sanitize_tab_url(title.as_str());
    }
    Some(title)
}

fn normalize_tab_id(value: String) -> Result<String, String> {
    let value = value.trim();
    if !is_valid_tab_id(value) {
        return Err("tab_id must be a stable browser tab ID such as t1".to_string());
    }
    Ok(value.to_string())
}

fn is_valid_tab_id(value: &str) -> bool {
    value.len() >= 2
        && value.len() <= MAX_BROWSER_TAB_ID_CHARS
        && value.starts_with('t')
        && value[1..].bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_ids_require_agent_browser_stable_id_shape() {
        assert_eq!(normalize_tab_id(" t12 ".to_string()).unwrap(), "t12");
        assert!(normalize_tab_id("12".to_string()).is_err());
        assert!(normalize_tab_id("t1/../../secret".to_string()).is_err());
        assert!(normalize_tab_id("tab-name".to_string()).is_err());
    }

    #[test]
    fn tab_urls_drop_credentials_non_web_content_and_query_values() {
        assert_eq!(
            sanitize_tab_url("https://user:pass@example.com/app?token=secret&mode=full#part")
                .as_deref(),
            Some("https://example.com/app?token=%5BREDACTED%5D&mode=%5BREDACTED%5D")
        );
        assert_eq!(
            sanitize_tab_url("about:blank").as_deref(),
            Some("about:blank")
        );
        assert!(sanitize_tab_url("data:text/plain,secret").is_none());
        assert!(sanitize_tab_url("file:///private/workspace/secret.html").is_none());
        assert!(sanitize_tab_url("javascript:alert(1)").is_none());
        assert!(sanitize_tab_title("data:text/html,<h1>secret</h1>").is_none());
        assert_eq!(
            sanitize_tab_title("https://user:pass@example.com/?token=secret").as_deref(),
            Some("https://example.com/?token=%5BREDACTED%5D")
        );
    }

    #[test]
    fn tab_list_normalization_is_bounded_and_omits_non_pages() {
        let raw = vec![
            json!({
                "active": true,
                "tabId": "t1",
                "title": "  Example   page  ",
                "type": "page",
                "url": "https://example.com/?token=secret"
            }),
            json!({
                "active": false,
                "tabId": "t2",
                "title": "data:text/plain,secret",
                "type": "page",
                "url": "data:text/plain,secret"
            }),
            json!({
                "active": false,
                "tabId": "t3",
                "title": "Worker",
                "type": "service_worker",
                "url": "https://example.com/sw.js"
            }),
        ];

        let normalized = normalize_tabs(raw.as_slice());
        assert_eq!(normalized.len(), 2);
        assert_eq!(normalized[0]["tab_id"], "t1");
        assert_eq!(normalized[0]["title"], "Example page");
        assert_eq!(
            normalized[0]["url"],
            "https://example.com/?token=%5BREDACTED%5D"
        );
        assert!(normalized[1]["url"].is_null());
        assert!(normalized[1]["title"].is_null());
    }

    #[test]
    fn bounded_tab_list_always_retains_the_active_tab() {
        let raw = (1..=MAX_BROWSER_TABS + 1)
            .map(|index| {
                json!({
                    "active": index == MAX_BROWSER_TABS + 1,
                    "tabId": format!("t{index}"),
                    "title": format!("Tab {index}"),
                    "type": "page",
                    "url": format!("https://example.com/{index}")
                })
            })
            .collect::<Vec<_>>();

        let normalized = normalize_tabs(raw.as_slice());
        assert_eq!(normalized.len(), MAX_BROWSER_TABS);
        assert!(normalized.iter().any(|tab| {
            tab["tab_id"] == format!("t{}", MAX_BROWSER_TABS + 1) && tab["active"] == true
        }));
    }
}
