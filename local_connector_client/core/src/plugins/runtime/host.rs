// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use anyhow::{Context, Result};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use chatos_plugin_management_sdk::{
    PluginArtifactCreateRequest, PluginArtifactListRequest, PluginArtifactReadMode,
    PluginArtifactReadRequest, PluginArtifactUiAccess, PluginArtifactUpdateRequest,
    PluginComponentKind, PluginHookEvent, PluginHookEventContext, PluginHookOutcome,
    PluginUiAssetReadResponse, PluginUiSnapshot, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE,
    PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_DOWNLOAD, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_LIST,
    PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_READ, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE,
};
use chatos_sandbox_contract::{GrantedPermissionProfile, PermissionGrantScope};
use chrono::Utc;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex as AsyncMutex, RwLock};
use uuid::Uuid;

use super::artifact_store::{PluginArtifactProducer, PluginArtifactStore, PluginUiArtifactGrant};
use super::hook_loader::PluginHookWorkspaceWriteDecision;
use super::mcp_runtime::{PluginMcpAdapter, PluginMcpInvocationCancelOutcome, PreparedPluginMcp};
use super::protocol::*;
use super::telemetry::{
    sanitize_error, PluginRuntimeTelemetryIdentity, PluginRuntimeTelemetryPhase,
    PluginRuntimeTelemetryState,
};
use super::{
    PluginAgentLoader, PluginAgentSnapshot, PluginCommandLoader, PluginCommandSnapshot,
    PluginHookLoader, PluginHookSetSnapshot, PluginNativeSkillBindingSnapshot, PluginSkillLoader,
    PluginSkillSnapshot, PluginUiLoader,
};
use crate::approval::{
    approval_project_key_for_relay_scope, cancel_pending_approvals_for_session,
    clear_session_approvals, ApprovalActionAudit, ApprovalActionAuditDetail, ApprovalDecision,
    CommandApprovalRequest, CommandApprovalService,
};
use crate::relay::RelayRequest;
use crate::LocalState;

const PLUGIN_SESSION_TTL_SECONDS: i64 = 2 * 60 * 60;
const LOAD_SKILL_RESOURCE_OPERATION: &str = "load_skill_resource";
const NATIVE_SKILL_TOOL_CALL_OPERATION: &str = "native_skill_tool_call";
const COMMAND_INVOKE_OPERATION: &str = "command_invoke";
const AGENT_APPLY_OPERATION: &str = "agent_apply";

mod artifact_ui;
mod execution;
mod plugin_lifecycle;
mod prepare;
mod support;

use support::*;

#[derive(Debug, Clone)]
pub struct PluginRuntimeHost {
    skill_loader: PluginSkillLoader,
    agent_loader: PluginAgentLoader,
    command_loader: PluginCommandLoader,
    hook_loader: PluginHookLoader,
    ui_loader: PluginUiLoader,
    mcp_adapter: PluginMcpAdapter,
    local_state: Option<Arc<RwLock<LocalState>>>,
    approval_state_path: Option<PathBuf>,
    sessions: Arc<Mutex<HashMap<String, PreparedPluginSession>>>,
    artifact_store: PluginArtifactStore,
    disabled_plugins: Arc<Mutex<BTreeSet<String>>>,
    telemetry: Arc<Mutex<PluginRuntimeTelemetryState>>,
}

#[derive(Debug, Clone)]
struct PreparedPluginSession {
    run_id: String,
    owner_user_id: String,
    device_id: String,
    workspace_id: String,
    plugin_id: String,
    release_id: String,
    version: String,
    artifact_sha256: String,
    component_key: String,
    permission_snapshot: BTreeSet<String>,
    skills: BTreeMap<String, PluginSkillSnapshot>,
    agents: BTreeMap<String, PluginAgentSnapshot>,
    commands: BTreeMap<String, PluginCommandSnapshot>,
    hooks: BTreeMap<String, PluginHookSetSnapshot>,
    ui: Option<PluginUiSnapshot>,
    native_skill: Option<PreparedPluginNativeSkill>,
    native_action_lock: Arc<AsyncMutex<()>>,
    hook_dispatch_lock: Arc<AsyncMutex<()>>,
    native_action_cancelled: Arc<AtomicBool>,
    mcp: Option<PreparedPluginMcp>,
    expires_at: i64,
}

#[derive(Debug, Clone, Serialize)]
struct PreparedPluginNativeSkill {
    #[serde(flatten)]
    binding: PluginNativeSkillBindingSnapshot,
    tools: Vec<Value>,
    tool_snapshot_sha256: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginDisabledHookReport {
    pub event_id: String,
    pub plugin_id: String,
    #[serde(default)]
    pub release_id: Option<String>,
    #[serde(default)]
    pub artifact_sha256: Option<String>,
    pub cancelled_sessions: usize,
    pub blocking_failures: usize,
    #[serde(default)]
    pub dispatches: Vec<super::PluginHookDispatchResult>,
    #[serde(default)]
    pub errors: Vec<String>,
}

impl PreparedPluginNativeSkill {
    fn publishes_tool(&self, tool_name: &str) -> bool {
        self.tools
            .iter()
            .any(|tool| tool.get("name").and_then(Value::as_str) == Some(tool_name))
    }
}

impl PluginRuntimeHost {
    pub fn new(skill_loader: PluginSkillLoader, mcp_adapter: PluginMcpAdapter) -> Self {
        let installer = skill_loader.installer();
        let agent_loader = PluginAgentLoader::new(installer.clone());
        let command_loader = PluginCommandLoader::new(installer);
        let hook_loader = PluginHookLoader::new(skill_loader.installer());
        let ui_loader = PluginUiLoader::new(skill_loader.installer());
        Self {
            skill_loader,
            agent_loader,
            command_loader,
            hook_loader,
            ui_loader,
            mcp_adapter,
            local_state: None,
            approval_state_path: None,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            artifact_store: PluginArtifactStore::default(),
            disabled_plugins: Arc::new(Mutex::new(BTreeSet::new())),
            telemetry: Arc::new(Mutex::new(PluginRuntimeTelemetryState::default())),
        }
    }

    pub(crate) fn with_local_state(mut self, state: Arc<RwLock<LocalState>>) -> Self {
        self.local_state = Some(state);
        self
    }

    pub(crate) fn with_approval_state_path(mut self, state_path: PathBuf) -> Self {
        #[cfg(not(test))]
        {
            self.artifact_store = PluginArtifactStore::for_state_path(state_path.as_path());
        }
        self.approval_state_path = Some(state_path);
        self
    }

    #[cfg(test)]
    pub(super) fn with_artifact_persistence_for_tests(
        mut self,
        state_path: PathBuf,
        storage: crate::secure_storage::SecureStorage,
    ) -> Self {
        self.artifact_store =
            PluginArtifactStore::for_state_path_with_storage(state_path.as_path(), storage);
        self
    }

    pub async fn handle_prepare(&self, value: Value) -> Value {
        let request = match decode_request("plugin_prepare_response", value) {
            Ok(request) => request,
            Err(response) => return response,
        };
        let identity = match telemetry_identity_from_request(&request) {
            Ok(identity) => identity,
            Err(error) => {
                return plugin_response("plugin_prepare_response", &request, Err(error));
            }
        };
        self.telemetry().record_prepare_started(&identity);
        let started = Instant::now();
        let result = self.prepare(&request).await;
        match &result {
            Ok(body) => {
                let adapter_session_id = body
                    .get("adapter_session_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let expires_at = body
                    .get("expires_at")
                    .and_then(Value::as_i64)
                    .unwrap_or_default();
                let health_status = body.pointer("/mcp_health/status").and_then(Value::as_str);
                self.telemetry().record_prepare_succeeded(
                    &identity,
                    adapter_session_id,
                    expires_at,
                    elapsed_millis(started),
                    health_status,
                );
            }
            Err((_status, error)) => self.telemetry().record_prepare_failed(
                &identity,
                elapsed_millis(started),
                error.as_str(),
            ),
        }
        plugin_response("plugin_prepare_response", &request, result)
    }

    pub async fn handle_execute(&self, value: Value) -> Value {
        let request = match decode_request("plugin_execute_response", value) {
            Ok(request) => request,
            Err(response) => return response,
        };
        plugin_response(
            "plugin_execute_response",
            &request,
            self.execute(&request).await,
        )
    }

    pub async fn handle_cancel(&self, value: Value) -> Value {
        let request = match decode_request("plugin_cancel_response", value) {
            Ok(request) => request,
            Err(response) => return response,
        };
        plugin_response(
            "plugin_cancel_response",
            &request,
            self.cancel(&request).await,
        )
    }

    pub async fn handle_ui_asset(&self, value: Value) -> Value {
        let request = match decode_request("plugin_ui_asset_response", value) {
            Ok(request) => request,
            Err(response) => return response,
        };
        plugin_response(
            "plugin_ui_asset_response",
            &request,
            self.read_ui_asset(&request),
        )
    }

    pub async fn handle_artifact_list(&self, value: Value) -> Value {
        let request = match decode_request("plugin_artifact_list_response", value) {
            Ok(request) => request,
            Err(response) => return response,
        };
        plugin_response(
            "plugin_artifact_list_response",
            &request,
            self.list_artifacts(&request),
        )
    }

    pub async fn handle_artifact_read(&self, value: Value) -> Value {
        let request = match decode_request("plugin_artifact_read_response", value) {
            Ok(request) => request,
            Err(response) => return response,
        };
        plugin_response(
            "plugin_artifact_read_response",
            &request,
            self.read_artifact(&request).await,
        )
    }

    pub async fn handle_artifact_create(&self, value: Value) -> Value {
        let request = match decode_request("plugin_artifact_create_response", value) {
            Ok(request) => request,
            Err(response) => return response,
        };
        plugin_response(
            "plugin_artifact_create_response",
            &request,
            self.create_artifact(&request).await,
        )
    }

    pub async fn handle_artifact_update(&self, value: Value) -> Value {
        let request = match decode_request("plugin_artifact_update_response", value) {
            Ok(request) => request,
            Err(response) => return response,
        };
        plugin_response(
            "plugin_artifact_update_response",
            &request,
            self.update_artifact(&request).await,
        )
    }

    pub fn telemetry_snapshot(&self) -> super::PluginRuntimeTelemetrySnapshot {
        self.prune_expired_sessions();
        self.telemetry().snapshot()
    }

    async fn execute(&self, request: &RelayRequest) -> Result<Value, (u16, String)> {
        let adapter_session_id = required_body_text(&request.body, "adapter_session_id")?;
        let operation = required_body_text(&request.body, "operation")?;
        let session = self.load_exact_session(request, adapter_session_id.as_str())?;
        let identity = session.telemetry_identity();
        let phase = if session
            .mcp
            .as_ref()
            .is_some_and(|mcp| mcp.health_operation() == operation)
        {
            PluginRuntimeTelemetryPhase::Health
        } else {
            PluginRuntimeTelemetryPhase::Execute
        };
        let tool_name = request.body.get("tool_name").and_then(Value::as_str);
        self.telemetry().record_execution_started(
            &identity,
            adapter_session_id.as_str(),
            phase,
            operation.as_str(),
            tool_name,
        );
        let started = Instant::now();
        let result = self
            .execute_prepared(
                request,
                adapter_session_id.as_str(),
                operation.as_str(),
                &session,
            )
            .await;
        let duration_ms = elapsed_millis(started);
        match &result {
            Ok(body) => {
                let health_status = body.pointer("/mcp_health/status").and_then(Value::as_str);
                self.telemetry().record_execution_finished(
                    &identity,
                    adapter_session_id.as_str(),
                    phase,
                    operation.as_str(),
                    tool_name,
                    duration_ms,
                    Ok(health_status),
                );
            }
            Err((_status, error)) => self.telemetry().record_execution_finished(
                &identity,
                adapter_session_id.as_str(),
                phase,
                operation.as_str(),
                tool_name,
                duration_ms,
                Err(error.as_str()),
            ),
        }
        result
    }

    async fn cancel(&self, request: &RelayRequest) -> Result<Value, (u16, String)> {
        let adapter_session_id = required_body_text(&request.body, "adapter_session_id")?;
        let expected = ExactSessionIdentity::from_request(request)?;
        self.prune_expired_sessions();
        if let Some(invocation_id) = request
            .body
            .get("invocation_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let session = self.load_exact_session(request, adapter_session_id.as_str())?;
            let mcp = session.mcp.as_ref().ok_or_else(|| {
                (
                    409,
                    "Plugin adapter session has no prepared MCP runtime".to_string(),
                )
            })?;
            let outcome = mcp
                .cancel_invocation(invocation_id)
                .map_err(|error| (400, error.to_string()))?;
            let status = match outcome {
                PluginMcpInvocationCancelOutcome::Cancelled => "cancelled",
                PluginMcpInvocationCancelOutcome::CancelRequested => "cancel_requested",
                PluginMcpInvocationCancelOutcome::InvocationNotFound => "invocation_not_found",
            };
            return Ok(json!({
                "run_id": session.run_id,
                "adapter_session_id": adapter_session_id,
                "invocation_id": invocation_id,
                "status": status,
            }));
        }
        let removed = {
            let mut sessions = self.sessions()?;
            let Some(session) = sessions.get(adapter_session_id.as_str()) else {
                return Ok(json!({
                    "adapter_session_id": adapter_session_id,
                    "cancelled": false,
                }));
            };
            expected.validate(session)?;
            sessions.remove(adapter_session_id.as_str())
        };
        if let Some(session) = removed {
            let identity = session.telemetry_identity();
            self.telemetry()
                .record_cancel_started(&identity, adapter_session_id.as_str());
            let started = Instant::now();
            session
                .native_action_cancelled
                .store(true, Ordering::SeqCst);
            if let Some(mcp) = &session.mcp {
                mcp.cancel();
            }
            let cancelled_approvals = cancel_pending_approvals_for_session(
                adapter_session_id.as_str(),
                "Plugin session was cancelled by the user or Task Runner",
            )
            .await;
            clear_session_approvals(adapter_session_id.as_str()).await;
            self.telemetry().record_cancelled(
                &identity,
                adapter_session_id.as_str(),
                elapsed_millis(started),
            );
            return Ok(json!({
                "run_id": session.run_id,
                "adapter_session_id": adapter_session_id,
                "cancelled": true,
                "cancelled_pending_approvals": cancelled_approvals,
            }));
        }
        Ok(json!({
            "adapter_session_id": adapter_session_id,
            "cancelled": true,
        }))
    }

    fn load_exact_session(
        &self,
        request: &RelayRequest,
        adapter_session_id: &str,
    ) -> Result<PreparedPluginSession, (u16, String)> {
        let expected = ExactSessionIdentity::from_request(request)?;
        self.prune_expired_sessions();
        let sessions = self.sessions()?;
        let session = sessions.get(adapter_session_id).cloned().ok_or_else(|| {
            (
                410,
                "Plugin adapter session is missing or expired".to_string(),
            )
        })?;
        expected.validate(&session)?;
        Ok(session)
    }

    fn sessions(
        &self,
    ) -> Result<MutexGuard<'_, HashMap<String, PreparedPluginSession>>, (u16, String)> {
        self.sessions
            .lock()
            .map_err(|_| (500, "Plugin session store is unavailable".to_string()))
    }

    fn telemetry(&self) -> MutexGuard<'_, PluginRuntimeTelemetryState> {
        self.telemetry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn prune_expired_sessions(&self) {
        let now = Utc::now().timestamp();
        let expired = {
            let Ok(mut sessions) = self.sessions.lock() else {
                return;
            };
            let expired_ids = sessions
                .iter()
                .filter(|(_, session)| session.expires_at <= now)
                .map(|(adapter_session_id, _)| adapter_session_id.clone())
                .collect::<Vec<_>>();
            expired_ids
                .into_iter()
                .filter_map(|adapter_session_id| {
                    sessions
                        .remove(adapter_session_id.as_str())
                        .map(|session| (adapter_session_id, session))
                })
                .collect::<Vec<_>>()
        };
        for (adapter_session_id, session) in expired {
            session
                .native_action_cancelled
                .store(true, Ordering::SeqCst);
            if let Some(mcp) = &session.mcp {
                mcp.cancel();
            }
            self.telemetry()
                .record_expired(&session.telemetry_identity(), adapter_session_id.as_str());
        }
    }

    async fn approve_hook_workspace_writes(
        &self,
        request: &RelayRequest,
        adapter_session_id: &str,
        session: &PreparedPluginSession,
        hook_set: &PluginHookSetSnapshot,
        event: PluginHookEvent,
        context: &PluginHookEventContext,
    ) -> Result<BTreeMap<String, PluginHookWorkspaceWriteDecision>, (u16, String)> {
        let hook_ids = self
            .hook_loader
            .matching_workspace_write_hook_ids(hook_set, event, context)
            .map_err(|error| (400, error.to_string()))?;
        if hook_ids.is_empty() {
            return Ok(BTreeMap::new());
        }

        let _dispatch_guard = session.hook_dispatch_lock.lock().await;
        self.load_exact_session(request, adapter_session_id)?;
        let state = self.state_snapshot().await?;
        let workspace = state
            .workspace_by_id(session.workspace_id.as_str())
            .cloned()
            .ok_or_else(|| {
                (
                    409,
                    "Plugin Hook workspace is not registered locally".to_string(),
                )
            })?;
        let workspace_root = approved_workspace_root(workspace.absolute_root.as_path())?;
        let workspace_identity =
            crate::workspace::trust::workspace_project_config_trust_fingerprint(
                workspace_root.as_path(),
            )
            .map_err(internal_error)?;
        let requested_permissions = workspace_write_permission_request(workspace_root.as_path());
        let expected_grant = GrantedPermissionProfile::from(requested_permissions.clone());
        let project_key = approval_project_key_for_relay_scope(&state, request);
        let approval_service = self.approval_service()?;
        let mut decisions = BTreeMap::new();
        for hook_id in hook_ids {
            let approval = approval_service
                .approve_interactive(CommandApprovalRequest {
                    request_id: format!("{}:plugin-hook:{hook_id}", request.request_id),
                    project_key: project_key.clone(),
                    command: "plugin-hook-workspace-write".to_string(),
                    args: vec![
                        session.plugin_id.clone(),
                        session.component_key.clone(),
                        hook_id.clone(),
                        event.as_str().to_string(),
                        hook_set.snapshot_sha256.clone(),
                    ],
                    redact_arguments_in_history: false,
                    cwd: ".".to_string(),
                    source: "plugin_hook_workspace_write".to_string(),
                    requested_permissions: Some(requested_permissions.clone()),
                    session_id: Some(adapter_session_id.to_string()),
                    action_audit: Some(ApprovalActionAudit {
                        kind: "plugin_hook_workspace_write".to_string(),
                        operation: event.as_str().to_string(),
                        details: vec![
                            ApprovalActionAuditDetail {
                                key: "plugin_id".to_string(),
                                value: session.plugin_id.clone(),
                            },
                            ApprovalActionAuditDetail {
                                key: "component_key".to_string(),
                                value: session.component_key.clone(),
                            },
                            ApprovalActionAuditDetail {
                                key: "hook_id".to_string(),
                                value: hook_id.clone(),
                            },
                            ApprovalActionAuditDetail {
                                key: "hook_snapshot_sha256".to_string(),
                                value: hook_set.snapshot_sha256.clone(),
                            },
                            ApprovalActionAuditDetail {
                                key: "workspace_id".to_string(),
                                value: session.workspace_id.clone(),
                            },
                        ],
                        privacy: Some(
                            "The approval and audit omit Hook stdin, stdout, stderr, tool payloads, and workspace file contents."
                                .to_string(),
                        ),
                        safety: Some(
                            "Approval authorizes one matched invocation of this exact signed Hook snapshot to write only the registered workspace; network and Plugin-root writes remain blocked."
                                .to_string(),
                        ),
                        recovery: Some(
                            "Deny to skip the Hook. After approval, review or revert workspace changes with the project's normal version-control workflow."
                                .to_string(),
                        ),
                    }),
                })
                .await
                .map_err(internal_error)?;
            self.load_exact_session(request, adapter_session_id)?;
            let current_state = self.state_snapshot().await?;
            let current_workspace = current_state
                .workspace_by_id(session.workspace_id.as_str())
                .ok_or_else(|| {
                    (
                        409,
                        "Plugin Hook workspace registration changed during approval".to_string(),
                    )
                })?;
            let current_workspace_root =
                approved_workspace_root(current_workspace.absolute_root.as_path())?;
            let current_identity =
                crate::workspace::trust::workspace_project_config_trust_fingerprint(
                    current_workspace_root.as_path(),
                )
                .map_err(internal_error)?;
            if current_workspace_root != workspace_root || current_identity != workspace_identity {
                return Err((
                    409,
                    "Plugin Hook workspace registration or identity changed during approval"
                        .to_string(),
                ));
            }
            let decision = match approval {
                ApprovalDecision::Approved {
                    granted_permissions,
                    permission_scope,
                    ..
                } if granted_permissions.as_ref() == Some(&expected_grant)
                    && permission_scope == PermissionGrantScope::Turn =>
                {
                    PluginHookWorkspaceWriteDecision::Approved(workspace_root.clone())
                }
                ApprovalDecision::Approved { .. } => PluginHookWorkspaceWriteDecision::Denied(
                    "Plugin Hook workspace-write approval did not grant the exact one-invocation workspace scope"
                        .to_string(),
                ),
                ApprovalDecision::Denied { reason, .. } => {
                    PluginHookWorkspaceWriteDecision::Denied(format!(
                        "Plugin Hook workspace-write approval was denied: {reason}"
                    ))
                }
            };
            decisions.insert(hook_id, decision);
        }
        Ok(decisions)
    }

    async fn state_snapshot(&self) -> Result<LocalState, (u16, String)> {
        let state = self.local_state.as_ref().ok_or_else(|| {
            (
                409,
                "Local Connector state is unavailable for local Plugin execution".to_string(),
            )
        })?;
        Ok(state.read().await.clone())
    }

    fn approval_service(&self) -> Result<CommandApprovalService, (u16, String)> {
        let state_path = self.approval_state_path.clone().ok_or_else(|| {
            (
                409,
                "Local Connector interactive approval is unavailable for this Plugin operation"
                    .to_string(),
            )
        })?;
        let state = self.local_state.clone().ok_or_else(|| {
            (
                409,
                "Local Connector state is unavailable for privileged local action approval"
                    .to_string(),
            )
        })?;
        Ok(CommandApprovalService::new(state_path, state))
    }
}
