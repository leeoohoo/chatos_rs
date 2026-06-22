#[path = "actions_basic.rs"]
mod actions_basic;
#[path = "actions_config.rs"]
mod actions_config;
#[path = "actions_console.rs"]
mod actions_console;
#[path = "actions_console_support.rs"]
mod actions_console_support;
#[path = "actions_inspect.rs"]
mod actions_inspect;
#[path = "actions_inspect_support.rs"]
mod actions_inspect_support;
#[path = "actions_research.rs"]
mod actions_research;
#[path = "actions_research_payloads.rs"]
mod actions_research_payloads;
#[path = "actions_research_text.rs"]
mod actions_research_text;
#[path = "actions_shared.rs"]
mod actions_shared;
#[path = "actions_vision.rs"]
mod actions_vision;

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
