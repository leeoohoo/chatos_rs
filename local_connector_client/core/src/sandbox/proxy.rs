// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use chatos_sandbox_contract::{
    ApprovalPolicy, ApprovalReviewer, FileSystemPermissionPolicy, GrantedPermissionProfile,
    NetworkPermissionPolicy, RequestPermissionProfile, SandboxBackendKind,
};
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
use crate::sandbox::lease::local_sandbox_lease_expired;
use crate::sandbox::types::{LocalSandboxLease, LocalSandboxRuntime};
use crate::workspace::paths::relative_to_workspace;
use crate::{local_now_rfc3339, LocalState};

pub(crate) async fn proxy_local_sandbox_mcp(
    request: &RelayRequest,
    state: &LocalState,
    _http_client: &reqwest::Client,
    sandbox_runtime: &LocalSandboxRuntime,
    runtime_id: &str,
    history_recorder: &CommandHistoryRecorder,
) -> Result<(u16, BTreeMap<String, String>, Value)> {
    let started_at = local_now_rfc3339();
    let tool_call = match sandbox_tool_call_details(&request.body) {
        Ok(tool_call) => tool_call,
        Err(reason) => {
            let denied = approval_denied_sandbox_body("permission_request", ".", reason.as_str());
            return Ok((
                200,
                BTreeMap::new(),
                sandbox_mcp_text_response(&request.body, denied),
            ));
        }
    };
    let lease = require_local_sandbox_lease(sandbox_runtime, runtime_id).await?;
    validate_mcp_management_lease_identity(request, &lease)?;
    let mut forwarded_body = request.body.clone();
    if let Some(tool_call) = tool_call.as_ref() {
        if let Some(response) = approve_sandbox_tool_call(
            request,
            state,
            &lease,
            history_recorder,
            tool_call,
            started_at.as_str(),
            &mut forwarded_body,
        )
        .await?
        {
            return Ok(response);
        }
    }
    let mut direct_request = request.clone();
    direct_request.body = forwarded_body;
    direct_request
        .headers
        .entry(chatos_mcp_service::LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER.to_string())
        .or_insert_with(|| "CodeMaintainerRead,CodeMaintainerWrite,TerminalController".to_string());
    let result = (
        200,
        BTreeMap::new(),
        crate::mcp::service::handle_standard_local_mcp_body(
            &direct_request,
            state,
            history_recorder,
        )
        .await?,
    );
    if let Some(tool_call) = tool_call {
        history_recorder
            .append(command_history_entry_for_sandbox_tool_call(
                state,
                request,
                &CommandExecutionContext::task_runner_lease(
                    request,
                    lease.id.as_str(),
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

fn validate_mcp_management_lease_identity(
    request: &RelayRequest,
    lease: &LocalSandboxLease,
) -> Result<()> {
    let owner_user_id = request
        .owner_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("MCP request is missing owner identity"))?;
    let lease_id = required_relay_header(request, "x-chatos-lease-id")?;
    let project_id = required_relay_header(request, "x-mcp-management-project-id")?;
    let run_id = required_relay_header(request, "x-mcp-management-run-id")?;
    validate_lease_identity(
        owner_user_id,
        lease_id,
        project_id,
        run_id,
        lease.tenant_id.as_str(),
        lease.id.as_str(),
        lease.project_id.as_str(),
        lease.run_id.as_str(),
    )
}

#[allow(clippy::too_many_arguments)]
fn validate_lease_identity(
    owner_user_id: &str,
    lease_id: &str,
    project_id: &str,
    run_id: &str,
    expected_owner_user_id: &str,
    expected_lease_id: &str,
    expected_project_id: &str,
    expected_run_id: &str,
) -> Result<()> {
    if owner_user_id != expected_owner_user_id
        || lease_id != expected_lease_id
        || project_id != expected_project_id
        || run_id != expected_run_id
    {
        return Err(anyhow!(
            "MCP request identity does not match the local lease"
        ));
    }
    Ok(())
}

fn required_relay_header<'a>(request: &'a RelayRequest, name: &str) -> Result<&'a str> {
    request
        .headers
        .get(name)
        .or_else(|| {
            request
                .headers
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| value)
        })
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("MCP request is missing required header {name}"))
}

async fn approve_sandbox_tool_call(
    request: &RelayRequest,
    state: &LocalState,
    lease: &LocalSandboxLease,
    history_recorder: &CommandHistoryRecorder,
    tool_call: &SandboxToolCallDetails,
    started_at: &str,
    forwarded_body: &mut Value,
) -> Result<Option<(u16, BTreeMap<String, String>, Value)>> {
    if !tool_call.requires_approval {
        return Ok(None);
    }
    let Some(requested_permissions) = tool_call.requested_permissions.clone() else {
        return Ok(Some(denied_sandbox_tool_response(
            request,
            tool_call,
            "permission elevation request is missing".to_string(),
        )));
    };
    if effective_permissions_cover_request(lease, &requested_permissions) {
        remove_permission_control_fields(forwarded_body);
        return Ok(None);
    }
    if lease.effective_policy.sandbox_mode != SandboxBackendKind::LocalProcess {
        return Ok(Some(denied_sandbox_tool_response(
            request,
            tool_call,
            "temporary permission overlays are not supported by this execution backend".to_string(),
        )));
    }
    let Some(mode) = approval_mode_for_lease(lease) else {
        return Ok(Some(denied_sandbox_tool_response(
            request,
            tool_call,
            "approval policy forbids temporary permission elevation".to_string(),
        )));
    };
    let workspace = state
        .workspace_by_id(request.workspace_id.as_str())
        .ok_or_else(|| anyhow!("workspace not found for command approval"))?;
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
            redact_arguments_in_history: false,
            cwd: cwd.clone(),
            source: "task_runner_lease".to_string(),
            requested_permissions: Some(requested_permissions.clone()),
            session_id: Some(lease.id.clone()),
            action_audit: None,
        },
        mode,
    )
    .await?;
    let granted_permissions = match approval {
        ApprovalDecision::Approved {
            granted_permissions: Some(granted_permissions),
            ..
        } => granted_permissions,
        ApprovalDecision::Approved {
            granted_permissions: None,
            ..
        } => {
            return Ok(Some(denied_sandbox_tool_response(
                request,
                tool_call,
                "approval did not include a permission grant".to_string(),
            )));
        }
        ApprovalDecision::Denied { reason, .. } => {
            let denied = approval_denied_sandbox_body(
                tool_call.command.as_str(),
                cwd.as_str(),
                reason.as_str(),
            );
            let response_body = sandbox_mcp_text_response(&request.body, denied);
            history_recorder
                .append(command_history_entry_for_sandbox_tool_call(
                    state,
                    request,
                    &CommandExecutionContext::task_runner_lease(
                        request,
                        lease.id.as_str(),
                        tool_call.tool_name.as_str(),
                    ),
                    tool_call.clone(),
                    200,
                    &response_body,
                    started_at.to_string(),
                ))
                .await;
            return Ok(Some((200, BTreeMap::new(), response_body)));
        }
    };
    if !requested_permissions.allows_grant(&granted_permissions) {
        return Ok(Some(denied_sandbox_tool_response(
            request,
            tool_call,
            "approved permission grant exceeded the command request".to_string(),
        )));
    }
    install_granted_permissions(forwarded_body, &granted_permissions)?;
    Ok(None)
}

fn denied_sandbox_tool_response(
    request: &RelayRequest,
    tool_call: &SandboxToolCallDetails,
    reason: String,
) -> (u16, BTreeMap<String, String>, Value) {
    let cwd = tool_call.cwd.as_deref().unwrap_or(".");
    let denied = approval_denied_sandbox_body(tool_call.command.as_str(), cwd, reason.as_str());
    (
        200,
        BTreeMap::new(),
        sandbox_mcp_text_response(&request.body, denied),
    )
}

fn command_arguments_mut(body: &mut Value) -> Option<&mut serde_json::Map<String, Value>> {
    if body.get("method").and_then(Value::as_str) == Some("tools/call") {
        return body
            .get_mut("params")?
            .get_mut("arguments")?
            .as_object_mut();
    }
    body.get_mut("arguments")?.as_object_mut()
}

fn remove_permission_control_fields(body: &mut Value) {
    if let Some(arguments) = command_arguments_mut(body) {
        arguments.remove("additionalPermissions");
        arguments.remove("_grantedPermissions");
    }
}

fn install_granted_permissions(
    body: &mut Value,
    granted_permissions: &GrantedPermissionProfile,
) -> Result<()> {
    let arguments =
        command_arguments_mut(body).ok_or_else(|| anyhow!("command arguments are unavailable"))?;
    arguments.remove("_grantedPermissions");
    arguments.insert(
        "_grantedPermissions".to_string(),
        serde_json::to_value(granted_permissions).context("encode granted permission overlay")?,
    );
    Ok(())
}

fn effective_permissions_cover_request(
    lease: &LocalSandboxLease,
    requested_permissions: &RequestPermissionProfile,
) -> bool {
    let file_system_covered =
        requested_permissions
            .file_system
            .as_ref()
            .is_none_or(|file_system| {
                file_system.is_empty()
                    || matches!(
                        lease.effective_permissions.file_system,
                        FileSystemPermissionPolicy::Unrestricted
                    )
            });
    let network_covered = requested_permissions
        .network
        .as_ref()
        .and_then(|network| network.enabled)
        != Some(true)
        || match &lease.effective_permissions.network {
            NetworkPermissionPolicy::Unrestricted => true,
            NetworkPermissionPolicy::Restricted { requirements } => {
                requirements.enabled == Some(true)
            }
        };
    file_system_covered && network_covered
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

async fn require_local_sandbox_lease(
    sandbox_runtime: &LocalSandboxRuntime,
    runtime_id: &str,
) -> Result<LocalSandboxLease> {
    let lease = sandbox_runtime
        .leases
        .read()
        .await
        .get(runtime_id)
        .cloned()
        .ok_or_else(|| anyhow!("lease not found"))?;
    if lease.status == crate::LOCAL_SANDBOX_STATUS_DESTROYED {
        return Err(anyhow!("lease is destroyed"));
    }
    if local_sandbox_lease_expired(&lease) {
        return Err(anyhow!("lease has expired"));
    }
    Ok(lease)
}

#[cfg(test)]
mod identity_tests {
    use super::*;

    #[test]
    fn mcp_identity_is_bound_to_owner_lease_project_and_run() {
        validate_lease_identity(
            "user-1",
            "lease-1",
            "project-1",
            "run-1",
            "user-1",
            "lease-1",
            "project-1",
            "run-1",
        )
        .expect("exact identity must be accepted");

        for actual in [
            ("user-2", "lease-1", "project-1", "run-1"),
            ("user-1", "lease-2", "project-1", "run-1"),
            ("user-1", "lease-1", "project-2", "run-1"),
            ("user-1", "lease-1", "project-1", "run-2"),
        ] {
            assert!(validate_lease_identity(
                actual.0,
                actual.1,
                actual.2,
                actual.3,
                "user-1",
                "lease-1",
                "project-1",
                "run-1",
            )
            .is_err());
        }
    }
}
