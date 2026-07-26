// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[path = "actions_basic.rs"]
mod actions_basic;
#[path = "actions_cdp.rs"]
mod actions_cdp;
#[path = "actions_config.rs"]
mod actions_config;
#[path = "actions_console.rs"]
mod actions_console;
#[path = "actions_console_support.rs"]
mod actions_console_support;
#[path = "actions_files.rs"]
mod actions_files;
#[path = "actions_har.rs"]
mod actions_har;
#[path = "actions_inspect.rs"]
mod actions_inspect;
#[path = "actions_inspect_support.rs"]
mod actions_inspect_support;
#[path = "actions_network.rs"]
mod actions_network;
#[path = "actions_research.rs"]
mod actions_research;
#[path = "actions_research_payloads.rs"]
mod actions_research_payloads;
#[path = "actions_research_text.rs"]
mod actions_research_text;
#[path = "actions_routes.rs"]
mod actions_routes;
#[path = "actions_shared.rs"]
mod actions_shared;
#[path = "actions_tabs.rs"]
mod actions_tabs;
#[path = "actions_vision.rs"]
mod actions_vision;
#[path = "actions_websocket.rs"]
mod actions_websocket;

use serde_json::Value;

use chatos_mcp_runtime::ToolCallerModelRuntime;

use super::BoundContext;
pub(super) const DEFAULT_BROWSER_RESEARCH_REQUEST_TIMEOUT_SECONDS: u64 =
    actions_config::DEFAULT_BROWSER_RESEARCH_REQUEST_TIMEOUT_SECONDS;
pub(super) const DEFAULT_BROWSER_RESEARCH_LIMIT: usize =
    actions_config::DEFAULT_BROWSER_RESEARCH_LIMIT;
pub(super) const MAX_BROWSER_RESEARCH_LIMIT: usize = actions_config::MAX_BROWSER_RESEARCH_LIMIT;
pub(super) const MAX_BROWSER_RESEARCH_EXTRACT_URLS: usize =
    actions_config::MAX_BROWSER_RESEARCH_EXTRACT_URLS;
pub(super) const DEFAULT_BROWSER_RESEARCH_MAX_EXTRACT_CHARS: usize =
    actions_config::DEFAULT_BROWSER_RESEARCH_MAX_EXTRACT_CHARS;
pub(super) const DEFAULT_BROWSER_NETWORK_LIMIT: usize =
    actions_network::DEFAULT_BROWSER_NETWORK_LIMIT;
pub(super) const MAX_BROWSER_NETWORK_LIMIT: usize = actions_network::MAX_BROWSER_NETWORK_LIMIT;
pub(super) const DEFAULT_BROWSER_NETWORK_BODY_CHARS: usize =
    actions_network::DEFAULT_BROWSER_NETWORK_BODY_CHARS;
pub(super) const MAX_BROWSER_NETWORK_BODY_CHARS: usize =
    actions_network::MAX_BROWSER_NETWORK_BODY_CHARS;
pub(super) const MAX_BROWSER_UPLOAD_FILES: usize = actions_files::MAX_BROWSER_UPLOAD_FILES;
pub(super) const DEFAULT_BROWSER_HAR_MAX_ENTRIES: usize =
    actions_har::DEFAULT_BROWSER_HAR_MAX_ENTRIES;
pub(super) const MAX_BROWSER_HAR_ENTRIES: usize = actions_har::MAX_BROWSER_HAR_ENTRIES;
pub(super) const DEFAULT_BROWSER_WEBSOCKET_FRAME_LIMIT: usize =
    actions_websocket::DEFAULT_BROWSER_WEBSOCKET_FRAME_LIMIT;
pub(super) const MAX_BROWSER_WEBSOCKET_FRAME_LIMIT: usize =
    actions_websocket::MAX_BROWSER_WEBSOCKET_FRAME_LIMIT;
pub(super) const DEFAULT_BROWSER_WEBSOCKET_PAYLOAD_CHARS: usize =
    actions_websocket::DEFAULT_BROWSER_WEBSOCKET_PAYLOAD_CHARS;
pub(super) const MAX_BROWSER_WEBSOCKET_PAYLOAD_CHARS: usize =
    actions_websocket::MAX_BROWSER_WEBSOCKET_PAYLOAD_CHARS;
pub(super) const MAX_BROWSER_ROUTE_PATTERN_CHARS: usize =
    actions_routes::MAX_BROWSER_ROUTE_PATTERN_CHARS;
pub(super) use actions_routes::BrowserRouteRecord;

pub(super) fn browser_interactive_approval_command(
    tool_name: &str,
    arguments: &Value,
) -> Result<(String, Vec<String>), String> {
    match tool_name {
        "browser_route_add" => actions_routes::browser_route_approval_command(arguments),
        "browser_cdp_command" => actions_cdp::browser_cdp_approval_command(arguments),
        _ => Err(format!(
            "Browser tool does not require interactive approval: {tool_name}"
        )),
    }
}

pub(super) async fn browser_route_add_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    arguments: Value,
) -> Result<Value, String> {
    actions_routes::browser_route_add_with_context(ctx, conversation_id, arguments).await
}

pub(super) async fn browser_route_list_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
) -> Result<Value, String> {
    actions_routes::browser_route_list_with_context(ctx, conversation_id).await
}

pub(super) async fn browser_route_remove_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    route_id: String,
) -> Result<Value, String> {
    actions_routes::browser_route_remove_with_context(ctx, conversation_id, route_id).await
}

pub(super) async fn browser_route_clear_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
) -> Result<Value, String> {
    actions_routes::browser_route_clear_with_context(ctx, conversation_id).await
}

pub(super) async fn browser_cdp_command_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    arguments: Value,
) -> Result<Value, String> {
    actions_cdp::browser_cdp_command_with_context(ctx, conversation_id, arguments).await
}

pub(super) fn discard_browser_routes(ctx: &BoundContext, conversation_key: &str) {
    actions_routes::discard_browser_routes(ctx, conversation_key);
}

pub(super) fn mark_browser_session_closed(session_name: &str) {
    actions_routes::mark_browser_session_closed(session_name);
}

pub(super) async fn browser_research_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    caller_model_runtime: Option<ToolCallerModelRuntime>,
    question: String,
    web_query: Option<String>,
    include_web: bool,
    web_limit: Option<usize>,
    extract_top: Option<usize>,
    full: bool,
    annotate: bool,
) -> Result<Value, String> {
    actions_research::browser_research_with_context(
        ctx,
        conversation_id,
        caller_model_runtime,
        question,
        web_query,
        include_web,
        web_limit,
        extract_top,
        full,
        annotate,
    )
    .await
}

pub(super) async fn browser_vision_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    caller_model_runtime: Option<ToolCallerModelRuntime>,
    question: String,
    annotate: bool,
) -> Result<Value, String> {
    actions_vision::browser_vision_with_context(
        ctx,
        conversation_id,
        caller_model_runtime,
        question,
        annotate,
    )
    .await
}

pub(super) async fn browser_console_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    clear: bool,
    expression: Option<String>,
) -> Result<Value, String> {
    actions_console::browser_console_with_context(ctx, conversation_id, clear, expression).await
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
    actions_network::browser_network_with_context(
        ctx,
        conversation_id,
        clear,
        limit,
        filter,
        resource_types,
        method,
        status,
    )
    .await
}

pub(super) async fn browser_network_request_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    request_id: String,
    include_request_body: bool,
    include_response_body: bool,
    max_body_chars: usize,
) -> Result<Value, String> {
    actions_network::browser_network_request_with_context(
        ctx,
        conversation_id,
        request_id,
        include_request_body,
        include_response_body,
        max_body_chars,
    )
    .await
}

pub(super) async fn browser_har_start_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
) -> Result<Value, String> {
    actions_har::browser_har_start_with_context(ctx, conversation_id).await
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
    actions_har::browser_har_stop_with_context(
        ctx,
        conversation_id,
        path,
        include_request_bodies,
        include_response_bodies,
        max_body_chars,
        max_entries,
    )
    .await
}

pub(super) async fn browser_websocket_start_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
) -> Result<Value, String> {
    actions_websocket::browser_websocket_start_with_context(ctx, conversation_id).await
}

pub(super) async fn browser_websocket_frames_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    clear: bool,
    limit: usize,
    request_id: Option<String>,
    direction: Option<String>,
    include_text_payloads: bool,
    max_payload_chars: usize,
) -> Result<Value, String> {
    actions_websocket::browser_websocket_frames_with_context(
        ctx,
        conversation_id,
        clear,
        limit,
        request_id,
        direction,
        include_text_payloads,
        max_payload_chars,
    )
    .await
}

pub(super) async fn browser_websocket_stop_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
) -> Result<Value, String> {
    actions_websocket::browser_websocket_stop_with_context(ctx, conversation_id).await
}

pub(super) fn stop_browser_websocket_observer(ctx: &BoundContext, conversation_key: &str) {
    actions_websocket::stop_browser_websocket_observer(ctx, conversation_key);
}

pub(super) async fn browser_inspect_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    caller_model_runtime: Option<ToolCallerModelRuntime>,
    question: Option<String>,
    full: bool,
    annotate: bool,
) -> Result<Value, String> {
    actions_inspect::browser_inspect_with_context(
        ctx,
        conversation_id,
        caller_model_runtime,
        question,
        full,
        annotate,
    )
    .await
}

pub(super) async fn browser_navigate_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    url: String,
) -> Result<Value, String> {
    actions_basic::browser_navigate_with_context(ctx, conversation_id, url).await
}

pub(super) async fn browser_tabs_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
) -> Result<Value, String> {
    actions_tabs::browser_tabs_with_context(ctx, conversation_id).await
}

pub(super) async fn browser_tab_new_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    url: Option<String>,
) -> Result<Value, String> {
    actions_tabs::browser_tab_new_with_context(ctx, conversation_id, url).await
}

pub(super) async fn browser_tab_switch_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    tab_id: String,
) -> Result<Value, String> {
    actions_tabs::browser_tab_switch_with_context(ctx, conversation_id, tab_id).await
}

pub(super) async fn browser_tab_close_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    tab_id: String,
) -> Result<Value, String> {
    actions_tabs::browser_tab_close_with_context(ctx, conversation_id, tab_id).await
}

pub(super) async fn browser_snapshot_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    full: bool,
) -> Result<Value, String> {
    actions_basic::browser_snapshot_with_context(ctx, conversation_id, full).await
}

pub(super) async fn browser_click_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    reference: String,
) -> Result<Value, String> {
    actions_basic::browser_click_with_context(ctx, conversation_id, reference).await
}

pub(super) async fn browser_type_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    reference: String,
    text: String,
) -> Result<Value, String> {
    actions_basic::browser_type_with_context(ctx, conversation_id, reference, text).await
}

pub(super) async fn browser_scroll_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    direction: String,
) -> Result<Value, String> {
    actions_basic::browser_scroll_with_context(ctx, conversation_id, direction).await
}

pub(super) async fn browser_back_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
) -> Result<Value, String> {
    actions_basic::browser_back_with_context(ctx, conversation_id).await
}

pub(super) async fn browser_press_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    key: String,
) -> Result<Value, String> {
    actions_basic::browser_press_with_context(ctx, conversation_id, key).await
}

pub(super) async fn browser_get_images_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
) -> Result<Value, String> {
    actions_basic::browser_get_images_with_context(ctx, conversation_id).await
}

pub(super) async fn browser_upload_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    reference: String,
    paths: Vec<String>,
) -> Result<Value, String> {
    actions_files::browser_upload_with_context(ctx, conversation_id, reference, paths).await
}

pub(super) async fn browser_download_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    reference: String,
    path: String,
) -> Result<Value, String> {
    actions_files::browser_download_with_context(ctx, conversation_id, reference, path).await
}
