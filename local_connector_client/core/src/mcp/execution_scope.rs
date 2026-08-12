// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chatos_mcp_service::TOOL_RESULT_MAX_CHARS_META_KEY;
use chatos_sandbox_contract::{
    PermissionProfileId, SandboxBackendKind, SandboxBackendReadinessStatus,
};
use chrono::Utc;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

use crate::approval::{
    approval_project_key_from_request, ApprovalDecision, CommandApprovalRequest,
    CommandApprovalService,
};
use crate::history::{
    command_history_entry_from_exec_result, CommandExecutionContext, CommandHistoryRecorder,
};
use crate::local_runtime::LocalDatabase;
use crate::mcp::tools::{
    code_maintainer_structured_result, normalize_request_project_relative_path,
    request_project_root,
};
use crate::relay::RelayRequest;
use crate::sandbox::docker::{
    destroy_local_sandbox_container, ensure_docker_running, published_local_sandbox_agent_endpoint,
    start_local_sandbox_container, wait_for_local_sandbox_agent,
};
use crate::sandbox::process::{
    call_native_sandbox_mcp, destroy_native_sandbox_process, native_process_sandbox_capability,
    start_native_sandbox_process,
};
use crate::sandbox::types::{
    LocalSandboxNetworkPolicy, LocalSandboxResourceLimits, LocalSandboxRuntime,
};
use crate::workspace::paths::relative_to_workspace;
use crate::{
    local_now_rfc3339, LocalState, WorkspaceState, DEFAULT_LOCAL_SANDBOX_IMAGE,
    DEFAULT_TERMINAL_EXEC_TIMEOUT_MS, MAX_TERMINAL_EXEC_TIMEOUT_MS,
};

const SESSION_ID_HEADER: &str = "x-mcp-management-session-id";
const SESSION_EXPIRES_AT_UNIX_HEADER: &str = "x-mcp-management-session-expires-at-unix";
const RUN_ID_HEADER: &str = "x-mcp-management-run-id";
const SCOPE_GENERATION_HEADER: &str = "x-mcp-management-execution-scope-generation";
const PROJECT_ID_HEADER: &str = "x-local-connector-project-id";
const EXECUTION_SCOPE_REAPER_INTERVAL: Duration = Duration::from_secs(15);
const EXECUTION_SCOPE_ORPHAN_GRACE_SECONDS: i64 = 60;
const EXECUTION_SCOPE_TOMBSTONE_TTL_SECONDS: i64 = 7 * 24 * 60 * 60;

#[derive(Debug)]
pub(crate) struct LocalExecutionScope {
    pub(crate) scope_id: String,
    generation: i64,
    backend: SandboxBackendKind,
    agent_endpoint: Option<String>,
    agent_token: String,
    active_invocations: AtomicUsize,
    draining: std::sync::atomic::AtomicBool,
    releasing: std::sync::atomic::AtomicBool,
    expires_at_unix: std::sync::atomic::AtomicI64,
    lifecycle_lock: tokio::sync::Mutex<()>,
}

impl LocalExecutionScope {
    async fn begin_invocation(
        self: &Arc<Self>,
        runtime: &LocalSandboxRuntime,
    ) -> Result<LocalExecutionInvocationGuard> {
        let _lifecycle = self.lifecycle_lock.lock().await;
        self.ensure_accepting_invocations()?;
        self.active_invocations.fetch_add(1, Ordering::AcqRel);
        Ok(LocalExecutionInvocationGuard {
            scope: self.clone(),
            runtime: runtime.clone(),
        })
    }

    fn ensure_accepting_invocations(&self) -> Result<()> {
        if self.draining.load(Ordering::Acquire) {
            return Err(anyhow!(
                "local execution scope is draining after run finalization"
            ));
        }
        Ok(())
    }

    fn renew_until(&self, expires_at_unix: i64) {
        self.expires_at_unix
            .fetch_max(expires_at_unix, Ordering::AcqRel);
    }

    fn ready_for_release(&self, now: i64) -> bool {
        if self.active_invocations.load(Ordering::Acquire) != 0 {
            return false;
        }
        self.draining.load(Ordering::Acquire)
            || self
                .expires_at_unix
                .load(Ordering::Acquire)
                .saturating_add(EXECUTION_SCOPE_ORPHAN_GRACE_SECONDS)
                <= now
    }
}

struct LocalExecutionInvocationGuard {
    scope: Arc<LocalExecutionScope>,
    runtime: LocalSandboxRuntime,
}

impl Drop for LocalExecutionInvocationGuard {
    fn drop(&mut self) {
        let previous = self.scope.active_invocations.fetch_sub(1, Ordering::AcqRel);
        if previous == 1 && self.scope.draining.load(Ordering::Acquire) {
            let runtime = self.runtime.clone();
            let scope = self.scope.clone();
            tokio::spawn(async move {
                release_drained_scope(&runtime, &scope).await;
            });
        }
    }
}

pub(crate) async fn call_local_execution_scope_tool(
    request: &RelayRequest,
    state: &LocalState,
    http_client: &reqwest::Client,
    runtime: &LocalSandboxRuntime,
    database: &LocalDatabase,
    project_root: &Path,
    tool_name: &str,
    arguments: Value,
    tool_result_max_chars: Option<usize>,
) -> Result<Value> {
    let scope =
        get_or_create_scope(request, state, http_client, runtime, database, project_root).await?;
    let _invocation = scope.begin_invocation(runtime).await?;
    let request_body = tool_call_body(request, tool_name, arguments, tool_result_max_chars);
    let response = call_scope_mcp(http_client, runtime, &scope, &request_body).await?;
    decode_tool_result(response, request_body.get("id"))
}

pub(crate) async fn finalize_local_execution_scope(
    request: &RelayRequest,
    runtime: &LocalSandboxRuntime,
    database: &LocalDatabase,
) -> Result<Value> {
    let _creation = runtime.execution_scope_creation_lock.lock().await;
    let run_id = relay_header(request, RUN_ID_HEADER)
        .ok_or_else(|| anyhow!("local execution scope finalize is missing run identity"))?;
    let owner_user_id = request
        .owner_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("local execution scope finalize is missing owner identity"))?;
    let project_id = relay_header(request, PROJECT_ID_HEADER)
        .ok_or_else(|| anyhow!("local execution scope finalize is missing project identity"))?;
    let generation = required_scope_generation(request)?;
    let terminal_status = request
        .body
        .pointer("/params/status")
        .and_then(Value::as_str)
        .unwrap_or("failed");
    database
        .persist_execution_scope_tombstone(
            owner_user_id,
            project_id,
            run_id,
            generation,
            terminal_status,
            Utc::now()
                .timestamp()
                .saturating_add(EXECUTION_SCOPE_TOMBSTONE_TTL_SECONDS),
        )
        .await?;
    let matching = runtime
        .execution_scopes
        .read()
        .await
        .iter()
        .filter(|(key, scope)| {
            scope_key_matches_run(key, owner_user_id, project_id, run_id)
                && scope.generation == generation
        })
        .map(|(key, scope)| (key.clone(), scope.clone()))
        .collect::<Vec<_>>();
    let mut released = 0usize;
    for (key, scope) in matching {
        {
            let _lifecycle = scope.lifecycle_lock.lock().await;
            scope.draining.store(true, Ordering::Release);
            scope.expires_at_unix.store(0, Ordering::Release);
        }
        if try_release_scope(runtime, key.as_str(), &scope).await? {
            released = released.saturating_add(1);
        }
    }
    Ok(json!({
        "jsonrpc": "2.0",
        "id": request.body.get("id").cloned().unwrap_or(Value::Null),
        "result": { "ok": true, "released_scopes": released },
    }))
}

fn scope_key_matches_run(key: &str, owner_user_id: &str, project_id: &str, run_id: &str) -> bool {
    let mut parts = key.split('\u{1f}');
    parts.next() == Some(owner_user_id)
        && parts.next() == Some(project_id)
        && parts.next() == Some(run_id)
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn call_local_execution_scope_terminal_tool(
    request: &RelayRequest,
    state: &LocalState,
    http_client: &reqwest::Client,
    runtime: &LocalSandboxRuntime,
    database: &LocalDatabase,
    workspace: &WorkspaceState,
    tool_name: &str,
    arguments: Value,
    tool_result_max_chars: Option<usize>,
    history_recorder: &CommandHistoryRecorder,
) -> Result<Value> {
    let project_root = request_project_root(workspace, request)?;
    let mut arguments = arguments;
    let timeout_ms = arguments
        .get("timeout_ms")
        .or_else(|| arguments.get("max_wait_ms"))
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_TERMINAL_EXEC_TIMEOUT_MS)
        .clamp(1_000, MAX_TERMINAL_EXEC_TIMEOUT_MS);
    let normalized_path = if tool_name == "execute_command" {
        let path = arguments.get("path").and_then(Value::as_str).unwrap_or(".");
        let normalized = normalize_request_project_relative_path(workspace, request, path)?;
        if let Some(map) = arguments.as_object_mut() {
            map.insert("path".to_string(), Value::String(normalized.clone()));
        }
        Some(normalized)
    } else {
        None
    };
    if tool_name == "execute_command" {
        if let Some(command) = terminal_command_text(&arguments) {
            let cwd_label = normalized_path.clone().unwrap_or_else(|| ".".to_string());
            let project_key = approval_project_key_from_request(
                state,
                request,
                workspace,
                relative_to_workspace(workspace, project_root.as_path()),
            );
            let approval = CommandApprovalService::new(
                history_recorder.state_path.clone(),
                history_recorder.state.clone(),
            )
            .approve(CommandApprovalRequest {
                request_id: request.request_id.clone(),
                project_key,
                command: command.clone(),
                args: Vec::new(),
                redact_arguments_in_history: false,
                cwd: cwd_label.clone(),
                source: "local_mcp".to_string(),
                requested_permissions: None,
                session_id: relay_header(request, RUN_ID_HEADER).map(ToOwned::to_owned),
                action_audit: None,
            })
            .await?;
            if let ApprovalDecision::Denied { reason, .. } = approval {
                let body = terminal_approval_denied_body(
                    command.as_str(),
                    cwd_label.as_str(),
                    timeout_ms,
                    reason.as_str(),
                );
                history_recorder
                    .append(command_history_entry_from_exec_result(
                        state,
                        request,
                        &CommandExecutionContext::local_mcp(request, "execute_command"),
                        command.as_str(),
                        &[],
                        cwd_label.as_str(),
                        local_now_rfc3339(),
                        &body,
                    ))
                    .await;
                return Ok(mcp_text_result(body));
            }
        }
    }
    let result = call_local_execution_scope_tool(
        request,
        state,
        http_client,
        runtime,
        database,
        project_root.as_path(),
        tool_name,
        arguments,
        tool_result_max_chars,
    )
    .await?;
    if tool_name != "execute_command" {
        return Ok(result);
    }
    let structured = code_maintainer_structured_result(result.clone());
    let command = structured
        .get("common")
        .or_else(|| structured.get("command"))
        .and_then(Value::as_str)
        .unwrap_or("execute_command");
    let cwd_label = structured
        .get("path")
        .and_then(Value::as_str)
        .or(normalized_path.as_deref())
        .unwrap_or(".");
    let history_body = json!({
        "command": command,
        "args": [],
        "cwd": cwd_label,
        "success": structured.get("success").and_then(Value::as_bool).unwrap_or(false),
        "exit_code": structured.get("exit_code").and_then(Value::as_i64),
        "timed_out": structured.get("timed_out").and_then(Value::as_bool).unwrap_or(false),
        "stdout": structured.get("stdout").or_else(|| structured.get("output")).and_then(Value::as_str).unwrap_or_default(),
        "stderr": structured.get("stderr").and_then(Value::as_str).unwrap_or_default(),
    });
    history_recorder
        .append(command_history_entry_from_exec_result(
            state,
            request,
            &CommandExecutionContext::local_mcp(request, "execute_command"),
            command,
            &[],
            cwd_label,
            local_now_rfc3339(),
            &history_body,
        ))
        .await;
    Ok(result)
}

fn terminal_command_text(arguments: &Value) -> Option<String> {
    arguments
        .get("common")
        .and_then(Value::as_str)
        .or_else(|| arguments.get("command").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn terminal_approval_denied_body(command: &str, cwd: &str, timeout_ms: u64, reason: &str) -> Value {
    json!({
        "command": command,
        "args": [],
        "cwd": cwd,
        "success": false,
        "exit_code": Option::<i32>::None,
        "timed_out": false,
        "timeout_ms": timeout_ms,
        "stdout": "",
        "stderr": "",
        "error": reason,
        "approval_decision": "denied",
        "approval_reason": reason,
    })
}

fn mcp_text_result(payload: Value) -> Value {
    let text = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string());
    json!({
        "content": [{ "type": "text", "text": text }],
        "_structured_result": payload,
    })
}

async fn get_or_create_scope(
    request: &RelayRequest,
    state: &LocalState,
    http_client: &reqwest::Client,
    runtime: &LocalSandboxRuntime,
    database: &LocalDatabase,
    project_root: &Path,
) -> Result<Arc<LocalExecutionScope>> {
    let identity = execution_scope_identity(request, project_root)?;
    if identity.run_id.is_some()
        && database
            .execution_scope_is_terminal(
                identity.owner_user_id.as_str(),
                identity.project_id.as_str(),
                identity.run_id.as_deref().unwrap_or_default(),
                identity.generation,
            )
            .await?
    {
        return Err(anyhow!("local execution scope is terminal"));
    }
    if let Some(scope) = runtime
        .execution_scopes
        .read()
        .await
        .get(&identity.key)
        .cloned()
    {
        scope.ensure_accepting_invocations()?;
        scope.renew_until(identity.expires_at_unix);
        return Ok(scope);
    }

    let _creation = runtime.execution_scope_creation_lock.lock().await;
    if identity.run_id.is_some()
        && database
            .execution_scope_is_terminal(
                identity.owner_user_id.as_str(),
                identity.project_id.as_str(),
                identity.run_id.as_deref().unwrap_or_default(),
                identity.generation,
            )
            .await?
    {
        return Err(anyhow!("local execution scope is terminal"));
    }
    if let Some(scope) = runtime
        .execution_scopes
        .read()
        .await
        .get(&identity.key)
        .cloned()
    {
        scope.ensure_accepting_invocations()?;
        scope.renew_until(identity.expires_at_unix);
        return Ok(scope);
    }

    let backend = state.sandbox.default_backend;
    let policy = state.sandbox.effective_policy_defaults();
    let permissions = state.sandbox.effective_permissions(
        None,
        &policy,
        vec![project_root.to_string_lossy().to_string()],
    );
    let limits = LocalSandboxResourceLimits::default();
    let agent_token = format!("scope-token-{}", uuid::Uuid::new_v4().simple());
    let agent_endpoint = match backend {
        SandboxBackendKind::LocalProcess => {
            let capability = native_process_sandbox_capability().await;
            if capability.status != SandboxBackendReadinessStatus::Ready {
                return Err(anyhow!(capability.message));
            }
            start_native_sandbox_process(
                runtime,
                identity.scope_id.as_str(),
                project_root,
                &policy,
                &permissions,
                &limits,
                identity.project_id.as_str(),
                identity.owner_user_id.as_str(),
            )
            .await?;
            None
        }
        SandboxBackendKind::Docker => {
            ensure_docker_running().await?;
            let image_ref = state
                .sandbox
                .selected_image_ref
                .clone()
                .unwrap_or_else(|| DEFAULT_LOCAL_SANDBOX_IMAGE.to_string());
            let network = LocalSandboxNetworkPolicy {
                mode: if policy.permission_profile_id == PermissionProfileId::FullAccess {
                    "bridge".to_string()
                } else {
                    "none".to_string()
                },
            };
            start_local_sandbox_container(
                identity.scope_id.as_str(),
                project_root,
                image_ref.as_str(),
                agent_token.as_str(),
                &limits,
                &network,
                policy.permission_profile_id,
            )
            .await?;
            let Some(endpoint) =
                published_local_sandbox_agent_endpoint(identity.scope_id.as_str()).await
            else {
                let _ = destroy_local_sandbox_container(identity.scope_id.as_str()).await;
                return Err(anyhow!(
                    "local Docker execution agent port was not published"
                ));
            };
            if let Err(error) = wait_for_local_sandbox_agent(http_client, endpoint.as_str()).await {
                let _ = destroy_local_sandbox_container(identity.scope_id.as_str()).await;
                return Err(error);
            }
            Some(endpoint)
        }
    };
    let scope = Arc::new(LocalExecutionScope {
        scope_id: identity.scope_id,
        generation: identity.generation,
        backend,
        agent_endpoint,
        agent_token,
        active_invocations: AtomicUsize::new(0),
        draining: std::sync::atomic::AtomicBool::new(false),
        releasing: std::sync::atomic::AtomicBool::new(false),
        expires_at_unix: std::sync::atomic::AtomicI64::new(identity.expires_at_unix),
        lifecycle_lock: tokio::sync::Mutex::new(()),
    });
    runtime
        .execution_scopes
        .write()
        .await
        .insert(identity.key, scope.clone());
    Ok(scope)
}

struct ExecutionScopeIdentity {
    key: String,
    scope_id: String,
    owner_user_id: String,
    project_id: String,
    run_id: Option<String>,
    generation: i64,
    expires_at_unix: i64,
}

fn execution_scope_identity(
    request: &RelayRequest,
    project_root: &Path,
) -> Result<ExecutionScopeIdentity> {
    let owner_user_id = request
        .owner_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("local execution scope is missing owner identity"))?
        .to_string();
    let project_id = relay_header(request, PROJECT_ID_HEADER)
        .ok_or_else(|| anyhow!("local execution scope is missing project identity"))?
        .to_string();
    let session_id = relay_header(request, SESSION_ID_HEADER)
        .ok_or_else(|| anyhow!("local execution scope is missing runtime session identity"))?;
    let run_id = relay_header(request, RUN_ID_HEADER).map(ToOwned::to_owned);
    let run_or_session = run_id.as_deref().unwrap_or(session_id);
    let generation = if run_id.is_some() {
        required_scope_generation(request)?
    } else {
        1
    };
    let expires_at_unix = relay_header(request, SESSION_EXPIRES_AT_UNIX_HEADER)
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > Utc::now().timestamp())
        .ok_or_else(|| anyhow!("local execution scope has an invalid session expiry"))?;
    let canonical_root = project_root
        .canonicalize()
        .context("canonicalize local execution scope project root")?;
    let key = format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}",
        owner_user_id, project_id, run_or_session, generation
    );
    let digest_material = format!(
        "{}\u{1f}{}\u{1f}{}",
        key,
        generation,
        canonical_root.to_string_lossy()
    );
    let digest = hex::encode(Sha256::digest(digest_material.as_bytes()));
    Ok(ExecutionScopeIdentity {
        key,
        scope_id: format!("mcp-scope-{}", &digest[..24]),
        owner_user_id,
        project_id,
        run_id,
        generation,
        expires_at_unix,
    })
}

fn required_scope_generation(request: &RelayRequest) -> Result<i64> {
    relay_header(request, SCOPE_GENERATION_HEADER)
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| anyhow!("local execution scope is missing a valid generation"))
}

fn tool_call_body(
    request: &RelayRequest,
    tool_name: &str,
    arguments: Value,
    tool_result_max_chars: Option<usize>,
) -> Value {
    let mut params = json!({ "name": tool_name, "arguments": arguments });
    if let Some(max_chars) = tool_result_max_chars {
        params["_meta"] = json!({ TOOL_RESULT_MAX_CHARS_META_KEY: max_chars });
    }
    json!({
        "jsonrpc": "2.0",
        "id": request.body.get("id").cloned().unwrap_or_else(|| json!(request.request_id)),
        "method": "tools/call",
        "params": params,
    })
}

async fn call_scope_mcp(
    http_client: &reqwest::Client,
    runtime: &LocalSandboxRuntime,
    scope: &LocalExecutionScope,
    request: &Value,
) -> Result<Value> {
    match scope.backend {
        SandboxBackendKind::LocalProcess => {
            call_native_sandbox_mcp(runtime, scope.scope_id.as_str(), request).await
        }
        SandboxBackendKind::Docker => {
            let endpoint = scope
                .agent_endpoint
                .as_deref()
                .ok_or_else(|| anyhow!("local Docker execution agent endpoint is unavailable"))?;
            let response = http_client
                .post(format!("{}/mcp", endpoint.trim_end_matches('/')))
                .bearer_auth(scope.agent_token.as_str())
                .json(request)
                .send()
                .await
                .context("call local Docker execution agent")?;
            let status = response.status();
            let body = response
                .json::<Value>()
                .await
                .context("decode local Docker execution agent response")?;
            if !status.is_success() {
                return Err(anyhow!(
                    "local Docker execution agent returned HTTP {status}: {body}"
                ));
            }
            Ok(body)
        }
    }
}

fn decode_tool_result(response: Value, expected_id: Option<&Value>) -> Result<Value> {
    if response.get("id") != expected_id {
        return Err(anyhow!(
            "local execution agent response id does not match the invocation"
        ));
    }
    if let Some(error) = response.get("error") {
        return Err(anyhow!("local execution agent tool call failed: {error}"));
    }
    response
        .get("result")
        .cloned()
        .ok_or_else(|| anyhow!("local execution agent response is missing result"))
}

fn relay_header<'a>(request: &'a RelayRequest, name: &str) -> Option<&'a str> {
    request
        .headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(crate) fn spawn_local_execution_scope_reaper(runtime: LocalSandboxRuntime) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(EXECUTION_SCOPE_REAPER_INTERVAL);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            reap_expired_execution_scopes(&runtime).await;
        }
    })
}

async fn reap_expired_execution_scopes(runtime: &LocalSandboxRuntime) {
    let now = Utc::now().timestamp();
    let expired = runtime
        .execution_scopes
        .read()
        .await
        .iter()
        .filter(|(_, scope)| scope.ready_for_release(now))
        .map(|(key, scope)| (key.clone(), scope.clone()))
        .collect::<Vec<_>>();
    for (key, scope) in expired {
        let result = try_release_scope(runtime, key.as_str(), &scope).await;
        if let Err(error) = result {
            crate::tracing_stdout(
                format!(
                    "release expired local MCP execution scope {} failed: {error:#}",
                    scope.scope_id
                )
                .as_str(),
            );
        }
    }
}

async fn release_scope(runtime: &LocalSandboxRuntime, scope: &LocalExecutionScope) -> Result<()> {
    match scope.backend {
        SandboxBackendKind::Docker => {
            destroy_local_sandbox_container(scope.scope_id.as_str()).await
        }
        SandboxBackendKind::LocalProcess => {
            destroy_native_sandbox_process(runtime, scope.scope_id.as_str()).await
        }
    }
}

async fn release_drained_scope(runtime: &LocalSandboxRuntime, scope: &Arc<LocalExecutionScope>) {
    let key = runtime
        .execution_scopes
        .read()
        .await
        .iter()
        .find(|(_, current)| Arc::ptr_eq(current, scope))
        .map(|(key, _)| key.clone());
    if let Some(key) = key {
        if let Err(error) = try_release_scope(runtime, key.as_str(), scope).await {
            crate::tracing_stdout(
                format!(
                    "release drained local MCP execution scope {} failed: {error:#}",
                    scope.scope_id
                )
                .as_str(),
            );
        }
    }
}

async fn try_release_scope(
    runtime: &LocalSandboxRuntime,
    key: &str,
    scope: &Arc<LocalExecutionScope>,
) -> Result<bool> {
    let _lifecycle = scope.lifecycle_lock.lock().await;
    if scope.active_invocations.load(Ordering::Acquire) != 0 {
        return Ok(false);
    }
    scope.draining.store(true, Ordering::Release);
    if scope
        .releasing
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Ok(false);
    }
    if scope.active_invocations.load(Ordering::Acquire) != 0 {
        scope.releasing.store(false, Ordering::Release);
        return Ok(false);
    }
    if let Err(error) = release_scope(runtime, scope).await {
        scope.releasing.store(false, Ordering::Release);
        return Err(error);
    }
    let mut scopes = runtime.execution_scopes.write().await;
    if scopes
        .get(key)
        .is_some_and(|current| Arc::ptr_eq(current, scope))
    {
        scopes.remove(key);
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs;

    fn request(_root: &Path, run_id: Option<&str>) -> RelayRequest {
        let mut headers = BTreeMap::from([
            (PROJECT_ID_HEADER.to_string(), "project-1".to_string()),
            (SESSION_ID_HEADER.to_string(), "session-1".to_string()),
            (
                SESSION_EXPIRES_AT_UNIX_HEADER.to_string(),
                (Utc::now().timestamp() + 300).to_string(),
            ),
        ]);
        if let Some(run_id) = run_id {
            headers.insert(RUN_ID_HEADER.to_string(), run_id.to_string());
            headers.insert(SCOPE_GENERATION_HEADER.to_string(), "1".to_string());
        }
        RelayRequest {
            _message_type: "mcp".to_string(),
            request_id: "request-1".to_string(),
            owner_user_id: Some("user-1".to_string()),
            device_id: Some("device-1".to_string()),
            workspace_id: "workspace-1".to_string(),
            method: Some("POST".to_string()),
            path: Some("/mcp".to_string()),
            headers,
            body: json!({}),
            platform_signature: None,
            platform_signature_key_id: None,
            platform_signature_alg: None,
            platform_timestamp: None,
            platform_nonce: None,
        }
    }

    #[test]
    fn run_identity_is_stable_across_runtime_sessions() {
        let root = tempfile::tempdir().unwrap();
        let first = request(root.path(), Some("run-1"));
        let mut second = request(root.path(), Some("run-1"));
        second
            .headers
            .insert(SESSION_ID_HEADER.to_string(), "session-2".to_string());
        assert_eq!(
            execution_scope_identity(&first, root.path()).unwrap().key,
            execution_scope_identity(&second, root.path()).unwrap().key
        );
    }

    #[test]
    fn session_identity_is_used_when_run_is_absent() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("probe.txt"), "ok").unwrap();
        let first = request(root.path(), None);
        let mut second = request(root.path(), None);
        second
            .headers
            .insert(SESSION_ID_HEADER.to_string(), "session-2".to_string());
        assert_ne!(
            execution_scope_identity(&first, root.path()).unwrap().key,
            execution_scope_identity(&second, root.path()).unwrap().key
        );
    }

    #[test]
    fn generation_separates_recreated_run_scopes() {
        let root = tempfile::tempdir().unwrap();
        let first = request(root.path(), Some("run-1"));
        let mut recreated = request(root.path(), Some("run-1"));
        recreated
            .headers
            .insert(SCOPE_GENERATION_HEADER.to_string(), "2".to_string());
        let first = execution_scope_identity(&first, root.path()).unwrap();
        let recreated = execution_scope_identity(&recreated, root.path()).unwrap();
        assert_ne!(first.key, recreated.key);
        assert_ne!(first.scope_id, recreated.scope_id);
    }

    #[tokio::test]
    async fn draining_scope_rejects_new_invocations_without_leaking_a_reference() {
        let runtime = LocalSandboxRuntime::default();
        let scope = Arc::new(LocalExecutionScope {
            scope_id: "scope-1".to_string(),
            generation: 1,
            backend: SandboxBackendKind::LocalProcess,
            agent_endpoint: None,
            agent_token: "token".to_string(),
            active_invocations: AtomicUsize::new(0),
            draining: std::sync::atomic::AtomicBool::new(true),
            releasing: std::sync::atomic::AtomicBool::new(false),
            expires_at_unix: std::sync::atomic::AtomicI64::new(0),
            lifecycle_lock: tokio::sync::Mutex::new(()),
        });
        assert!(scope.begin_invocation(&runtime).await.is_err());
        assert_eq!(scope.active_invocations.load(Ordering::Acquire), 0);
    }
}
