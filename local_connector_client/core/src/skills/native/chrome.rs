// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use url::Url;
use uuid::Uuid;

use crate::chrome_bridge::{
    execute_chrome_extension_command, execute_chrome_extension_command_cancellable,
    execute_chrome_extension_command_cancellable_with_timeout, CHROME_EXTENSION_VERSION,
};
use crate::chrome_integration::chrome_integration_status;
use crate::relay::RelayRequest;
use crate::workspace::paths::{
    canonicalize_existing_dir, normalize_request_workspace_relative_path, workspace_for_request,
};
use crate::LocalState;

use super::required_text;

const DEFAULT_TAB_LIMIT: usize = 20;
const MAX_TAB_LIMIT: usize = 50;
const DEFAULT_SNAPSHOT_CHARS: usize = 20_000;
const MIN_SNAPSHOT_CHARS: usize = 1_000;
const MAX_SNAPSHOT_CHARS: usize = 50_000;
const MIN_SCREENSHOT_QUALITY: u64 = 40;
const MAX_SCREENSHOT_QUALITY: u64 = 85;
const DEFAULT_SCREENSHOT_QUALITY: u64 = 65;
const MAX_SCREENSHOT_BYTES: usize = 700 * 1024;
const MAX_TEXT_CHARS: usize = 2_000;
const MAX_UPLOAD_BYTES: u64 = 10 * 1024 * 1024;
const UPLOAD_CHUNK_BYTES: usize = 192 * 1024;
const MAX_UPLOAD_CHUNKS: usize = 64;
const MAX_UPLOAD_PATH_CHARS: usize = 4_096;
const MAX_DOWNLOAD_BYTES: u64 = 10 * 1024 * 1024;
const DOWNLOAD_CHUNK_BYTES: usize = 192 * 1024;
const MAX_DOWNLOAD_CHUNKS: usize = 64;
const DEFAULT_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(45);
const DOWNLOAD_ABORT_TIMEOUT: Duration = Duration::from_secs(2);
const DOWNLOAD_STAGING_PREFIX: &str = ".chatos-chrome-download-";
const MAX_OPTION_LABEL_CHARS: usize = 240;
const MAX_SCROLL_DELTA: i64 = 2_000;

pub(super) fn tool_definitions(include_sensitive_reads: bool) -> Vec<Value> {
    let mut tools = vec![chrome_status_tool()];
    if include_sensitive_reads {
        tools.extend([
            chrome_tabs_tool(),
            chrome_tab_snapshot_tool(),
            chrome_tab_navigate_tool(),
            chrome_tab_click_tool(),
            chrome_tab_type_text_tool(),
            chrome_tab_select_tool(),
            chrome_tab_scroll_tool(),
            chrome_tab_history_tool(),
            chrome_tab_activate_tool(),
            chrome_tab_upload_tool(),
            chrome_tab_download_tool(),
            chrome_tab_screenshot_tool(),
            chrome_tab_release_tool(),
        ]);
    }
    tools
}

pub(super) fn dependency_error() -> Option<String> {
    (std::env::consts::OS != "macos").then(|| {
        "Chrome existing-session integration is currently available on macOS only".to_string()
    })
}

pub(super) fn requires_interactive_approval(operation: &str) -> bool {
    matches!(
        operation,
        "chrome_tabs"
            | "chrome_tab_snapshot"
            | "chrome_tab_navigate"
            | "chrome_tab_click"
            | "chrome_tab_type_text"
            | "chrome_tab_select"
            | "chrome_tab_scroll"
            | "chrome_tab_history"
            | "chrome_tab_activate"
            | "chrome_tab_upload"
            | "chrome_tab_download"
            | "chrome_tab_screenshot"
            | "chrome_tab_release"
    )
}

pub(super) fn approval_command(
    operation: &str,
    arguments: &Value,
) -> Result<(String, Vec<String>)> {
    let args = match operation {
        "chrome_tabs" => vec![
            "tabs".to_string(),
            format!("limit={}", tab_limit(arguments)?),
        ],
        "chrome_tab_snapshot" => vec![
            "snapshot".to_string(),
            format!("tab_id={}", chrome_tab_id(arguments)?),
            format!("max_chars={}", snapshot_chars(arguments)?),
        ],
        "chrome_tab_navigate" => vec![
            "navigate".to_string(),
            format!("tab_id={}", chrome_tab_id(arguments)?),
            format!("url={}", navigation_url(arguments)?),
        ],
        "chrome_tab_click" => vec![
            "click".to_string(),
            format!("tab_id={}", chrome_tab_id(arguments)?),
            format!("target_id={}", chrome_target_id(arguments)?),
        ],
        "chrome_tab_type_text" => {
            let text = chrome_text(arguments)?;
            vec![
                "type_text".to_string(),
                format!("tab_id={}", chrome_tab_id(arguments)?),
                format!("target_id={}", chrome_target_id(arguments)?),
                format!("text_chars={}", text.chars().count()),
                format!(
                    "text_sha256={}",
                    hex::encode(Sha256::digest(text.as_bytes()))
                ),
                format!("replace={}", replace_text(arguments)),
            ]
        }
        "chrome_tab_select" => vec![
            "select_option".to_string(),
            format!("tab_id={}", chrome_tab_id(arguments)?),
            format!("target_id={}", chrome_target_id(arguments)?),
            format!("option_label={}", option_label(arguments)?),
        ],
        "chrome_tab_scroll" => {
            let (delta_x, delta_y) = scroll_deltas(arguments)?;
            vec![
                "scroll".to_string(),
                format!("tab_id={}", chrome_tab_id(arguments)?),
                format!("delta_x={delta_x}"),
                format!("delta_y={delta_y}"),
            ]
        }
        "chrome_tab_history" => vec![
            "history".to_string(),
            format!("tab_id={}", chrome_tab_id(arguments)?),
            format!("direction={}", history_direction(arguments)?),
        ],
        "chrome_tab_activate" => vec![
            "activate".to_string(),
            format!("tab_id={}", chrome_tab_id(arguments)?),
        ],
        "chrome_tab_upload" => vec![
            "upload".to_string(),
            format!("tab_id={}", chrome_tab_id(arguments)?),
            format!("target_id={}", chrome_target_id(arguments)?),
            format!("path={}", upload_path_argument(arguments)?),
        ],
        "chrome_tab_download" => vec![
            "download".to_string(),
            format!("tab_id={}", chrome_tab_id(arguments)?),
            format!("target_id={}", chrome_target_id(arguments)?),
            format!("path={}", download_path_argument(arguments)?),
            format!("max_bytes={}", download_max_bytes(arguments)?),
        ],
        "chrome_tab_screenshot" => vec![
            "screenshot".to_string(),
            format!("tab_id={}", chrome_tab_id(arguments)?),
            format!("quality={}", screenshot_quality(arguments)?),
        ],
        "chrome_tab_release" => vec![
            "release_tab".to_string(),
            format!("tab_id={}", chrome_tab_id(arguments)?),
        ],
        _ => bail!("Chrome operation does not support interactive approval"),
    };
    Ok(("chatos-chrome".to_string(), args))
}

pub(super) fn execute(
    operation: &str,
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    approved_command_args: Option<&[String]>,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    if requires_interactive_approval(operation) {
        let (_, expected) = approval_command(operation, arguments)?;
        if approved_command_args != Some(expected.as_slice()) {
            bail!("approved Chrome action no longer matches the exact reviewed arguments");
        }
    }
    match operation {
        "chrome_status" => chrome_status(),
        "chrome_tabs" => chrome_tabs(arguments),
        "chrome_tab_snapshot" => chrome_tab_snapshot(arguments),
        "chrome_tab_navigate" => chrome_tab_navigate(arguments, action_cancelled),
        "chrome_tab_click" => chrome_tab_click(arguments, action_cancelled),
        "chrome_tab_type_text" => chrome_tab_type_text(arguments, action_cancelled),
        "chrome_tab_select" => chrome_tab_select(arguments, action_cancelled),
        "chrome_tab_scroll" => chrome_tab_scroll(arguments, action_cancelled),
        "chrome_tab_history" => chrome_tab_history(arguments, action_cancelled),
        "chrome_tab_activate" => chrome_tab_activate(arguments, action_cancelled),
        "chrome_tab_upload" => chrome_tab_upload(arguments, state, request, action_cancelled),
        "chrome_tab_download" => chrome_tab_download(arguments, state, request, action_cancelled),
        "chrome_tab_screenshot" => chrome_tab_screenshot(arguments, action_cancelled),
        "chrome_tab_release" => chrome_tab_release(arguments),
        _ => bail!("Chrome operation is not implemented: {operation}"),
    }
}

fn chrome_status() -> Result<Value> {
    let status = chrome_integration_status();
    Ok(json!({
        "platform_supported": status.platform_supported,
        "native_host_registered": status.enabled,
        "native_host_available": status.native_host_available,
        "extension_available": status.extension_available,
        "extension_id": status.extension_id,
        "connected": status.bridge.connected,
        "extension_version": status.bridge.extension_version,
        "expected_extension_version": CHROME_EXTENSION_VERSION,
        "extension_compatible": status.bridge.extension_compatible,
        "claimed_tab_count": status.bridge.claimed_tab_count,
        "authorized_origin_count": status.bridge.authorized_origin_count,
        "setup_required": !(status.enabled && status.bridge.connected && status.bridge.extension_compatible),
        "setup_note": status.setup_note,
    }))
}

fn chrome_tabs(arguments: &Value) -> Result<Value> {
    let limit = tab_limit(arguments)?;
    let result = execute_chrome_extension_command("tabs", json!({"limit": limit}))?;
    let tabs = result
        .get("tabs")
        .and_then(Value::as_array)
        .context("Chrome extension tabs response is malformed")?;
    if tabs.len() > limit || tabs.len() > MAX_TAB_LIMIT {
        bail!("Chrome extension returned too many tabs");
    }
    let tabs = tabs.iter().map(normalize_tab).collect::<Result<Vec<_>>>()?;
    Ok(json!({
        "tabs": tabs,
        "count": tabs.len(),
        "authorization": "Only tabs explicitly connected from the ChatOS Chrome extension are returned.",
    }))
}

fn chrome_tab_snapshot(arguments: &Value) -> Result<Value> {
    let tab_id = chrome_tab_id(arguments)?;
    let max_chars = snapshot_chars(arguments)?;
    let result = execute_chrome_extension_command(
        "snapshot",
        json!({"tab_id": tab_id, "max_chars": max_chars}),
    )?;
    let tab = normalize_tab(
        result
            .get("tab")
            .context("Chrome extension snapshot is missing tab metadata")?,
    )?;
    if tab.get("tab_id").and_then(Value::as_str) != Some(tab_id.as_str()) {
        bail!("Chrome extension snapshot tab identity changed");
    }
    let snapshot = result
        .get("snapshot")
        .and_then(Value::as_str)
        .context("Chrome extension snapshot text is malformed")?;
    if snapshot.len() > max_chars || snapshot.len() > MAX_SNAPSHOT_CHARS {
        bail!("Chrome extension snapshot exceeded the requested character limit");
    }
    let snapshot = normalize_snapshot_text(snapshot)?;
    Ok(json!({
        "tab": tab,
        "snapshot": snapshot,
        "truncated": result.get("truncated").and_then(Value::as_bool).unwrap_or(false),
        "target_count": result.get("target_count").and_then(Value::as_u64).unwrap_or(0),
        "captured_at": bounded_optional_text(result.get("captured_at"), 64)?,
        "sensitive_content_notice": "The snapshot may contain signed-in page content and was returned only after local approval.",
        "target_note": "Action targets are short-lived. Capture a fresh snapshot after navigation, click, text input, selection, scroll, history movement or upload.",
    }))
}

fn chrome_tab_navigate(arguments: &Value, action_cancelled: Option<&AtomicBool>) -> Result<Value> {
    let tab_id = chrome_tab_id(arguments)?;
    let url = navigation_url(arguments)?;
    let result = execute_chrome_extension_command_cancellable(
        "navigate",
        json!({"tab_id": tab_id, "url": url}),
        action_cancelled,
    )?;
    let tab = normalize_tab(
        result
            .get("tab")
            .context("Chrome extension navigation response is missing tab metadata")?,
    )?;
    Ok(json!({
        "success": result.get("navigated").and_then(Value::as_bool).unwrap_or(false),
        "tab": tab,
        "scope": "same_authorized_origin",
        "snapshot_required": true,
        "cancellation_note": "Cancellation stops bounded waiting but cannot reverse a navigation already accepted by Chrome.",
    }))
}

fn chrome_tab_click(arguments: &Value, action_cancelled: Option<&AtomicBool>) -> Result<Value> {
    let tab_id = chrome_tab_id(arguments)?;
    let target_id = chrome_target_id(arguments)?;
    let result = execute_chrome_extension_command_cancellable(
        "click",
        json!({"tab_id": tab_id, "target_id": target_id}),
        action_cancelled,
    )?;
    Ok(json!({
        "success": result.get("clicked").and_then(Value::as_bool).unwrap_or(false),
        "tab_id": tab_id,
        "target_id": target_id,
        "target_kind": bounded_optional_text(result.get("target_kind"), 64)?,
        "snapshot_required": true,
        "cancellation_note": "Cancellation prevents undispatched work but cannot undo a click already delivered to the page.",
    }))
}

fn chrome_tab_type_text(arguments: &Value, action_cancelled: Option<&AtomicBool>) -> Result<Value> {
    let tab_id = chrome_tab_id(arguments)?;
    let target_id = chrome_target_id(arguments)?;
    let text = chrome_text(arguments)?;
    let replace = replace_text(arguments);
    let text_chars = text.chars().count();
    let text_sha256 = hex::encode(Sha256::digest(text.as_bytes()));
    let result = execute_chrome_extension_command_cancellable(
        "type_text",
        json!({
            "tab_id": tab_id,
            "target_id": target_id,
            "text": text,
            "replace": replace,
        }),
        action_cancelled,
    )?;
    Ok(json!({
        "success": result.get("typed").and_then(Value::as_bool).unwrap_or(false),
        "tab_id": tab_id,
        "target_id": target_id,
        "target_kind": bounded_optional_text(result.get("target_kind"), 64)?,
        "character_count": text_chars,
        "text_sha256": text_sha256,
        "replace": replace,
        "text_persisted_in_result": false,
        "snapshot_required": true,
    }))
}

fn chrome_tab_select(arguments: &Value, action_cancelled: Option<&AtomicBool>) -> Result<Value> {
    let tab_id = chrome_tab_id(arguments)?;
    let target_id = chrome_target_id(arguments)?;
    let option_label = option_label(arguments)?;
    let result = execute_chrome_extension_command_cancellable(
        "select_option",
        json!({
            "tab_id": tab_id,
            "target_id": target_id,
            "option_label": option_label,
        }),
        action_cancelled,
    )?;
    let returned_label = bounded_required_text(result.get("option_label"), 1_024)?;
    if returned_label != option_label {
        bail!("Chrome extension selected a different option label");
    }
    let option_index = result
        .get("option_index")
        .and_then(Value::as_u64)
        .filter(|value| *value <= 10_000)
        .context("Chrome extension option index is invalid")?;
    Ok(json!({
        "success": result.get("selected").and_then(Value::as_bool).unwrap_or(false),
        "tab_id": tab_id,
        "target_id": target_id,
        "target_kind": bounded_optional_text(result.get("target_kind"), 64)?,
        "option_label": returned_label,
        "option_index": option_index,
        "snapshot_required": true,
    }))
}

fn chrome_tab_scroll(arguments: &Value, action_cancelled: Option<&AtomicBool>) -> Result<Value> {
    let tab_id = chrome_tab_id(arguments)?;
    let (delta_x, delta_y) = scroll_deltas(arguments)?;
    let result = execute_chrome_extension_command_cancellable(
        "scroll",
        json!({"tab_id": tab_id, "delta_x": delta_x, "delta_y": delta_y}),
        action_cancelled,
    )?;
    Ok(json!({
        "success": result.get("scrolled").and_then(Value::as_bool).unwrap_or(false),
        "tab_id": tab_id,
        "delta_x": delta_x,
        "delta_y": delta_y,
        "scroll_x": bounded_page_metric(&result, "scroll_x")?,
        "scroll_y": bounded_page_metric(&result, "scroll_y")?,
        "viewport_width": bounded_page_metric(&result, "viewport_width")?,
        "viewport_height": bounded_page_metric(&result, "viewport_height")?,
        "snapshot_required": true,
    }))
}

fn chrome_tab_history(arguments: &Value, action_cancelled: Option<&AtomicBool>) -> Result<Value> {
    let tab_id = chrome_tab_id(arguments)?;
    let direction = history_direction(arguments)?;
    let result = execute_chrome_extension_command_cancellable(
        "history",
        json!({"tab_id": tab_id, "direction": direction}),
        action_cancelled,
    )?;
    let tab = normalize_tab(
        result
            .get("tab")
            .context("Chrome extension history response is missing tab metadata")?,
    )?;
    Ok(json!({
        "success": result.get("moved").and_then(Value::as_bool).unwrap_or(false),
        "direction": direction,
        "tab": tab,
        "snapshot_required": true,
        "origin_note": "If browser history leaves the authorized origin, the tab claim is released and this action fails closed.",
    }))
}

fn chrome_tab_activate(arguments: &Value, action_cancelled: Option<&AtomicBool>) -> Result<Value> {
    let tab_id = chrome_tab_id(arguments)?;
    let result = execute_chrome_extension_command_cancellable(
        "activate",
        json!({"tab_id": tab_id}),
        action_cancelled,
    )?;
    let tab = normalize_tab(
        result
            .get("tab")
            .context("Chrome extension activation response is missing tab metadata")?,
    )?;
    if tab.get("tab_id").and_then(Value::as_str) != Some(tab_id.as_str())
        || tab.get("active").and_then(Value::as_bool) != Some(true)
    {
        bail!("Chrome extension activated a different tab");
    }
    Ok(json!({
        "success": result.get("activated").and_then(Value::as_bool).unwrap_or(false),
        "tab": tab,
        "window_focused": false,
        "note": "The tab was activated inside its existing window; ChatOS did not force the Chrome window to the foreground.",
    }))
}

fn chrome_tab_upload(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    let tab_id = chrome_tab_id(arguments)?;
    let target_id = chrome_target_id(arguments)?;
    let file = resolve_upload_file(state, request, upload_path_argument(arguments)?)?;
    let upload_id = Uuid::new_v4().to_string();
    let chunk_count = file.bytes.len().div_ceil(UPLOAD_CHUNK_BYTES);
    if chunk_count == 0 || chunk_count > MAX_UPLOAD_CHUNKS {
        bail!("Chrome upload requires too many Native Messaging chunks");
    }
    ensure_not_cancelled(action_cancelled)?;
    execute_chrome_extension_command_cancellable(
        "upload_begin",
        json!({
            "tab_id": tab_id,
            "target_id": target_id,
            "upload_id": upload_id,
            "filename": file.filename,
            "size_bytes": file.bytes.len(),
            "chunk_count": chunk_count,
            "sha256": file.sha256,
            "mime_type": file.mime_type,
        }),
        action_cancelled,
    )?;

    let upload_result = (|| -> Result<Value> {
        for (chunk_index, chunk) in file.bytes.chunks(UPLOAD_CHUNK_BYTES).enumerate() {
            ensure_not_cancelled(action_cancelled)?;
            let accepted = execute_chrome_extension_command_cancellable(
                "upload_chunk",
                json!({
                    "tab_id": tab_id,
                    "target_id": target_id,
                    "upload_id": upload_id,
                    "chunk_index": chunk_index,
                    "data_base64": STANDARD.encode(chunk),
                }),
                action_cancelled,
            )?;
            if accepted.get("accepted_chunk_index").and_then(Value::as_u64)
                != u64::try_from(chunk_index).ok()
            {
                bail!("Chrome extension acknowledged an unexpected upload chunk");
            }
        }
        ensure_not_cancelled(action_cancelled)?;
        execute_chrome_extension_command_cancellable(
            "upload_finish",
            json!({
                "tab_id": tab_id,
                "target_id": target_id,
                "upload_id": upload_id,
            }),
            action_cancelled,
        )
    })();

    let result = match upload_result {
        Ok(result) => result,
        Err(error) => {
            let _ = execute_chrome_extension_command(
                "upload_abort",
                json!({"tab_id": tab_id, "upload_id": upload_id}),
            );
            return Err(error);
        }
    };
    if result.get("sha256").and_then(Value::as_str) != Some(file.sha256.as_str())
        || result.get("size_bytes").and_then(Value::as_u64) != u64::try_from(file.bytes.len()).ok()
    {
        bail!("Chrome extension upload verification did not match the workspace file");
    }
    Ok(json!({
        "success": result.get("uploaded").and_then(Value::as_bool).unwrap_or(false),
        "tab_id": tab_id,
        "target_id": target_id,
        "path": file.relative,
        "filename": file.filename,
        "size_bytes": file.bytes.len(),
        "sha256": file.sha256,
        "chunk_count": chunk_count,
        "source_scope": "authorized_workspace_regular_file",
        "snapshot_required": true,
    }))
}

fn chrome_tab_download(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    let tab_id = chrome_tab_id(arguments)?;
    let target_id = chrome_target_id(arguments)?;
    let max_bytes = download_max_bytes(arguments)?;
    let destination =
        resolve_download_destination(state, request, download_path_argument(arguments)?)?;
    let mut staging = ChromeDownloadStaging::create(&destination)?;
    let download_id = Uuid::new_v4().to_string();

    let download_result = (|| -> Result<Value> {
        ensure_not_cancelled(action_cancelled)?;
        let begin = execute_chrome_extension_command_cancellable_with_timeout(
            "download_begin",
            json!({
                "tab_id": tab_id,
                "target_id": target_id,
                "download_id": download_id,
                "max_bytes": max_bytes,
            }),
            DEFAULT_DOWNLOAD_TIMEOUT,
            action_cancelled,
        )?;
        if begin.get("download_id").and_then(Value::as_str) != Some(download_id.as_str())
            || begin.get("ready").and_then(Value::as_bool) != Some(true)
        {
            bail!("Chrome extension returned an invalid download session");
        }
        let size_bytes = begin
            .get("size_bytes")
            .and_then(Value::as_u64)
            .filter(|size| (1..=max_bytes).contains(size))
            .context("Chrome extension returned an invalid download size")?;
        let expected_sha256 = download_sha256(begin.get("sha256"))?;
        let expected_chunk_count = usize::try_from(size_bytes)
            .ok()
            .map(|size| size.div_ceil(DOWNLOAD_CHUNK_BYTES))
            .filter(|count| (1..=MAX_DOWNLOAD_CHUNKS).contains(count))
            .context("Chrome download requires too many Native Messaging chunks")?;
        if begin.get("chunk_count").and_then(Value::as_u64)
            != u64::try_from(expected_chunk_count).ok()
        {
            bail!("Chrome extension returned an invalid download chunk count");
        }
        let source_kind = download_source_kind(begin.get("source_kind"))?;
        let source_url = download_source_url(&begin, source_kind)?;
        let mime_type = download_mime_type(begin.get("mime_type"))?;

        let mut received_bytes = 0_u64;
        let mut hasher = Sha256::new();
        for chunk_index in 0..expected_chunk_count {
            ensure_not_cancelled(action_cancelled)?;
            let chunk_result = execute_chrome_extension_command_cancellable(
                "download_chunk",
                json!({
                    "tab_id": tab_id,
                    "target_id": target_id,
                    "download_id": download_id,
                    "chunk_index": chunk_index,
                }),
                action_cancelled,
            )?;
            if chunk_result.get("download_id").and_then(Value::as_str) != Some(download_id.as_str())
                || chunk_result.get("chunk_index").and_then(Value::as_u64)
                    != u64::try_from(chunk_index).ok()
            {
                bail!("Chrome extension returned an unexpected download chunk");
            }
            let encoded = chunk_result
                .get("data_base64")
                .and_then(Value::as_str)
                .context("Chrome extension download chunk is missing data")?;
            let chunk = STANDARD
                .decode(encoded)
                .context("decode Chrome extension download chunk")?;
            let remaining = size_bytes
                .checked_sub(received_bytes)
                .context("Chrome extension download exceeded the declared size")?;
            let expected_chunk_bytes = remaining.min(DOWNLOAD_CHUNK_BYTES as u64);
            if u64::try_from(chunk.len()).ok() != Some(expected_chunk_bytes)
                || chunk_result.get("size_bytes").and_then(Value::as_u64)
                    != Some(expected_chunk_bytes)
            {
                bail!("Chrome extension download chunk size verification failed");
            }
            received_bytes = received_bytes
                .checked_add(expected_chunk_bytes)
                .filter(|total| *total <= size_bytes && *total <= max_bytes)
                .context("Chrome extension download exceeded the approved byte limit")?;
            staging
                .file_mut()
                .write_all(chunk.as_slice())
                .context("write Chrome download staging file")?;
            hasher.update(chunk.as_slice());
        }
        if received_bytes != size_bytes {
            bail!("Chrome extension download ended before the declared size");
        }
        let actual_sha256 = hex::encode(hasher.finalize());
        if actual_sha256 != expected_sha256 {
            bail!("Chrome extension download SHA-256 verification failed");
        }
        staging.sync()?;
        ensure_not_cancelled(action_cancelled)?;
        let finish = execute_chrome_extension_command_cancellable(
            "download_finish",
            json!({
                "tab_id": tab_id,
                "target_id": target_id,
                "download_id": download_id,
            }),
            action_cancelled,
        )?;
        if finish.get("download_id").and_then(Value::as_str) != Some(download_id.as_str())
            || finish.get("released").and_then(Value::as_bool) != Some(true)
        {
            bail!("Chrome extension did not release the verified download session");
        }
        ensure_not_cancelled(action_cancelled)?;
        staging.commit(&destination)?;

        Ok(json!({
            "success": true,
            "tab_id": tab_id,
            "target_id": target_id,
            "path": destination.relative,
            "size_bytes": size_bytes,
            "sha256": actual_sha256,
            "chunk_count": expected_chunk_count,
            "mime_type": mime_type,
            "source_kind": source_kind,
            "source_url": source_url,
            "destination_scope": "authorized_workspace_new_file",
            "overwritten": false,
        }))
    })();

    match download_result {
        Ok(result) => Ok(result),
        Err(error) => {
            let _ = execute_chrome_extension_command_cancellable_with_timeout(
                "download_abort",
                json!({"tab_id": tab_id, "download_id": download_id}),
                DOWNLOAD_ABORT_TIMEOUT,
                None,
            );
            Err(error)
        }
    }
}

fn chrome_tab_screenshot(
    arguments: &Value,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    let tab_id = chrome_tab_id(arguments)?;
    let quality = screenshot_quality(arguments)?;
    let result = execute_chrome_extension_command_cancellable(
        "screenshot",
        json!({"tab_id": tab_id, "quality": quality}),
        action_cancelled,
    )?;
    let data_url = result
        .get("data_url")
        .and_then(Value::as_str)
        .context("Chrome extension screenshot is missing image data")?;
    let encoded = data_url
        .strip_prefix("data:image/jpeg;base64,")
        .context("Chrome extension screenshot is not a JPEG data URL")?;
    let bytes = STANDARD
        .decode(encoded)
        .context("decode Chrome extension screenshot")?;
    if bytes.is_empty()
        || bytes.len() > MAX_SCREENSHOT_BYTES
        || !bytes.starts_with(&[0xff, 0xd8, 0xff])
    {
        bail!("Chrome extension screenshot is not a valid bounded JPEG");
    }
    if result.get("size_bytes").and_then(Value::as_u64) != u64::try_from(bytes.len()).ok() {
        bail!("Chrome extension screenshot size verification failed");
    }
    let tab = normalize_tab(
        result
            .get("tab")
            .context("Chrome extension screenshot is missing tab metadata")?,
    )?;
    let sha256 = hex::encode(Sha256::digest(bytes.as_slice()));
    Ok(json!({
        "text": "Captured the visible viewport of the active, explicitly connected Chrome tab and attached it as transient image input for the next model step.",
        "_structured_result": {
            "success": true,
            "tab": tab,
            "mime_type": "image/jpeg",
            "quality": quality,
            "size_bytes": bytes.len(),
            "sha256": sha256,
            "persisted": false,
            "sensitive_content_possible": true,
        },
        "_model_input": [{
            "type": "input_image",
            "image_url": data_url,
            "detail": "high",
        }],
    }))
}

fn chrome_tab_release(arguments: &Value) -> Result<Value> {
    let tab_id = chrome_tab_id(arguments)?;
    let result = execute_chrome_extension_command("release_tab", json!({"tab_id": tab_id}))?;
    let released = result
        .get("released")
        .and_then(Value::as_bool)
        .context("Chrome extension release response is malformed")?;
    Ok(json!({
        "tab_id": tab_id,
        "released": released,
        "site_permission_revoked": false,
        "note": "Releasing a tab stops ChatOS access to that tab but does not revoke the user-managed site permission.",
    }))
}

fn normalize_tab(value: &Value) -> Result<Value> {
    let object = value
        .as_object()
        .context("Chrome extension tab metadata must be an object")?;
    let tab_id = object
        .get("tab_id")
        .and_then(Value::as_str)
        .filter(|value| valid_prefixed_numeric_id(value, "ct"))
        .context("Chrome extension tab ID is invalid")?;
    let window_id = object
        .get("window_id")
        .and_then(Value::as_str)
        .filter(|value| valid_prefixed_numeric_id(value, "cw"))
        .context("Chrome extension window ID is invalid")?;
    let raw_url = object
        .get("url")
        .and_then(Value::as_str)
        .context("Chrome extension tab URL is missing")?;
    let url = sanitize_page_url(raw_url)?;
    let title = bounded_optional_text(object.get("title"), 512)?;
    Ok(json!({
        "tab_id": tab_id,
        "window_id": window_id,
        "active": object.get("active").and_then(Value::as_bool).unwrap_or(false),
        "pinned": object.get("pinned").and_then(Value::as_bool).unwrap_or(false),
        "incognito": object.get("incognito").and_then(Value::as_bool).unwrap_or(false),
        "title": title,
        "url": url,
    }))
}

fn sanitize_page_url(value: &str) -> Result<String> {
    if value.len() > 8_192 || value.chars().any(char::is_control) {
        bail!("Chrome extension tab URL is invalid");
    }
    let mut url = Url::parse(value).context("parse Chrome extension tab URL")?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.host_str().is_none()
    {
        bail!("Chrome extension tab URL is outside the allowed HTTP(S) scope");
    }
    let query_keys = url
        .query_pairs()
        .map(|(key, _)| key.into_owned())
        .take(100)
        .collect::<Vec<_>>();
    if query_keys.is_empty() {
        url.set_query(None);
    } else {
        let mut pairs = url.query_pairs_mut();
        pairs.clear();
        for key in query_keys {
            pairs.append_pair(key.as_str(), "[REDACTED]");
        }
    }
    url.set_fragment(None);
    Ok(url.to_string())
}

fn normalize_snapshot_text(value: &str) -> Result<String> {
    if value
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        bail!("Chrome extension snapshot contains unsupported control characters");
    }
    Ok(value.replace("\r\n", "\n").replace('\r', "\n"))
}

fn tab_limit(arguments: &Value) -> Result<usize> {
    let limit = arguments
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_TAB_LIMIT as u64);
    usize::try_from(limit)
        .ok()
        .filter(|value| (1..=MAX_TAB_LIMIT).contains(value))
        .ok_or_else(|| anyhow!("limit must be between 1 and {MAX_TAB_LIMIT}"))
}

fn snapshot_chars(arguments: &Value) -> Result<usize> {
    let limit = arguments
        .get("max_chars")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_SNAPSHOT_CHARS as u64);
    usize::try_from(limit)
        .ok()
        .filter(|value| (MIN_SNAPSHOT_CHARS..=MAX_SNAPSHOT_CHARS).contains(value))
        .ok_or_else(|| {
            anyhow!("max_chars must be between {MIN_SNAPSHOT_CHARS} and {MAX_SNAPSHOT_CHARS}")
        })
}

fn chrome_tab_id(arguments: &Value) -> Result<String> {
    let value = required_text(arguments, "tab_id")?;
    if !valid_prefixed_numeric_id(value, "ct") {
        bail!("tab_id must be a stable Chrome tab ID such as ct123");
    }
    Ok(value.to_string())
}

fn chrome_target_id(arguments: &Value) -> Result<String> {
    let value = required_text(arguments, "target_id")?;
    let Some((snapshot, ordinal)) = value
        .strip_prefix("cr")
        .and_then(|value| value.split_once('-'))
    else {
        bail!("target_id must come from the latest Chrome tab snapshot");
    };
    if snapshot.len() != 16
        || !snapshot.bytes().all(|byte| byte.is_ascii_hexdigit())
        || ordinal.is_empty()
        || ordinal.len() > 4
        || ordinal.starts_with('0')
        || !ordinal.bytes().all(|byte| byte.is_ascii_digit())
    {
        bail!("target_id must come from the latest Chrome tab snapshot");
    }
    Ok(value.to_ascii_lowercase())
}

fn navigation_url(arguments: &Value) -> Result<String> {
    let value = required_text(arguments, "url")?;
    if value.len() > 8_192 || value.chars().any(char::is_control) {
        bail!("url is empty, too long, or contains control characters");
    }
    let url = Url::parse(value).context("parse Chrome navigation URL")?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.host_str().is_none()
    {
        bail!("Chrome navigation URL must be HTTP(S) without embedded credentials");
    }
    Ok(url.to_string())
}

fn chrome_text(arguments: &Value) -> Result<&str> {
    let value = required_text(arguments, "text")?;
    let characters = value.chars().count();
    if characters == 0
        || characters > MAX_TEXT_CHARS
        || value.chars().any(|character| {
            (character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
                || matches!(
                    character,
                    '\u{200b}'..='\u{200f}'
                        | '\u{202a}'..='\u{202e}'
                        | '\u{2060}'..='\u{206f}'
                )
        })
    {
        bail!(
            "text must contain 1-{MAX_TEXT_CHARS} visible characters without control, bidi, or zero-width formatting characters"
        );
    }
    Ok(value)
}

fn replace_text(arguments: &Value) -> bool {
    arguments
        .get("replace")
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

fn option_label(arguments: &Value) -> Result<String> {
    let value = required_text(arguments, "option_label")?;
    if value.chars().any(char::is_control) {
        bail!("option_label contains unsupported control characters");
    }
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let characters = normalized.chars().count();
    if characters == 0 || characters > MAX_OPTION_LABEL_CHARS {
        bail!("option_label must contain 1-{MAX_OPTION_LABEL_CHARS} visible characters");
    }
    Ok(normalized)
}

fn scroll_deltas(arguments: &Value) -> Result<(i64, i64)> {
    let delta_x = arguments
        .get("delta_x")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let delta_y = arguments
        .get("delta_y")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if !(-MAX_SCROLL_DELTA..=MAX_SCROLL_DELTA).contains(&delta_x)
        || !(-MAX_SCROLL_DELTA..=MAX_SCROLL_DELTA).contains(&delta_y)
        || (delta_x == 0 && delta_y == 0)
    {
        bail!(
            "delta_x and delta_y must be integers between -{MAX_SCROLL_DELTA} and {MAX_SCROLL_DELTA}, with at least one non-zero value"
        );
    }
    Ok((delta_x, delta_y))
}

fn history_direction(arguments: &Value) -> Result<&str> {
    match required_text(arguments, "direction")? {
        "back" => Ok("back"),
        "forward" => Ok("forward"),
        _ => bail!("direction must be back or forward"),
    }
}

fn bounded_page_metric(result: &Value, field: &str) -> Result<u64> {
    result
        .get(field)
        .and_then(Value::as_u64)
        .filter(|value| *value <= 1_000_000_000)
        .with_context(|| format!("Chrome extension returned invalid {field}"))
}

fn screenshot_quality(arguments: &Value) -> Result<u64> {
    let quality = arguments
        .get("quality")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_SCREENSHOT_QUALITY);
    if !(MIN_SCREENSHOT_QUALITY..=MAX_SCREENSHOT_QUALITY).contains(&quality) {
        bail!("quality must be between {MIN_SCREENSHOT_QUALITY} and {MAX_SCREENSHOT_QUALITY}");
    }
    Ok(quality)
}

fn upload_path_argument(arguments: &Value) -> Result<&str> {
    let path = required_text(arguments, "path")?;
    if path.chars().count() > MAX_UPLOAD_PATH_CHARS || path.chars().any(char::is_control) {
        bail!("path is too long or contains control characters");
    }
    Ok(path)
}

fn download_path_argument(arguments: &Value) -> Result<&str> {
    let path = required_text(arguments, "path")?;
    if path.chars().count() > MAX_UPLOAD_PATH_CHARS || path.chars().any(char::is_control) {
        bail!("path is too long or contains control characters");
    }
    Ok(path)
}

fn download_max_bytes(arguments: &Value) -> Result<u64> {
    let max_bytes = arguments
        .get("max_bytes")
        .and_then(Value::as_u64)
        .unwrap_or(MAX_DOWNLOAD_BYTES);
    if !(1..=MAX_DOWNLOAD_BYTES).contains(&max_bytes) {
        bail!("max_bytes must be between 1 and {MAX_DOWNLOAD_BYTES}");
    }
    Ok(max_bytes)
}

struct ChromeDownloadDestination {
    root: PathBuf,
    parent: PathBuf,
    target: PathBuf,
    relative: String,
}

struct ChromeDownloadStaging {
    path: PathBuf,
    file: File,
}

impl ChromeDownloadStaging {
    fn create(destination: &ChromeDownloadDestination) -> Result<Self> {
        for _ in 0..4 {
            let path = destination
                .parent
                .join(format!("{DOWNLOAD_STAGING_PREFIX}{}.part", Uuid::new_v4()));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => return Ok(Self { path, file }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(error).context("create Chrome download staging file");
                }
            }
        }
        bail!("could not reserve a unique Chrome download staging file")
    }

    fn file_mut(&mut self) -> &mut File {
        &mut self.file
    }

    fn sync(&mut self) -> Result<()> {
        self.file
            .flush()
            .context("flush Chrome download staging file")?;
        self.file
            .sync_all()
            .context("sync Chrome download staging file")
    }

    fn commit(&self, destination: &ChromeDownloadDestination) -> Result<()> {
        validate_download_destination(destination)?;
        fs::hard_link(self.path.as_path(), destination.target.as_path()).with_context(|| {
            format!(
                "create new Chrome download destination {}",
                destination.relative
            )
        })
    }
}

impl Drop for ChromeDownloadStaging {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.path.as_path());
    }
}

fn resolve_download_destination(
    state: &LocalState,
    request: &RelayRequest,
    requested: &str,
) -> Result<ChromeDownloadDestination> {
    if request.workspace_id.trim().is_empty() {
        bail!("workspace_id is required for Chrome file download");
    }
    let workspace = workspace_for_request(state, request.workspace_id.as_str())?;
    let relative = normalize_request_workspace_relative_path(workspace, request, requested)?;
    prepare_download_destination_at_root(workspace.absolute_root.as_path(), relative)
}

fn prepare_download_destination_at_root(
    root: &Path,
    relative: String,
) -> Result<ChromeDownloadDestination> {
    if relative == "." {
        bail!("Chrome download path must name a new workspace file");
    }
    let root = canonicalize_existing_dir(root)?;
    let target = root.join(Path::new(relative.as_str()));
    let parent = target
        .parent()
        .context("Chrome download path has no parent directory")?
        .to_path_buf();
    validate_non_symlink_directory_chain(root.as_path(), parent.as_path())?;
    let parent = canonicalize_existing_dir(parent.as_path())?;
    if !parent.starts_with(root.as_path()) {
        bail!("Chrome download path escapes the authorized workspace");
    }
    reject_existing_download_target(target.as_path())?;
    Ok(ChromeDownloadDestination {
        root,
        parent,
        target,
        relative,
    })
}

fn validate_download_destination(destination: &ChromeDownloadDestination) -> Result<()> {
    validate_non_symlink_directory_chain(destination.root.as_path(), destination.parent.as_path())?;
    let current_parent = canonicalize_existing_dir(destination.parent.as_path())?;
    if current_parent != destination.parent || !current_parent.starts_with(&destination.root) {
        bail!("Chrome download destination parent changed during transfer");
    }
    reject_existing_download_target(destination.target.as_path())
}

fn validate_non_symlink_directory_chain(root: &Path, parent: &Path) -> Result<()> {
    let relative = parent
        .strip_prefix(root)
        .context("Chrome download path escapes the authorized workspace")?;
    let mut cursor = root.to_path_buf();
    for component in relative.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            bail!("Chrome download path contains an unsupported component");
        }
        cursor.push(component.as_os_str());
        let metadata = fs::symlink_metadata(cursor.as_path())
            .with_context(|| format!("inspect Chrome download parent {}", cursor.display()))?;
        if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
            bail!("Chrome download parent must be an existing non-symlink workspace directory");
        }
    }
    Ok(())
}

fn reject_existing_download_target(target: &Path) -> Result<()> {
    match fs::symlink_metadata(target) {
        Ok(_) => bail!("Chrome download refuses to overwrite an existing workspace path"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).context("inspect Chrome download destination"),
    }
}

fn download_sha256(value: Option<&Value>) -> Result<String> {
    let value = value
        .and_then(Value::as_str)
        .filter(|value| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        })
        .context("Chrome extension returned an invalid download SHA-256")?;
    Ok(value.to_string())
}

fn download_source_kind(value: Option<&Value>) -> Result<&str> {
    match value.and_then(Value::as_str) {
        Some(kind @ ("http" | "https" | "blob" | "data")) => Ok(kind),
        _ => bail!("Chrome extension returned an invalid download source kind"),
    }
}

fn download_source_url(result: &Value, source_kind: &str) -> Result<Option<String>> {
    let value = result.get("source_url");
    match source_kind {
        "http" | "https" => {
            let raw = value
                .and_then(Value::as_str)
                .context("Chrome extension HTTP(S) download is missing its final URL")?;
            let sanitized = sanitize_page_url(raw)?;
            let scheme = Url::parse(sanitized.as_str())
                .context("parse sanitized Chrome download URL")?
                .scheme()
                .to_string();
            if scheme != source_kind {
                bail!("Chrome extension download source kind changed unexpectedly");
            }
            Ok(Some(sanitized))
        }
        "blob" | "data" => {
            if !matches!(value, None | Some(Value::Null)) {
                bail!("Chrome extension exposed an unexpected non-HTTP download URL");
            }
            Ok(None)
        }
        _ => bail!("Chrome extension returned an invalid download source kind"),
    }
}

fn download_mime_type(value: Option<&Value>) -> Result<String> {
    let value = value
        .and_then(Value::as_str)
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 120
                && value.bytes().all(|byte| byte.is_ascii_graphic())
                && value.split_once('/').is_some_and(|(kind, subtype)| {
                    !kind.is_empty() && !subtype.is_empty() && !subtype.contains('/')
                })
        })
        .context("Chrome extension returned an invalid download MIME type")?;
    Ok(value.to_string())
}

struct ChromeUploadFile {
    relative: String,
    filename: String,
    bytes: Vec<u8>,
    sha256: String,
    mime_type: &'static str,
}

fn resolve_upload_file(
    state: &LocalState,
    request: &RelayRequest,
    requested: &str,
) -> Result<ChromeUploadFile> {
    if request.workspace_id.trim().is_empty() {
        bail!("workspace_id is required for Chrome file upload");
    }
    let workspace = workspace_for_request(state, request.workspace_id.as_str())?;
    let root = canonicalize_existing_dir(workspace.absolute_root.as_path())?;
    let relative = normalize_request_workspace_relative_path(workspace, request, requested)?;
    if relative == "." {
        bail!("Chrome upload path must name a workspace file");
    }
    resolve_upload_file_at_root(root.as_path(), relative)
}

fn resolve_upload_file_at_root(root: &Path, relative: String) -> Result<ChromeUploadFile> {
    let candidate = root.join(Path::new(relative.as_str()));
    let link_metadata = fs::symlink_metadata(candidate.as_path())
        .with_context(|| format!("read Chrome upload file {relative}"))?;
    if link_metadata.file_type().is_symlink() || !link_metadata.file_type().is_file() {
        bail!("Chrome upload path must be a regular non-symlink workspace file");
    }
    if link_metadata.len() == 0 || link_metadata.len() > MAX_UPLOAD_BYTES {
        bail!("Chrome upload file must contain 1-{MAX_UPLOAD_BYTES} bytes");
    }
    let canonical = candidate
        .canonicalize()
        .with_context(|| format!("canonicalize Chrome upload file {relative}"))?;
    if !canonical.starts_with(root) {
        bail!("Chrome upload path escapes the authorized workspace");
    }
    let filename = canonical
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 255
                && !value.chars().any(char::is_control)
                && !value.contains('/')
                && !value.contains('\\')
        })
        .context("Chrome upload filename is invalid")?
        .to_string();
    let bytes = fs::read(canonical.as_path())
        .with_context(|| format!("read Chrome upload file {relative}"))?;
    if bytes.len() as u64 != link_metadata.len() || bytes.len() as u64 > MAX_UPLOAD_BYTES {
        bail!("Chrome upload file changed while it was being read");
    }
    let after = fs::metadata(canonical.as_path())
        .with_context(|| format!("recheck Chrome upload file {relative}"))?;
    if !after.is_file() || after.len() != bytes.len() as u64 {
        bail!("Chrome upload file changed while it was being read");
    }
    let sha256 = hex::encode(Sha256::digest(bytes.as_slice()));
    let mime_type = upload_mime_type(canonical.as_path());
    Ok(ChromeUploadFile {
        relative,
        filename,
        bytes,
        sha256,
        mime_type,
    })
}

fn upload_mime_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("txt" | "md" | "csv" | "tsv" | "log") => "text/plain",
        Some("json") => "application/json",
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("docx") => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        Some("xlsx") => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        Some("pptx") => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        Some("zip") => "application/zip",
        _ => "application/octet-stream",
    }
}

fn ensure_not_cancelled(action_cancelled: Option<&AtomicBool>) -> Result<()> {
    if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)) {
        bail!("Chrome action was cancelled");
    }
    Ok(())
}

fn valid_prefixed_numeric_id(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(|suffix| {
        !suffix.is_empty()
            && suffix.len() <= 10
            && suffix.bytes().all(|byte| byte.is_ascii_digit())
            && !suffix.starts_with('0')
    })
}

fn bounded_optional_text(value: Option<&Value>, max_bytes: usize) -> Result<Option<String>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value))
            if value.len() <= max_bytes && !value.chars().any(char::is_control) =>
        {
            Ok(Some(value.clone()))
        }
        _ => bail!("Chrome extension returned malformed bounded text"),
    }
}

fn bounded_required_text(value: Option<&Value>, max_bytes: usize) -> Result<String> {
    bounded_optional_text(value, max_bytes)?
        .filter(|value| !value.is_empty())
        .context("Chrome extension returned missing bounded text")
}

fn chrome_status_tool() -> Value {
    tool("chrome_status", "Inspect whether the packaged ChatOS Chrome extension and Native Messaging Host are registered and connected. This does not return tab URLs or signed-in page content.", Map::new(), Vec::new())
}

fn chrome_tabs_tool() -> Value {
    let mut properties = Map::new();
    properties.insert(
        "limit".to_string(),
        json!({"type":"integer","minimum":1,"maximum":MAX_TAB_LIMIT,"default":DEFAULT_TAB_LIMIT}),
    );
    tool(
        "chrome_tabs",
        "After local approval, list only Chrome tabs explicitly connected by the user from the ChatOS extension popup. URL query values are redacted.",
        properties,
        Vec::new(),
    )
}

fn chrome_tab_snapshot_tool() -> Value {
    let mut properties = Map::new();
    properties.insert(
        "tab_id".to_string(),
        json!({"type":"string","pattern":"^ct[1-9][0-9]{0,9}$"}),
    );
    properties.insert(
        "max_chars".to_string(),
        json!({"type":"integer","minimum":MIN_SNAPSHOT_CHARS,"maximum":MAX_SNAPSHOT_CHARS,"default":DEFAULT_SNAPSHOT_CHARS}),
    );
    tool(
        "chrome_tab_snapshot",
        "After local approval, read a bounded structural snapshot from one user-connected Chrome tab. Form values, password fields, cookies, storage, history and downloads are not read.",
        properties,
        vec!["tab_id"],
    )
}

fn chrome_tab_navigate_tool() -> Value {
    let mut properties = Map::new();
    properties.insert(
        "tab_id".to_string(),
        json!({"type":"string","pattern":"^ct[1-9][0-9]{0,9}$"}),
    );
    properties.insert(
        "url".to_string(),
        json!({"type":"string","maxLength":8192,"description":"Absolute HTTP(S) URL on the tab's currently authorized exact origin."}),
    );
    tool(
        "chrome_tab_navigate",
        "After local approval, navigate one connected Chrome tab to an HTTP(S) URL on its currently authorized exact origin. Cross-origin navigation is rejected and action targets become stale.",
        properties,
        vec!["tab_id", "url"],
    )
}

fn chrome_tab_click_tool() -> Value {
    let mut properties = Map::new();
    properties.insert(
        "tab_id".to_string(),
        json!({"type":"string","pattern":"^ct[1-9][0-9]{0,9}$"}),
    );
    properties.insert(
        "target_id".to_string(),
        json!({"type":"string","pattern":"^cr[0-9a-f]{16}-[1-9][0-9]{0,3}$","description":"Short-lived target ID from the latest chrome_tab_snapshot."}),
    );
    tool(
        "chrome_tab_click",
        "After local approval, click one visible, unchanged target from the latest snapshot of an explicitly connected Chrome tab. Capture a fresh snapshot after the click.",
        properties,
        vec!["tab_id", "target_id"],
    )
}

fn chrome_tab_type_text_tool() -> Value {
    let mut properties = Map::new();
    properties.insert(
        "tab_id".to_string(),
        json!({"type":"string","pattern":"^ct[1-9][0-9]{0,9}$"}),
    );
    properties.insert(
        "target_id".to_string(),
        json!({"type":"string","pattern":"^cr[0-9a-f]{16}-[1-9][0-9]{0,3}$","description":"Short-lived editable target ID from the latest chrome_tab_snapshot."}),
    );
    properties.insert(
        "text".to_string(),
        json!({"type":"string","minLength":1,"maxLength":MAX_TEXT_CHARS,"description":"Visible text. Password/secure controls and control or direction-format characters are rejected; raw text is omitted from persisted result metadata."}),
    );
    properties.insert(
        "replace".to_string(),
        json!({"type":"boolean","default":true,"description":"Replace the current control value; false appends."}),
    );
    tool(
        "chrome_tab_type_text",
        "After local approval, type bounded text into one safe editable target from the latest connected-tab snapshot. Password and secure fields are rejected.",
        properties,
        vec!["tab_id", "target_id", "text"],
    )
}

fn chrome_tab_select_tool() -> Value {
    let mut properties = Map::new();
    properties.insert(
        "tab_id".to_string(),
        json!({"type":"string","pattern":"^ct[1-9][0-9]{0,9}$"}),
    );
    properties.insert(
        "target_id".to_string(),
        json!({"type":"string","pattern":"^cr[0-9a-f]{16}-[1-9][0-9]{0,3}$","description":"Short-lived native select target ID from the latest chrome_tab_snapshot."}),
    );
    properties.insert(
        "option_label".to_string(),
        json!({"type":"string","minLength":1,"maxLength":MAX_OPTION_LABEL_CHARS,"description":"Exact visible option label shown in the latest snapshot. Duplicate, disabled, missing and multi-select options are rejected."}),
    );
    tool(
        "chrome_tab_select",
        "After local approval, choose one uniquely matching enabled option in a native single-select target from the latest connected-tab snapshot.",
        properties,
        vec!["tab_id", "target_id", "option_label"],
    )
}

fn chrome_tab_scroll_tool() -> Value {
    let mut properties = Map::new();
    properties.insert(
        "tab_id".to_string(),
        json!({"type":"string","pattern":"^ct[1-9][0-9]{0,9}$"}),
    );
    properties.insert(
        "delta_x".to_string(),
        json!({"type":"integer","minimum":-MAX_SCROLL_DELTA,"maximum":MAX_SCROLL_DELTA,"default":0}),
    );
    properties.insert(
        "delta_y".to_string(),
        json!({"type":"integer","minimum":-MAX_SCROLL_DELTA,"maximum":MAX_SCROLL_DELTA,"default":0}),
    );
    tool(
        "chrome_tab_scroll",
        "After local approval, scroll one explicitly connected Chrome tab by bounded pixel deltas. At least one delta must be non-zero; capture a fresh snapshot afterwards.",
        properties,
        vec!["tab_id"],
    )
}

fn chrome_tab_history_tool() -> Value {
    let mut properties = Map::new();
    properties.insert(
        "tab_id".to_string(),
        json!({"type":"string","pattern":"^ct[1-9][0-9]{0,9}$"}),
    );
    properties.insert(
        "direction".to_string(),
        json!({"type":"string","enum":["back","forward"]}),
    );
    tool(
        "chrome_tab_history",
        "After local approval, move backward or forward in one connected Chrome tab. If history leaves the authorized origin, the claim is released and the action fails closed.",
        properties,
        vec!["tab_id", "direction"],
    )
}

fn chrome_tab_activate_tool() -> Value {
    let mut properties = Map::new();
    properties.insert(
        "tab_id".to_string(),
        json!({"type":"string","pattern":"^ct[1-9][0-9]{0,9}$"}),
    );
    tool(
        "chrome_tab_activate",
        "After local approval, activate one explicitly connected tab inside its existing Chrome window without forcing the window to the foreground. Useful before a visible-viewport screenshot.",
        properties,
        vec!["tab_id"],
    )
}

fn chrome_tab_upload_tool() -> Value {
    let mut properties = Map::new();
    properties.insert(
        "tab_id".to_string(),
        json!({"type":"string","pattern":"^ct[1-9][0-9]{0,9}$"}),
    );
    properties.insert(
        "target_id".to_string(),
        json!({"type":"string","pattern":"^cr[0-9a-f]{16}-[1-9][0-9]{0,3}$","description":"Short-lived file-input target ID from the latest chrome_tab_snapshot."}),
    );
    properties.insert(
        "path".to_string(),
        json!({"type":"string","maxLength":MAX_UPLOAD_PATH_CHARS,"description":"Workspace-relative regular non-symlink file, 1 byte to 10 MiB."}),
    );
    tool(
        "chrome_tab_upload",
        "After local approval, upload one bounded regular non-symlink file from the authorized workspace into a connected tab's file input using hash-verified Native Messaging chunks.",
        properties,
        vec!["tab_id", "target_id", "path"],
    )
}

fn chrome_tab_download_tool() -> Value {
    let mut properties = Map::new();
    properties.insert(
        "tab_id".to_string(),
        json!({"type":"string","pattern":"^ct[1-9][0-9]{0,9}$"}),
    );
    properties.insert(
        "target_id".to_string(),
        json!({"type":"string","pattern":"^cr[0-9a-f]{16}-[1-9][0-9]{0,3}$","description":"Short-lived direct-link target ID from the latest chrome_tab_snapshot."}),
    );
    properties.insert(
        "path".to_string(),
        json!({"type":"string","maxLength":MAX_UPLOAD_PATH_CHARS,"description":"Workspace-relative destination whose parent already exists. The destination must not exist and is never overwritten."}),
    );
    properties.insert(
        "max_bytes".to_string(),
        json!({"type":"integer","minimum":1,"maximum":MAX_DOWNLOAD_BYTES,"default":MAX_DOWNLOAD_BYTES,"description":"Approved maximum response size, up to 10 MiB."}),
    );
    tool(
        "chrome_tab_download",
        "After local approval, fetch one unchanged direct-link target from the latest connected-tab snapshot and save its hash-verified bytes as a new workspace file. Only same-origin HTTP(S)/blob or bounded data links are accepted; existing paths are never overwritten.",
        properties,
        vec!["tab_id", "target_id", "path"],
    )
}

fn chrome_tab_screenshot_tool() -> Value {
    let mut properties = Map::new();
    properties.insert(
        "tab_id".to_string(),
        json!({"type":"string","pattern":"^ct[1-9][0-9]{0,9}$"}),
    );
    properties.insert(
        "quality".to_string(),
        json!({"type":"integer","minimum":MIN_SCREENSHOT_QUALITY,"maximum":MAX_SCREENSHOT_QUALITY,"default":DEFAULT_SCREENSHOT_QUALITY}),
    );
    tool(
        "chrome_tab_screenshot",
        "After local approval, capture the visible viewport of an active, explicitly connected Chrome tab as a bounded transient JPEG model input. The image is not persisted in structured tool history.",
        properties,
        vec!["tab_id"],
    )
}

fn chrome_tab_release_tool() -> Value {
    let mut properties = Map::new();
    properties.insert(
        "tab_id".to_string(),
        json!({"type":"string","pattern":"^ct[1-9][0-9]{0,9}$"}),
    );
    tool(
        "chrome_tab_release",
        "After local approval, release one ChatOS-connected Chrome tab. Site permission remains under the user's control in the extension popup.",
        properties,
        vec!["tab_id"],
    )
}

fn tool(
    name: &str,
    description: &str,
    properties: Map<String, Value>,
    required: Vec<&str>,
) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_query_values_are_redacted_and_fragments_removed() {
        let sanitized =
            sanitize_page_url("https://example.com/account?token=secret&view=private#credential")
                .expect("sanitize URL");
        assert!(sanitized.contains("token=%5BREDACTED%5D"));
        assert!(sanitized.contains("view=%5BREDACTED%5D"));
        assert!(!sanitized.contains("secret"));
        assert!(!sanitized.contains("credential"));
    }

    #[test]
    fn sensitive_tools_are_hidden_without_local_approval() {
        let names = tool_definitions(false)
            .into_iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str).map(str::to_string))
            .collect::<Vec<_>>();
        assert_eq!(names, ["chrome_status"]);
    }

    #[test]
    fn writable_tools_require_short_lived_targets_and_redact_text_from_approval_args() {
        let names = tool_definitions(true)
            .into_iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str).map(str::to_string))
            .collect::<Vec<_>>();
        assert_eq!(names.len(), 14);
        for required in [
            "chrome_tab_navigate",
            "chrome_tab_click",
            "chrome_tab_type_text",
            "chrome_tab_select",
            "chrome_tab_scroll",
            "chrome_tab_history",
            "chrome_tab_activate",
            "chrome_tab_upload",
            "chrome_tab_download",
            "chrome_tab_screenshot",
        ] {
            assert!(names.iter().any(|name| name == required));
            assert!(requires_interactive_approval(required));
        }

        let secret = "not-persisted-text";
        let (_, args) = approval_command(
            "chrome_tab_type_text",
            &json!({
                "tab_id":"ct12",
                "target_id":"cr0123456789abcdef-3",
                "text":secret,
            }),
        )
        .expect("approval command");
        let rendered = args.join(" ");
        assert!(!rendered.contains(secret));
        assert!(rendered.contains("text_sha256="));
        assert!(rendered.contains("text_chars=18"));

        let (_, download_args) = approval_command(
            "chrome_tab_download",
            &json!({
                "tab_id":"ct12",
                "target_id":"cr0123456789abcdef-3",
                "path":"exports/report.pdf",
                "max_bytes":4096,
            }),
        )
        .expect("download approval command");
        assert_eq!(
            download_args,
            [
                "download",
                "tab_id=ct12",
                "target_id=cr0123456789abcdef-3",
                "path=exports/report.pdf",
                "max_bytes=4096",
            ]
        );
    }

    #[test]
    fn navigation_and_target_inputs_fail_closed() {
        assert!(navigation_url(&json!({"url":"javascript:alert(1)"})).is_err());
        assert!(navigation_url(&json!({"url":"https://user:pass@example.com/"})).is_err());
        assert!(chrome_target_id(&json!({"target_id":"cr0123456789abcdef-1"})).is_ok());
        assert!(chrome_target_id(&json!({"target_id":"cr0123456789abcdef-0"})).is_err());
        assert!(chrome_text(&json!({"text":"line one\nline two"})).is_ok());
        assert!(chrome_text(&json!({"text":"bad\u{202e}text"})).is_err());
        assert_eq!(
            option_label(&json!({"option_label":"  First   option  "})).unwrap(),
            "First option"
        );
        assert_eq!(scroll_deltas(&json!({"delta_y":500})).unwrap(), (0, 500));
        assert!(scroll_deltas(&json!({"delta_x":0,"delta_y":0})).is_err());
        assert!(scroll_deltas(&json!({"delta_x":2001})).is_err());
        assert_eq!(
            history_direction(&json!({"direction":"back"})).unwrap(),
            "back"
        );
        assert!(history_direction(&json!({"direction":"reload"})).is_err());
    }

    #[test]
    fn upload_files_are_bounded_regular_workspace_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().canonicalize().expect("canonical root");
        fs::write(root.join("report.pdf"), b"bounded pdf bytes").expect("write file");
        let file =
            resolve_upload_file_at_root(&root, "report.pdf".to_string()).expect("resolve upload");
        assert_eq!(file.relative, "report.pdf");
        assert_eq!(file.filename, "report.pdf");
        assert_eq!(file.mime_type, "application/pdf");
        assert_eq!(file.bytes, b"bounded pdf bytes");

        fs::write(root.join("empty.txt"), []).expect("write empty");
        assert!(resolve_upload_file_at_root(&root, "empty.txt".to_string()).is_err());

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("report.pdf"), root.join("link.pdf"))
                .expect("create symlink");
            assert!(resolve_upload_file_at_root(&root, "link.pdf".to_string()).is_err());
        }
    }

    #[test]
    fn download_bounds_fail_closed() {
        assert_eq!(download_max_bytes(&json!({})).unwrap(), MAX_DOWNLOAD_BYTES);
        assert_eq!(download_max_bytes(&json!({"max_bytes":1})).unwrap(), 1);
        assert!(download_max_bytes(&json!({"max_bytes":0})).is_err());
        assert!(download_max_bytes(&json!({"max_bytes":MAX_DOWNLOAD_BYTES + 1})).is_err());
        assert!(download_path_argument(&json!({"path":"exports/report.pdf"})).is_ok());
        assert!(download_path_argument(&json!({"path":"bad\npath"})).is_err());
    }

    #[test]
    fn download_destination_is_create_new_and_staged_in_the_same_parent() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().canonicalize().expect("canonical root");
        fs::create_dir(root.join("exports")).expect("create exports");
        let destination =
            prepare_download_destination_at_root(root.as_path(), "exports/report.pdf".to_string())
                .expect("prepare download destination");
        let mut staging = ChromeDownloadStaging::create(&destination).expect("create staging");
        assert_eq!(staging.path.parent(), Some(destination.parent.as_path()));
        let staging_name = staging
            .path
            .file_name()
            .and_then(|value| value.to_str())
            .expect("staging filename");
        assert!(staging_name.starts_with(DOWNLOAD_STAGING_PREFIX));
        assert!(staging_name.ends_with(".part"));
        staging
            .file_mut()
            .write_all(b"verified download bytes")
            .expect("write staging");
        staging.sync().expect("sync staging");
        staging.commit(&destination).expect("commit download");
        drop(staging);
        assert_eq!(
            fs::read(root.join("exports/report.pdf")).expect("read committed file"),
            b"verified download bytes"
        );
        assert!(prepare_download_destination_at_root(
            root.as_path(),
            "exports/report.pdf".to_string()
        )
        .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn download_destination_rejects_symlink_parents_and_targets() {
        let temp = tempfile::tempdir().expect("tempdir");
        let outside = tempfile::tempdir().expect("outside tempdir");
        let root = temp.path().canonicalize().expect("canonical root");
        std::os::unix::fs::symlink(outside.path(), root.join("escape"))
            .expect("create parent symlink");
        assert!(prepare_download_destination_at_root(
            root.as_path(),
            "escape/report.pdf".to_string()
        )
        .is_err());

        fs::create_dir(root.join("exports")).expect("create exports");
        std::os::unix::fs::symlink(
            outside.path().join("missing"),
            root.join("exports/link.pdf"),
        )
        .expect("create target symlink");
        assert!(prepare_download_destination_at_root(
            root.as_path(),
            "exports/link.pdf".to_string()
        )
        .is_err());
    }
}
