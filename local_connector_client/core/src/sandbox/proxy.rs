// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use chatos_sandbox_contract::{ApprovalPolicy, ApprovalReviewer};
use serde_json::json;
use serde_json::Value;

use crate::approval::{
    approval_project_key_from_request, ApprovalDecision, ApprovalMode, CommandApprovalRequest,
    CommandApprovalService,
};
use crate::history::{
    command_history_entry_for_sandbox_tool_call, sandbox_tool_call_details,
    CommandExecutionContext, CommandHistoryRecorder, SandboxToolCallDetails,
};
use crate::relay::RelayRequest;
use crate::sandbox::types::{LocalSandboxLease, LocalSandboxRuntime};
use crate::workspace::paths::relative_to_workspace;
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
    if let Some(tool_call) = tool_call.as_ref() {
        if let Some(response) = approve_sandbox_tool_call(
            request,
            state,
            &lease,
            sandbox_id,
            history_recorder,
            tool_call,
            started_at.as_str(),
        )
        .await?
        {
            return Ok(response);
        }
    }
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

async fn approve_sandbox_tool_call(
    request: &RelayRequest,
    state: &LocalState,
    lease: &LocalSandboxLease,
    sandbox_id: &str,
    history_recorder: &CommandHistoryRecorder,
    tool_call: &SandboxToolCallDetails,
    started_at: &str,
) -> Result<Option<(u16, BTreeMap<String, String>, Value)>> {
    if !tool_call.requires_approval {
        return Ok(None);
    }
    let Some(mode) = approval_mode_for_lease(lease) else {
        return Ok(None);
    };
    let workspace = state
        .workspace_by_id(request.workspace_id.as_str())
        .ok_or_else(|| anyhow!("workspace not found for sandbox command approval"))?;
    let project_root_relative_path =
        relative_to_workspace(workspace, Path::new(lease.workspace_root.as_str()));
    let project_key =
        approval_project_key_from_request(state, request, workspace, project_root_relative_path);
    let cwd = tool_call.cwd.clone().unwrap_or_else(|| ".".to_string());
    let approval = CommandApprovalService::new(
        history_recorder.state_path.clone(),
        history_recorder.state.clone(),
    )
    .approve_with_mode(
        CommandApprovalRequest {
            request_id: request.request_id.clone(),
            project_key,
            command: tool_call.command.clone(),
            args: tool_call.args.clone(),
            cwd: cwd.clone(),
            source: "task_runner_sandbox".to_string(),
        },
        mode,
    )
    .await?;
    let ApprovalDecision::Denied { reason, .. } = approval else {
        return Ok(None);
    };
    let denied =
        approval_denied_sandbox_body(tool_call.command.as_str(), cwd.as_str(), reason.as_str());
    let response_body = sandbox_mcp_text_response(&request.body, denied);
    history_recorder
        .append(command_history_entry_for_sandbox_tool_call(
            state,
            request,
            &CommandExecutionContext::task_runner_sandbox(
                request,
                sandbox_id,
                tool_call.tool_name.as_str(),
            ),
            tool_call.clone(),
            200,
            &response_body,
            started_at.to_string(),
        ))
        .await;
    Ok(Some((200, BTreeMap::new(), response_body)))
}

fn approval_mode_for_lease(lease: &LocalSandboxLease) -> Option<ApprovalMode> {
    match lease.effective_policy.approval_policy {
        ApprovalPolicy::Never => None,
        ApprovalPolicy::OnRequest => match lease.effective_policy.approval_reviewer {
            ApprovalReviewer::AutoReview => Some(ApprovalMode::AutoApproval),
            ApprovalReviewer::User => Some(ApprovalMode::RequestApproval),
        },
    }
}

fn approval_denied_sandbox_body(command: &str, cwd: &str, reason: &str) -> Value {
    json!({
        "command": command,
        "args": [],
        "cwd": cwd,
        "success": false,
        "exit_code": Option::<i32>::None,
        "timed_out": false,
        "stdout": "",
        "stderr": "",
        "error": reason,
        "approval_decision": "denied",
        "approval_reason": reason,
    })
}

fn sandbox_mcp_text_response(request_body: &Value, payload: Value) -> Value {
    let text = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string());
    let result = json!({
        "content": [
            { "type": "text", "text": text }
        ],
        "_structured_result": payload,
    });
    if request_body.get("jsonrpc").is_some() || request_body.get("id").is_some() {
        json!({
            "jsonrpc": "2.0",
            "id": request_body.get("id").cloned().unwrap_or(Value::Null),
            "result": result,
        })
    } else {
        result
    }
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
