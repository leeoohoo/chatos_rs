// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use futures::StreamExt;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio_tungstenite::connect_async_with_config;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

use super::super::managed_preview::{
    active_page_target_id, read_cdp_result, send_cdp_command, validate_loopback_cdp_endpoint,
    CDP_CONNECT_TIMEOUT, CDP_MAX_MESSAGE_BYTES,
};
use super::actions_shared::{
    copy_response_fields, get_or_create_session, is_success, run_browser_command,
};
use super::BoundContext;
use crate::browser_command_support::parse_browser_command_eval_payload;

const DEFAULT_CDP_TIMEOUT_SECONDS: u64 = 5;
const MAX_CDP_TIMEOUT_SECONDS: u64 = 15;
const MAX_CDP_METHOD_CHARS: usize = 160;
const MAX_CDP_PARAMS_BYTES: usize = 64 * 1024;
const MAX_CDP_RESULT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone)]
struct BrowserCdpSpec {
    target: &'static str,
    method: String,
    params: Value,
    params_json: String,
    timeout_seconds: u64,
}

pub(super) fn browser_cdp_approval_command(
    arguments: &Value,
) -> Result<(String, Vec<String>), String> {
    let spec = parse_cdp_spec(arguments)?;
    Ok((
        "browser_cdp_command".to_string(),
        vec![
            "--target".to_string(),
            spec.target.to_string(),
            "--method".to_string(),
            spec.method,
            "--params".to_string(),
            spec.params_json,
            "--timeout-seconds".to_string(),
            spec.timeout_seconds.to_string(),
        ],
    ))
}

pub(super) async fn browser_cdp_command_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    arguments: Value,
) -> Result<Value, String> {
    let spec = parse_cdp_spec(&arguments)?;
    let conversation_key = super::super::context::conversation_key(conversation_id);
    let (session, _) = get_or_create_session(&ctx, conversation_key.as_str());
    if session.cdp_url.is_some() {
        return Err(
            "full CDP access is limited to managed Local Connector browser sessions".to_string(),
        );
    }

    let page_url = if spec.target == "page" {
        let metadata = run_browser_command(
            &ctx,
            conversation_key.as_str(),
            "eval",
            vec!["JSON.stringify({url:window.location.href})".to_string()],
            ctx.command_timeout_seconds,
        )
        .await?;
        if !is_success(&metadata) {
            return Err("active browser page identity is unavailable for CDP access".to_string());
        }
        Some(active_page_url(&metadata)?)
    } else {
        None
    };

    let endpoint_result = run_browser_command(
        &ctx,
        conversation_key.as_str(),
        "get",
        vec!["cdp-url".to_string()],
        ctx.command_timeout_seconds,
    )
    .await?;
    if !is_success(&endpoint_result) {
        return Err("managed browser CDP endpoint is unavailable".to_string());
    }
    let endpoint = endpoint_result
        .pointer("/data/cdpUrl")
        .and_then(Value::as_str)
        .ok_or_else(|| "managed browser CDP endpoint is malformed".to_string())?;
    let endpoint = validate_loopback_cdp_endpoint(endpoint)?;
    let result = execute_cdp_command(endpoint.as_str(), page_url.as_deref(), &spec).await?;
    let result_bytes = serde_json::to_vec(&result)
        .map_err(|_| "CDP response could not be serialized".to_string())?
        .len();
    if result_bytes > MAX_CDP_RESULT_BYTES {
        return Err(format!(
            "CDP response exceeded the {MAX_CDP_RESULT_BYTES} byte tool-output limit"
        ));
    }

    let mut response = json!({
        "success": true,
        "developer_mode": true,
        "approval_required": true,
        "source": "managed_loopback_cdp",
        "target": spec.target,
        "method": spec.method,
        "params_bytes": spec.params_json.len(),
        "params_sha256": hex::encode(Sha256::digest(spec.params_json.as_bytes())),
        "result_bytes": result_bytes,
        "result": result,
        "_summary_text": format!("Executed approved high-risk CDP command {} against the current managed browser {} target.", spec.method, spec.target)
    });
    copy_response_fields(&mut response, &endpoint_result, &["browser_session"]);
    Ok(response)
}

async fn execute_cdp_command(
    endpoint: &str,
    page_url: Option<&str>,
    spec: &BrowserCdpSpec,
) -> Result<Value, String> {
    let config = WebSocketConfig::default()
        .read_buffer_size(32 * 1024)
        .write_buffer_size(16 * 1024)
        .max_write_buffer_size(128 * 1024)
        .max_message_size(Some(CDP_MAX_MESSAGE_BYTES))
        .max_frame_size(Some(CDP_MAX_MESSAGE_BYTES));
    let (mut stream, _) = tokio::time::timeout(
        CDP_CONNECT_TIMEOUT,
        connect_async_with_config(endpoint, Some(config), true),
    )
    .await
    .map_err(|_| "managed browser CDP connection timed out".to_string())?
    .map_err(|_| "managed browser CDP connection failed".to_string())?;

    tokio::time::timeout(Duration::from_secs(spec.timeout_seconds), async {
        if let Some(page_url) = page_url {
            send_cdp_command(&mut stream, 1, "Target.getTargets", json!({}), None).await?;
            let targets = read_cdp_result(&mut stream, 1).await?;
            let target_id = active_page_target_id(&targets, page_url)?;
            send_cdp_command(
                &mut stream,
                2,
                "Target.attachToTarget",
                json!({"targetId": target_id, "flatten": true}),
                None,
            )
            .await?;
            let attached = read_cdp_result(&mut stream, 2).await?;
            let session_id = attached
                .get("sessionId")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty() && value.len() <= 256)
                .ok_or_else(|| "managed browser CDP attachment is malformed".to_string())?;
            send_cdp_command(
                &mut stream,
                3,
                spec.method.as_str(),
                spec.params.clone(),
                Some(session_id),
            )
            .await?;
            read_full_cdp_result(&mut stream, 3).await
        } else {
            send_cdp_command(
                &mut stream,
                1,
                spec.method.as_str(),
                spec.params.clone(),
                None,
            )
            .await?;
            read_full_cdp_result(&mut stream, 1).await
        }
    })
    .await
    .map_err(|_| {
        format!(
            "CDP command timed out after {} seconds",
            spec.timeout_seconds
        )
    })?
}

async fn read_full_cdp_result<S>(
    stream: &mut tokio_tungstenite::WebSocketStream<S>,
    expected_id: u64,
) -> Result<Value, String>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let mut messages_seen = 0_usize;
    while let Some(message) = stream.next().await {
        messages_seen = messages_seen.saturating_add(1);
        if messages_seen > 256 {
            return Err("managed browser CDP emitted too many messages".to_string());
        }
        let message = message.map_err(|_| "managed browser CDP read failed".to_string())?;
        if !message.is_text() {
            continue;
        }
        let text = message
            .to_text()
            .map_err(|_| "managed browser CDP emitted invalid text".to_string())?;
        if text.len() > CDP_MAX_MESSAGE_BYTES {
            return Err("managed browser CDP response exceeded the message limit".to_string());
        }
        let response: Value = serde_json::from_str(text)
            .map_err(|_| "managed browser CDP emitted malformed JSON".to_string())?;
        if response.get("id").and_then(Value::as_u64) != Some(expected_id) {
            continue;
        }
        if let Some(error) = response.get("error") {
            let code = error.get("code").and_then(Value::as_i64);
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("CDP command was rejected");
            return Err(match code {
                Some(code) => format!("CDP command was rejected ({code}): {message}"),
                None => format!("CDP command was rejected: {message}"),
            });
        }
        return response
            .get("result")
            .cloned()
            .ok_or_else(|| "managed browser CDP response is missing its result".to_string());
    }
    Err("managed browser CDP closed before returning a result".to_string())
}

fn parse_cdp_spec(arguments: &Value) -> Result<BrowserCdpSpec, String> {
    let target = match arguments
        .get("target")
        .and_then(Value::as_str)
        .unwrap_or("page")
    {
        "page" => "page",
        "browser" => "browser",
        _ => return Err("target must be page or browser".to_string()),
    };
    let method = arguments
        .get("method")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "method is required".to_string())?;
    validate_cdp_method(method)?;
    let params = arguments
        .get("params")
        .cloned()
        .unwrap_or_else(|| json!({}));
    if !params.is_object() {
        return Err("params must be a JSON object".to_string());
    }
    let params_json = serde_json::to_string(&params)
        .map_err(|_| "CDP params could not be serialized".to_string())?;
    if params_json.len() > MAX_CDP_PARAMS_BYTES {
        return Err(format!(
            "CDP params exceed {MAX_CDP_PARAMS_BYTES} serialized bytes"
        ));
    }
    let timeout_seconds = arguments
        .get("timeout_seconds")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_CDP_TIMEOUT_SECONDS);
    if !(1..=MAX_CDP_TIMEOUT_SECONDS).contains(&timeout_seconds) {
        return Err(format!(
            "timeout_seconds must be between 1 and {MAX_CDP_TIMEOUT_SECONDS}"
        ));
    }
    Ok(BrowserCdpSpec {
        target,
        method: method.to_string(),
        params,
        params_json,
        timeout_seconds,
    })
}

fn validate_cdp_method(method: &str) -> Result<(), String> {
    if method.len() > MAX_CDP_METHOD_CHARS || !method.is_ascii() {
        return Err("CDP method is invalid".to_string());
    }
    let Some((domain, command)) = method.split_once('.') else {
        return Err("CDP method must use Domain.command syntax".to_string());
    };
    if command.contains('.') || !valid_cdp_identifier(domain) || !valid_cdp_identifier(command) {
        return Err("CDP method must use Domain.command syntax".to_string());
    }
    Ok(())
}

fn valid_cdp_identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic())
        && bytes.all(|byte| byte.is_ascii_alphanumeric())
}

fn active_page_url(value: &Value) -> Result<String, String> {
    let raw = value
        .pointer("/data/result")
        .cloned()
        .ok_or_else(|| "active browser page identity is malformed".to_string())?;
    parse_browser_command_eval_payload(raw)
        .get("url")
        .and_then(Value::as_str)
        .filter(|url| !url.is_empty() && url.len() <= 8_192)
        .map(ToOwned::to_owned)
        .ok_or_else(|| "active browser page identity is malformed".to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{browser_cdp_approval_command, parse_cdp_spec, validate_cdp_method};

    #[test]
    fn cdp_contract_requires_exact_domain_command_and_object_params() {
        assert!(validate_cdp_method("Runtime.evaluate").is_ok());
        assert!(validate_cdp_method("Runtime.evaluate.extra").is_err());
        assert!(validate_cdp_method("Runtime/evaluate").is_err());
        assert!(parse_cdp_spec(&json!({
            "method": "Runtime.evaluate",
            "params": []
        }))
        .is_err());
    }

    #[test]
    fn cdp_approval_contains_exact_canonical_params() {
        let (command, args) = browser_cdp_approval_command(&json!({
            "target": "page",
            "method": "Runtime.evaluate",
            "params": {"expression": "document.title"},
            "timeout_seconds": 7
        }))
        .unwrap();
        assert_eq!(command, "browser_cdp_command");
        assert!(args
            .iter()
            .any(|value| value == "{\"expression\":\"document.title\"}"));
        assert_eq!(args.last().map(String::as_str), Some("7"));
    }
}
