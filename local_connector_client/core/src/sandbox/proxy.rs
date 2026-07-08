// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use anyhow::{anyhow, Context, Result};
use serde_json::Value;

use crate::history::{
    command_history_entry_for_sandbox_tool_call, sandbox_tool_call_details,
    CommandExecutionContext, CommandHistoryRecorder,
};
use crate::relay::RelayRequest;
use crate::sandbox::types::{LocalSandboxLease, LocalSandboxRuntime};
use crate::{local_now_rfc3339, LocalState};

pub(crate) async fn proxy_local_sandbox_mcp(
    request: &RelayRequest,
    state: &LocalState,
    http_client: &reqwest::Client,
    sandbox_runtime: &LocalSandboxRuntime,
    sandbox_id: &str,
    history_recorder: &CommandHistoryRecorder,
) -> Result<(u16, BTreeMap<String, String>, Value)> {
    let started_at = local_now_rfc3339();
    let tool_call = sandbox_tool_call_details(&request.body);
    let lease = require_local_sandbox_lease(sandbox_runtime, sandbox_id).await?;
    let endpoint = require_local_sandbox_agent_endpoint(&lease)?;
    let response = http_client
        .post(format!("{}/mcp", endpoint.trim_end_matches('/')))
        .bearer_auth(lease.agent_token.as_str())
        .json(&request.body)
        .send()
        .await
        .context("proxy local sandbox mcp request")?;
    let result = local_sandbox_http_response(response).await?;
    if let Some(tool_call) = tool_call {
        history_recorder
            .append(command_history_entry_for_sandbox_tool_call(
                state,
                request,
                &CommandExecutionContext::task_runner_sandbox(
                    request,
                    sandbox_id,
                    tool_call.tool_name.as_str(),
                ),
                tool_call,
                result.0,
                &result.2,
                started_at,
            ))
            .await;
    }
    Ok(result)
}

async fn local_sandbox_http_response(
    response: reqwest::Response,
) -> Result<(u16, BTreeMap<String, String>, Value)> {
    let status = response.status().as_u16();
    let headers = sandbox_response_headers(response.headers());
    let bytes = response
        .bytes()
        .await
        .context("read local sandbox response")?;
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice::<Value>(bytes.as_ref())
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(bytes.as_ref()).into_owned()))
    };
    Ok((status, headers, body))
}

async fn require_local_sandbox_lease(
    sandbox_runtime: &LocalSandboxRuntime,
    sandbox_id: &str,
) -> Result<LocalSandboxLease> {
    sandbox_runtime
        .leases
        .read()
        .await
        .get(sandbox_id)
        .cloned()
        .ok_or_else(|| anyhow!("sandbox not found"))
}

fn require_local_sandbox_agent_endpoint(lease: &LocalSandboxLease) -> Result<String> {
    lease
        .agent_endpoint
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("local sandbox agent endpoint is not ready"))
}

fn sandbox_response_headers(headers: &reqwest::header::HeaderMap) -> BTreeMap<String, String> {
    headers
        .iter()
        .filter_map(|(key, value)| {
            let key = key.as_str().to_ascii_lowercase();
            if matches!(
                key.as_str(),
                "set-cookie" | "transfer-encoding" | "connection"
            ) {
                return None;
            }
            value.to_str().ok().map(|value| (key, value.to_string()))
        })
        .collect()
}
