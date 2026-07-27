// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;

use axum::extract::{Path as AxumPath, State};
use axum::Json;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use chatos_mcp::{BrowserToolsOptions, BrowserToolsService};
use serde_json::{json, Value};

use crate::workspace::paths::canonicalize_existing_dir;
use crate::LocalRuntime;

use super::super::types::{LocalApiError, LocalBrowserSessionCommandRequest};

const BROWSER_UI_MAX_URL_CHARS: usize = 4_096;
const BROWSER_UI_MAX_TEXT_CHARS: usize = 32 * 1024;
const BROWSER_UI_DEFAULT_NETWORK_LIMIT: usize = 100;
const BROWSER_UI_MAX_NETWORK_LIMIT: usize = 200;
const BROWSER_UI_DEFAULT_NETWORK_BODY_CHARS: usize = 16 * 1024;
const BROWSER_UI_MAX_NETWORK_BODY_CHARS: usize = 64 * 1024;
const BROWSER_UI_DEFAULT_HAR_ENTRIES: usize = 500;
const BROWSER_UI_MAX_HAR_ENTRIES: usize = 1_000;
const BROWSER_UI_DEFAULT_WEBSOCKET_LIMIT: usize = 100;
const BROWSER_UI_MAX_WEBSOCKET_LIMIT: usize = 200;
const BROWSER_UI_DEFAULT_WEBSOCKET_PAYLOAD_CHARS: usize = 1_024;
const BROWSER_UI_MAX_WEBSOCKET_PAYLOAD_CHARS: usize = 4_096;

pub(crate) async fn local_browser_session_command(
    State(runtime): State<LocalRuntime>,
    AxumPath(session_id): AxumPath<String>,
    Json(request): Json<LocalBrowserSessionCommandRequest>,
) -> Result<Json<Value>, LocalApiError> {
    let workspace_id = normalize_required(request.workspace_id.as_str(), "workspace_id")?;
    let action = normalize_required(request.action.as_str(), "action")?.to_ascii_lowercase();
    let workspace_root = {
        let state = runtime.state.read().await;
        state
            .workspace_by_id(workspace_id.as_str())
            .map(|workspace| workspace.absolute_root.clone())
            .ok_or_else(|| LocalApiError::bad_request("workspace is not registered locally"))?
    };
    let workspace_root = canonicalize_existing_dir(workspace_root.as_path())
        .map_err(|error| LocalApiError::bad_request(error.to_string()))?;
    let service = BrowserToolsService::new(BrowserToolsOptions {
        server_name: "local_browser_session_ui".to_string(),
        workspace_dir: workspace_root.clone(),
        command_timeout_seconds: 30,
        max_snapshot_chars: 8_000,
        vision_adapter: None,
        route_interception_enabled: false,
        full_cdp_access_enabled: false,
        schema_catalog_only: false,
    })
    .map_err(LocalApiError::bad_gateway)?;
    let conversation_id = format!("browser-ui-{session_id}");
    service
        .attach_managed_session(conversation_id.as_str(), session_id.as_str())
        .map_err(LocalApiError::bad_request)?;

    if action == "stream_stop" {
        let stopped = service
            .stop_attached_managed_session_preview_stream(conversation_id.as_str())
            .map_err(LocalApiError::bad_request)?;
        return Ok(Json(json!({
            "success": true,
            "session_id": session_id,
            "workspace_id": workspace_id,
            "status": "active",
            "action": action,
            "stream_stopped": stopped,
        })));
    }

    if action == "stream_frame" {
        let after_sequence = request.after_frame_sequence.unwrap_or(0);
        let frame = service
            .capture_attached_managed_session_preview_frame_after(
                conversation_id.as_str(),
                after_sequence,
            )
            .await
            .map_err(LocalApiError::bad_gateway)?;
        let Some(frame) = frame else {
            return Ok(Json(json!({
                "success": true,
                "session_id": session_id,
                "workspace_id": workspace_id,
                "status": "active",
                "action": action,
                "unchanged": true,
                "after_frame_sequence": after_sequence,
                "frame_data_url": Value::Null,
                "captured_at": crate::local_now_rfc3339(),
            })));
        };
        let frame_data_url = format!(
            "data:{};base64,{}",
            frame.media_type,
            STANDARD.encode(frame.bytes)
        );
        return Ok(Json(json!({
            "success": true,
            "session_id": session_id,
            "workspace_id": workspace_id,
            "status": "active",
            "action": action,
            "frame_data_url": frame_data_url,
            "frame": {
                "media_type": frame.media_type,
                "sequence": frame.sequence,
                "width": frame.width,
                "height": frame.height,
                "page_scale_factor": frame.page_scale_factor,
                "offset_top": frame.offset_top,
                "scroll_offset_x": frame.scroll_offset_x,
                "scroll_offset_y": frame.scroll_offset_y,
                "crop_offset_y": frame.crop_offset_y,
                "timestamp": frame.timestamp,
                "source": frame.source,
            },
            "frame_warning": frame.warning,
            "captured_at": crate::local_now_rfc3339(),
        })));
    }

    if action == "close" {
        let result = service
            .close_attached_managed_session(conversation_id.as_str())
            .await
            .map_err(LocalApiError::bad_gateway)?;
        let success = result
            .get("success")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        return Ok(Json(json!({
            "success": success,
            "session_id": session_id,
            "workspace_id": workspace_id,
            "status": if success { "closed" } else { "error" },
            "result": result,
        })));
    }

    let result = execute_browser_ui_action(
        &service,
        conversation_id.as_str(),
        workspace_root.as_path(),
        action.as_str(),
        &request,
    )?;
    let page = structured_browser_result(result);
    if matches!(action.as_str(), "tab_new" | "tab_switch" | "tab_close") {
        let _ = service.stop_attached_managed_session_preview_stream(conversation_id.as_str());
    }
    if matches!(
        action.as_str(),
        "websocket_start" | "websocket_frames" | "websocket_stop"
    ) {
        return Ok(Json(json!({
            "success": page.get("success").and_then(Value::as_bool).unwrap_or(true),
            "session_id": session_id,
            "workspace_id": workspace_id,
            "status": "active",
            "action": action,
            "page": page,
            "screenshot_data_url": Value::Null,
            "screenshot_error": Value::Null,
            "captured_at": crate::local_now_rfc3339(),
        })));
    }
    let screenshot = service
        .capture_attached_managed_session_screenshot(conversation_id.as_str())
        .await;
    let (screenshot_data_url, screenshot_error) = match screenshot {
        Ok(bytes) => (
            Some(format!("data:image/png;base64,{}", STANDARD.encode(bytes))),
            None,
        ),
        Err(error) => (None, Some(error)),
    };
    Ok(Json(json!({
        "success": page.get("success").and_then(Value::as_bool).unwrap_or(true),
        "session_id": session_id,
        "workspace_id": workspace_id,
        "status": "active",
        "action": action,
        "page": page,
        "screenshot_data_url": screenshot_data_url,
        "screenshot_error": screenshot_error,
        "captured_at": crate::local_now_rfc3339(),
    })))
}

fn execute_browser_ui_action(
    service: &BrowserToolsService,
    conversation_id: &str,
    workspace_root: &Path,
    action: &str,
    request: &LocalBrowserSessionCommandRequest,
) -> Result<Value, LocalApiError> {
    let (tool_name, arguments) = match action {
        "tabs" => ("browser_tabs", json!({})),
        "tab_new" => {
            let url = request
                .url
                .as_deref()
                .map(|value| validate_navigation_url(workspace_root, value))
                .transpose()?;
            ("browser_tab_new", json!({ "url": url }))
        }
        "tab_switch" => (
            "browser_tab_switch",
            json!({
                "tab_id": validate_tab_id(required_optional(
                    request.tab_id.as_deref(),
                    "tab_id",
                )?)?
            }),
        ),
        "tab_close" => (
            "browser_tab_close",
            json!({
                "tab_id": validate_tab_id(required_optional(
                    request.tab_id.as_deref(),
                    "tab_id",
                )?)?
            }),
        ),
        "snapshot" | "refresh" => ("browser_snapshot", json!({ "full": false })),
        "navigate" => (
            "browser_navigate",
            json!({
                "url": validate_navigation_url(
                    workspace_root,
                    required_optional(request.url.as_deref(), "url")?.as_str(),
                )?
            }),
        ),
        "back" => ("browser_back", json!({})),
        "scroll" => {
            let direction =
                required_optional(request.direction.as_deref(), "direction")?.to_ascii_lowercase();
            if !matches!(direction.as_str(), "up" | "down") {
                return Err(LocalApiError::bad_request(
                    "direction must be either up or down",
                ));
            }
            ("browser_scroll", json!({ "direction": direction }))
        }
        "press" => {
            let key = required_optional(request.key.as_deref(), "key")?;
            if key.len() > 64 {
                return Err(LocalApiError::bad_request("key is too long"));
            }
            ("browser_press", json!({ "key": key }))
        }
        "click" => {
            let reference = validate_element_reference(required_optional(
                request.reference.as_deref(),
                "ref",
            )?)?;
            ("browser_click", json!({ "ref": reference }))
        }
        "type" => {
            let reference = validate_element_reference(required_optional(
                request.reference.as_deref(),
                "ref",
            )?)?;
            let text = validate_browser_text(request.text.as_deref())?;
            ("browser_type", json!({ "ref": reference, "text": text }))
        }
        "upload" => {
            let reference = validate_element_reference(required_optional(
                request.reference.as_deref(),
                "ref",
            )?)?;
            let paths = request
                .paths
                .as_ref()
                .ok_or_else(|| LocalApiError::bad_request("paths is required"))?;
            (
                "browser_upload",
                json!({ "ref": reference, "paths": paths }),
            )
        }
        "download" => {
            let reference = validate_element_reference(required_optional(
                request.reference.as_deref(),
                "ref",
            )?)?;
            let path = required_optional(request.path.as_deref(), "path")?;
            (
                "browser_download",
                json!({ "ref": reference, "path": path }),
            )
        }
        "console" => ("browser_console", json!({ "clear": request.clear })),
        "network" => (
            "browser_network",
            json!({
                "clear": request.clear,
                "limit": validate_network_limit(request.limit)?,
                "filter": request.filter.clone(),
                "resource_types": request.resource_types.clone(),
                "method": request.method.clone(),
                "status": request.status.clone(),
            }),
        ),
        "network_request" => (
            "browser_network_request",
            json!({
                "request_id": required_optional(request.request_id.as_deref(), "request_id")?,
                "include_request_body": request.include_request_body,
                "include_response_body": request.include_response_body,
                "max_body_chars": validate_network_body_chars(request.max_body_chars)?,
            }),
        ),
        "har_start" => ("browser_har_start", json!({})),
        "har_stop" => (
            "browser_har_stop",
            json!({
                "path": required_optional(request.path.as_deref(), "path")?,
                "include_request_bodies": request.include_request_bodies,
                "include_response_bodies": request.include_response_bodies,
                "max_body_chars": validate_network_body_chars(request.max_body_chars)?,
                "max_entries": validate_har_entries(request.max_entries)?,
            }),
        ),
        "websocket_start" => ("browser_websocket_start", json!({})),
        "websocket_frames" => (
            "browser_websocket_frames",
            json!({
                "clear": request.clear,
                "limit": validate_websocket_limit(request.limit)?,
                "request_id": request.request_id.clone(),
                "direction": request.direction.clone(),
                "include_text_payloads": request.include_text_payloads,
                "max_payload_chars": validate_websocket_payload_chars(request.max_payload_chars)?,
            }),
        ),
        "websocket_stop" => ("browser_websocket_stop", json!({})),
        _ => {
            return Err(LocalApiError::bad_request(
                "unsupported browser session action",
            ))
        }
    };
    service
        .call_tool(tool_name, arguments, Some(conversation_id))
        .map_err(LocalApiError::bad_gateway)
}

fn structured_browser_result(result: Value) -> Value {
    if let Some(value) = result.get("_structured_result") {
        return value.clone();
    }
    if let Some(text) = result.pointer("/content/0/text").and_then(Value::as_str) {
        return serde_json::from_str(text).unwrap_or_else(|_| json!({ "text": text }));
    }
    result
}

fn validate_navigation_url(workspace_root: &Path, value: &str) -> Result<String, LocalApiError> {
    if value.chars().count() > BROWSER_UI_MAX_URL_CHARS {
        return Err(LocalApiError::bad_request("url is too long"));
    }
    let parsed = url::Url::parse(value)
        .map_err(|_| LocalApiError::bad_request("url must be an absolute URL"))?;
    match parsed.scheme() {
        "http" | "https" => {
            if !parsed.username().is_empty() || parsed.password().is_some() {
                return Err(LocalApiError::bad_request(
                    "url credentials are not allowed",
                ));
            }
            Ok(parsed.to_string())
        }
        "about" if parsed.as_str() == "about:blank" => Ok(parsed.to_string()),
        "file" => {
            let file_path = parsed
                .to_file_path()
                .map_err(|_| LocalApiError::bad_request("file URL is invalid"))?;
            let file_path = file_path
                .canonicalize()
                .map_err(|_| LocalApiError::bad_request("file URL does not exist"))?;
            if !file_path.starts_with(workspace_root) {
                return Err(LocalApiError::bad_request(
                    "file URL must remain inside the selected workspace",
                ));
            }
            url::Url::from_file_path(file_path)
                .map(|url| url.to_string())
                .map_err(|_| LocalApiError::bad_request("file URL is invalid"))
        }
        _ => Err(LocalApiError::bad_request(
            "only http, https, about:blank, and workspace file URLs are allowed",
        )),
    }
}

fn validate_tab_id(value: String) -> Result<String, LocalApiError> {
    let value = value.trim();
    if value.len() < 2
        || value.len() > 32
        || !value.starts_with('t')
        || !value[1..].bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(LocalApiError::bad_request(
            "tab_id must be a stable browser tab ID such as t1",
        ));
    }
    Ok(value.to_string())
}

fn validate_element_reference(value: String) -> Result<String, LocalApiError> {
    let normalized = value.strip_prefix('@').unwrap_or(value.as_str());
    if normalized.is_empty()
        || normalized.len() > 64
        || !normalized
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(LocalApiError::bad_request("element ref is invalid"));
    }
    Ok(format!("@{normalized}"))
}

fn validate_browser_text(value: Option<&str>) -> Result<String, LocalApiError> {
    let value = value.ok_or_else(|| LocalApiError::bad_request("text is required"))?;
    if value.chars().count() > BROWSER_UI_MAX_TEXT_CHARS {
        return Err(LocalApiError::bad_request("text is too long"));
    }
    Ok(value.to_string())
}

fn validate_network_limit(value: Option<usize>) -> Result<usize, LocalApiError> {
    let value = value.unwrap_or(BROWSER_UI_DEFAULT_NETWORK_LIMIT);
    if value == 0 || value > BROWSER_UI_MAX_NETWORK_LIMIT {
        return Err(LocalApiError::bad_request(format!(
            "network limit must be between 1 and {BROWSER_UI_MAX_NETWORK_LIMIT}"
        )));
    }
    Ok(value)
}

fn validate_network_body_chars(value: Option<usize>) -> Result<usize, LocalApiError> {
    let value = value.unwrap_or(BROWSER_UI_DEFAULT_NETWORK_BODY_CHARS);
    if value == 0 || value > BROWSER_UI_MAX_NETWORK_BODY_CHARS {
        return Err(LocalApiError::bad_request(format!(
            "network body limit must be between 1 and {BROWSER_UI_MAX_NETWORK_BODY_CHARS}"
        )));
    }
    Ok(value)
}

fn validate_har_entries(value: Option<usize>) -> Result<usize, LocalApiError> {
    let value = value.unwrap_or(BROWSER_UI_DEFAULT_HAR_ENTRIES);
    if value == 0 || value > BROWSER_UI_MAX_HAR_ENTRIES {
        return Err(LocalApiError::bad_request(format!(
            "HAR entry limit must be between 1 and {BROWSER_UI_MAX_HAR_ENTRIES}"
        )));
    }
    Ok(value)
}

fn validate_websocket_limit(value: Option<usize>) -> Result<usize, LocalApiError> {
    let value = value.unwrap_or(BROWSER_UI_DEFAULT_WEBSOCKET_LIMIT);
    if !(1..=BROWSER_UI_MAX_WEBSOCKET_LIMIT).contains(&value) {
        return Err(LocalApiError::bad_request(
            "WebSocket frame limit is outside the allowed range",
        ));
    }
    Ok(value)
}

fn validate_websocket_payload_chars(value: Option<usize>) -> Result<usize, LocalApiError> {
    let value = value.unwrap_or(BROWSER_UI_DEFAULT_WEBSOCKET_PAYLOAD_CHARS);
    if !(1..=BROWSER_UI_MAX_WEBSOCKET_PAYLOAD_CHARS).contains(&value) {
        return Err(LocalApiError::bad_request(
            "WebSocket payload limit is outside the allowed range",
        ));
    }
    Ok(value)
}

fn normalize_required(value: &str, field: &str) -> Result<String, LocalApiError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(LocalApiError::bad_request(format!("{field} is required")));
    }
    Ok(value.to_string())
}

fn required_optional(value: Option<&str>, field: &str) -> Result<String, LocalApiError> {
    normalize_required(value.unwrap_or_default(), field)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_url_rejects_non_web_and_outside_workspace_files() {
        let workspace = std::env::temp_dir();
        assert!(validate_navigation_url(workspace.as_path(), "javascript:alert(1)").is_err());
        assert!(validate_navigation_url(workspace.as_path(), "https://example.com").is_ok());
        assert!(validate_navigation_url(
            workspace.as_path(),
            "https://user:password@example.com/private"
        )
        .is_err());
    }

    #[test]
    fn browser_tab_id_accepts_only_stable_agent_browser_ids() {
        assert_eq!(validate_tab_id(" t12 ".to_string()).unwrap(), "t12");
        assert!(validate_tab_id("12".to_string()).is_err());
        assert!(validate_tab_id("t1/../../secret".to_string()).is_err());
    }

    #[test]
    fn element_reference_is_normalized_and_restricted() {
        assert_eq!(
            validate_element_reference("e12".to_string()).unwrap(),
            "@e12"
        );
        assert!(validate_element_reference("../../secret".to_string()).is_err());
    }

    #[test]
    fn browser_text_input_preserves_whitespace_and_allows_empty_values() {
        assert_eq!(
            validate_browser_text(Some("  keep whitespace  ")).unwrap(),
            "  keep whitespace  "
        );
        assert_eq!(validate_browser_text(Some("")).unwrap(), "");
        assert!(validate_browser_text(None).is_err());
    }

    #[test]
    fn browser_network_limit_is_bounded_and_defaults_safely() {
        assert_eq!(
            validate_network_limit(None).unwrap(),
            BROWSER_UI_DEFAULT_NETWORK_LIMIT
        );
        assert_eq!(validate_network_limit(Some(1)).unwrap(), 1);
        assert_eq!(
            validate_network_limit(Some(BROWSER_UI_MAX_NETWORK_LIMIT)).unwrap(),
            BROWSER_UI_MAX_NETWORK_LIMIT
        );
        assert!(validate_network_limit(Some(0)).is_err());
        assert!(validate_network_limit(Some(BROWSER_UI_MAX_NETWORK_LIMIT + 1)).is_err());
    }

    #[test]
    fn browser_network_body_limit_is_bounded_and_defaults_safely() {
        assert_eq!(
            validate_network_body_chars(None).unwrap(),
            BROWSER_UI_DEFAULT_NETWORK_BODY_CHARS
        );
        assert_eq!(validate_network_body_chars(Some(1)).unwrap(), 1);
        assert_eq!(
            validate_network_body_chars(Some(BROWSER_UI_MAX_NETWORK_BODY_CHARS)).unwrap(),
            BROWSER_UI_MAX_NETWORK_BODY_CHARS
        );
        assert!(validate_network_body_chars(Some(0)).is_err());
        assert!(validate_network_body_chars(Some(BROWSER_UI_MAX_NETWORK_BODY_CHARS + 1)).is_err());
    }

    #[test]
    fn browser_har_entry_limit_is_bounded_and_defaults_safely() {
        assert_eq!(
            validate_har_entries(None).unwrap(),
            BROWSER_UI_DEFAULT_HAR_ENTRIES
        );
        assert_eq!(validate_har_entries(Some(1)).unwrap(), 1);
        assert_eq!(
            validate_har_entries(Some(BROWSER_UI_MAX_HAR_ENTRIES)).unwrap(),
            BROWSER_UI_MAX_HAR_ENTRIES
        );
        assert!(validate_har_entries(Some(0)).is_err());
        assert!(validate_har_entries(Some(BROWSER_UI_MAX_HAR_ENTRIES + 1)).is_err());
    }

    #[test]
    fn browser_websocket_limits_are_bounded_and_default_safely() {
        assert_eq!(
            validate_websocket_limit(None).unwrap(),
            BROWSER_UI_DEFAULT_WEBSOCKET_LIMIT
        );
        assert_eq!(validate_websocket_limit(Some(1)).unwrap(), 1);
        assert!(validate_websocket_limit(Some(0)).is_err());
        assert!(validate_websocket_limit(Some(BROWSER_UI_MAX_WEBSOCKET_LIMIT + 1)).is_err());
        assert_eq!(
            validate_websocket_payload_chars(None).unwrap(),
            BROWSER_UI_DEFAULT_WEBSOCKET_PAYLOAD_CHARS
        );
        assert_eq!(validate_websocket_payload_chars(Some(1)).unwrap(), 1);
        assert!(validate_websocket_payload_chars(Some(0)).is_err());
        assert!(
            validate_websocket_payload_chars(Some(BROWSER_UI_MAX_WEBSOCKET_PAYLOAD_CHARS + 1))
                .is_err()
        );
    }
}
