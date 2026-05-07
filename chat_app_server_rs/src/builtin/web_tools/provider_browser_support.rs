use serde_json::Value;

use crate::builtin::browser_command_support::{
    browser_command_error_text, browser_command_succeeded, parse_browser_command_eval_payload,
};
use crate::builtin::browser_runtime::{
    new_browser_session, run_browser_command, BrowserRuntimeSession,
};

use super::provider_types::BrowserRenderOptions;
use super::provider_utils::sanitize_provider_error;

pub(super) async fn open_browser_page(
    url: &str,
    options: &BrowserRenderOptions,
    open_timeout_seconds: u64,
) -> Result<Option<BrowserRuntimeSession>, String> {
    let session = new_browser_session();
    let open_result = run_browser_command(
        &options.workspace_dir,
        &session,
        "open",
        vec![url.to_string()],
        options.command_timeout_seconds.max(open_timeout_seconds),
    )
    .await?;
    if !browser_command_succeeded(&open_result) {
        return Ok(None);
    }
    Ok(Some(session))
}

pub(super) async fn eval_on_browser_page(
    session: &BrowserRuntimeSession,
    expression: String,
    options: &BrowserRenderOptions,
    timeout_seconds: u64,
) -> Result<Value, String> {
    run_browser_command(
        &options.workspace_dir,
        session,
        "eval",
        vec![expression],
        options.command_timeout_seconds.clamp(3, timeout_seconds),
    )
    .await
}

pub(super) async fn snapshot_browser_page(
    session: &BrowserRuntimeSession,
    compact: bool,
    options: &BrowserRenderOptions,
) -> Result<Value, String> {
    let args = if compact {
        vec!["-c".to_string()]
    } else {
        Vec::new()
    };
    run_browser_command(
        &options.workspace_dir,
        session,
        "snapshot",
        args,
        options.command_timeout_seconds,
    )
    .await
}

pub(super) async fn close_browser_page(
    session: &BrowserRuntimeSession,
    options: &BrowserRenderOptions,
) {
    let _ = run_browser_command(
        &options.workspace_dir,
        session,
        "close",
        Vec::new(),
        10,
    )
    .await;
}

pub(super) fn parse_browser_eval_result(raw: Value) -> Value {
    parse_browser_command_eval_payload(raw)
}

pub(super) fn is_browser_command_success(value: &Value) -> bool {
    browser_command_succeeded(value)
}

pub(crate) fn browser_command_error(value: &Value, fallback: &str) -> String {
    sanitize_provider_error(browser_command_error_text(value, fallback))
}
