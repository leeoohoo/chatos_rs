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
    PluginUiAssetReadResponse, PluginUiSnapshot, PLUGIN_ARTIFACT_WRITE_MAX_BYTES,
    PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_DOWNLOAD,
    PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_LIST, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_READ,
    PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE,
};
use chatos_sandbox_contract::{
    AdditionalFileSystemPermissions, FileSystemAccessMode, FileSystemPath, FileSystemSandboxEntry,
    GrantedPermissionProfile, PermissionGrantScope, RequestPermissionProfile,
};
use chrono::Utc;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex as AsyncMutex, RwLock};
use uuid::Uuid;

use super::artifact_store::{PluginArtifactProducer, PluginArtifactStore, PluginUiArtifactGrant};
use super::hook_loader::PluginHookWorkspaceWriteDecision;
use super::mcp_adapter::{PluginMcpAdapter, PreparedPluginMcp};
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

    pub async fn dispatch_plugin_disabled(&self, plugin_id: &str) -> PluginDisabledHookReport {
        let started = Instant::now();
        let event_id = format!("plugin-disabled-{}", Uuid::new_v4());
        if let Ok(mut disabled_plugins) = self.disabled_plugins.lock() {
            disabled_plugins.insert(plugin_id.to_string());
        }
        let cancelled_sessions = self.cancel_plugin_sessions(plugin_id).await;
        let mut report = PluginDisabledHookReport {
            event_id: event_id.clone(),
            plugin_id: plugin_id.to_string(),
            release_id: None,
            artifact_sha256: None,
            cancelled_sessions,
            blocking_failures: 0,
            dispatches: Vec::new(),
            errors: Vec::new(),
        };
        let installer = self.skill_loader.installer();
        let installation = match installer.active_installation(plugin_id) {
            Ok(Some(installation)) => installation,
            Ok(None) => {
                self.record_plugin_disabled_telemetry(&report, started);
                return report;
            }
            Err(error) => {
                report
                    .errors
                    .push(sanitize_error(error.to_string().as_str()));
                self.record_plugin_disabled_telemetry(&report, started);
                return report;
            }
        };
        report.release_id = Some(installation.version.release_id.clone());
        report.artifact_sha256 = Some(installation.version.artifact_sha256.clone());
        let permission_snapshot = installation
            .version
            .inventory
            .permissions
            .iter()
            .map(|requirement| requirement.permission.clone())
            .collect::<BTreeSet<_>>();
        let summary_sha256 = plugin_disabled_summary_sha256(&installation);
        for component in installation
            .version
            .inventory
            .components
            .iter()
            .filter(|component| component.kind == PluginComponentKind::HookSet)
        {
            let result = async {
                let entrypoint = component
                    .entrypoint
                    .as_ref()
                    .context("Plugin Hook component entrypoint is missing")?;
                let relative_path = entrypoint.path.trim_start_matches("./");
                let expected_content_sha256 = installation
                    .version
                    .package_file_sha256
                    .get(relative_path)
                    .context("Plugin Hook source is not covered by package checksums")?;
                let snapshot = self.hook_loader.load(
                    plugin_id,
                    component.component_key.as_str(),
                    expected_content_sha256.as_str(),
                    &permission_snapshot,
                )?;
                self.hook_loader
                    .dispatch(
                        &snapshot,
                        &permission_snapshot,
                        event_id.as_str(),
                        PluginHookEvent::PluginDisabled,
                        &PluginHookEventContext {
                            component_key: Some(component.component_key.clone()),
                            outcome: Some(PluginHookOutcome::Succeeded),
                            summary_sha256: Some(summary_sha256.clone()),
                            ..PluginHookEventContext::default()
                        },
                        &BTreeMap::new(),
                    )
                    .await
            }
            .await;
            match result {
                Ok(dispatch) => {
                    report.blocking_failures = report
                        .blocking_failures
                        .saturating_add(usize::from(dispatch.blocking_failure));
                    report.errors.extend(
                        dispatch
                            .executions
                            .iter()
                            .filter(|execution| execution.matched && !execution.succeeded)
                            .map(|execution| {
                                format!(
                                    "PluginDisabled Hook {} failed for component {}",
                                    execution.hook_id, component.component_key
                                )
                            }),
                    );
                    report.dispatches.push(dispatch);
                }
                Err(error) => report.errors.push(sanitize_error(
                    format!(
                        "PluginDisabled Hook dispatch failed for component {}: {error}",
                        component.component_key
                    )
                    .as_str(),
                )),
            }
        }
        self.record_plugin_disabled_telemetry(&report, started);
        report
    }

    pub fn mark_plugin_enabled(&self, plugin_id: &str) {
        if let Ok(mut disabled_plugins) = self.disabled_plugins.lock() {
            disabled_plugins.remove(plugin_id);
        }
    }

    fn record_plugin_disabled_telemetry(
        &self,
        report: &PluginDisabledHookReport,
        started: Instant,
    ) {
        let identity = PluginRuntimeTelemetryIdentity {
            run_id: report.event_id.clone(),
            plugin_id: report.plugin_id.clone(),
            release_id: report
                .release_id
                .clone()
                .unwrap_or_else(|| "not-installed".to_string()),
            component_key: "plugin-disabled".to_string(),
        };
        let error = (!report.errors.is_empty()).then(|| report.errors.join("; "));
        self.telemetry().record_lifecycle_finished(
            &identity,
            "plugin_disabled",
            elapsed_millis(started),
            error.as_deref().map_or(Ok(()), Err),
        );
    }

    async fn cancel_plugin_sessions(&self, plugin_id: &str) -> usize {
        let removed = {
            let Ok(mut sessions) = self.sessions.lock() else {
                return 0;
            };
            let ids = sessions
                .iter()
                .filter(|(_, session)| session.plugin_id == plugin_id)
                .map(|(adapter_session_id, _)| adapter_session_id.clone())
                .collect::<Vec<_>>();
            ids.into_iter()
                .filter_map(|adapter_session_id| {
                    sessions
                        .remove(adapter_session_id.as_str())
                        .map(|session| (adapter_session_id, session))
                })
                .collect::<Vec<_>>()
        };
        let count = removed.len();
        for (adapter_session_id, session) in removed {
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
            cancel_pending_approvals_for_session(
                adapter_session_id.as_str(),
                "Plugin was disabled by the user",
            )
            .await;
            clear_session_approvals(adapter_session_id.as_str()).await;
            self.telemetry().record_cancelled(
                &identity,
                adapter_session_id.as_str(),
                elapsed_millis(started),
            );
        }
        count
    }

    async fn prepare(&self, request: &RelayRequest) -> Result<Value, (u16, String)> {
        let run_id = required_body_text(&request.body, "run_id")?;
        let owner_user_id =
            required_envelope_text(request.owner_user_id.as_deref(), "owner_user_id")?;
        let device_id = required_envelope_text(request.device_id.as_deref(), "device_id")?;
        let plugin_id = required_body_text(&request.body, "plugin_id")?;
        if self
            .disabled_plugins
            .lock()
            .map_err(|_| {
                (
                    500,
                    "Plugin disabled-state store is unavailable".to_string(),
                )
            })?
            .contains(plugin_id.as_str())
        {
            return Err((409, "Plugin is disabled by the user".to_string()));
        }
        let release_id = required_body_text(&request.body, "release_id")?;
        let artifact_sha256 = required_sha256(&request.body, "artifact_sha256")?;
        let component_key = required_body_text(&request.body, "component_key")?;
        let permission_snapshot =
            optional_body_text_set(&request.body, "permission_snapshot", 256)?;
        let component_kind = self
            .skill_loader
            .active_component_kind(plugin_id.as_str(), component_key.as_str())
            .map_err(|error| (409, error.to_string()))?;
        let adapter_session_id = Uuid::new_v4().to_string();
        let (skills, agents, commands, hooks, ui, native_skill, mcp, version, operations) =
            match component_kind {
                PluginComponentKind::SkillCollection => {
                    let skill_keys = required_body_text_array(&request.body, "skill_keys", 64)?;
                    let available = self
                        .skill_loader
                        .load_component(plugin_id.as_str(), component_key.as_str())
                        .map_err(|error| (409, error.to_string()))?;
                    let mut by_key = available
                        .into_iter()
                        .map(|skill| (skill.skill_key.clone(), skill))
                        .collect::<BTreeMap<_, _>>();
                    let mut selected = BTreeMap::new();
                    for skill_key in skill_keys {
                        let skill = by_key.remove(skill_key.as_str()).ok_or_else(|| {
                        (
                            404,
                            format!(
                                "Plugin Skill is not available in the selected component: {skill_key}"
                            ),
                        )
                    })?;
                        validate_prepared_release(
                            skill.release_id.as_str(),
                            skill.artifact_sha256.as_str(),
                            release_id.as_str(),
                            artifact_sha256.as_str(),
                        )?;
                        selected.insert(skill_key, skill);
                    }
                    let version = selected
                        .values()
                        .next()
                        .context("Plugin prepare selected no Skills")
                        .map_err(internal_error)?
                        .version
                        .clone();
                    let runtime_kind = optional_body_text(&request.body, "runtime_kind")?;
                    let runtime_metadata = request.body.get("runtime_metadata");
                    let content_sha256 = optional_body_text(&request.body, "content_sha256")?;
                    let native_binding = self
                        .skill_loader
                        .load_bundled_native_binding(
                            plugin_id.as_str(),
                            component_key.as_str(),
                            runtime_kind.as_deref(),
                            runtime_metadata,
                            content_sha256.as_deref(),
                            &selected,
                        )
                        .map_err(|error| (409, error.to_string()))?;
                    let native_skill = if let Some(binding) = native_binding {
                        for permission in &binding.permissions {
                            if !permission_snapshot.contains(permission) {
                                return Err((
                                403,
                                format!(
                                    "native Plugin Skill permission was not granted in the immutable Run snapshot: {permission}"
                                ),
                            ));
                            }
                        }
                        if binding.requires_workspace && request.workspace_id.trim().is_empty() {
                            return Err((
                                400,
                                "native Plugin Skill requires an authorized local workspace"
                                    .to_string(),
                            ));
                        }
                        if let Some(error) =
                            crate::skills::native::dependency_error(binding.skill_id.as_str())
                        {
                            return Err((409, error));
                        }
                        let state = self.state_snapshot().await?;
                        if binding.requires_workspace
                            && state.workspace_by_id(request.workspace_id.trim()).is_none()
                        {
                            return Err((
                                404,
                                "native Plugin Skill workspace is not registered locally"
                                    .to_string(),
                            ));
                        }
                        let allow_interactive_control = binding
                            .permissions
                            .iter()
                            .any(|permission| permission == "desktop.control")
                            && permission_snapshot.contains("desktop.control");
                        let tools = crate::skills::native::plugin_tool_definitions(
                            binding.skill_id.as_str(),
                            &state,
                            request,
                            allow_interactive_control,
                            self.approval_state_path.is_some(),
                        )
                        .map_err(|error| (409, error.to_string()))?;
                        if tools.is_empty() {
                            return Err((
                                409,
                                "native Plugin Skill published no executable tools".to_string(),
                            ));
                        }
                        let tool_snapshot_sha256 = native_tool_snapshot_sha256(&binding, &tools)
                            .map_err(internal_error)?;
                        Some(PreparedPluginNativeSkill {
                            binding,
                            tools,
                            tool_snapshot_sha256,
                        })
                    } else {
                        None
                    };
                    let mut operations = vec![LOAD_SKILL_RESOURCE_OPERATION];
                    if native_skill.is_some() {
                        operations.push(NATIVE_SKILL_TOOL_CALL_OPERATION);
                    }
                    (
                        selected,
                        BTreeMap::new(),
                        BTreeMap::new(),
                        BTreeMap::new(),
                        None,
                        native_skill,
                        None,
                        version,
                        operations,
                    )
                }
                PluginComponentKind::McpServer => {
                    let server_key = optional_body_text(&request.body, "server_key")?;
                    let tool_allowlist =
                        optional_body_text_set(&request.body, "tool_allowlist", 200)?;
                    let tool_blocklist =
                        optional_body_text_set(&request.body, "tool_blocklist", 200)?;
                    let mcp = self
                        .mcp_adapter
                        .prepare(
                            plugin_id.as_str(),
                            component_key.as_str(),
                            server_key.as_deref(),
                            adapter_session_id.as_str(),
                            owner_user_id.as_str(),
                            device_id.as_str(),
                            &permission_snapshot,
                            &tool_allowlist,
                            &tool_blocklist,
                        )
                        .await
                        .map_err(|error| (409, error.to_string()))?;
                    validate_prepared_release(
                        mcp.snapshot().release_id.as_str(),
                        mcp.snapshot().artifact_sha256.as_str(),
                        release_id.as_str(),
                        artifact_sha256.as_str(),
                    )?;
                    let version = mcp.snapshot().version.clone();
                    let operation = mcp.operation();
                    let health_operation = mcp.health_operation();
                    (
                        BTreeMap::new(),
                        BTreeMap::new(),
                        BTreeMap::new(),
                        BTreeMap::new(),
                        None,
                        None,
                        Some(mcp),
                        version,
                        vec![operation, health_operation],
                    )
                }
                PluginComponentKind::Command => {
                    let content_sha256 = required_sha256(&request.body, "content_sha256")?;
                    let arguments = optional_body_text(&request.body, "arguments")?;
                    let mut command = self
                        .command_loader
                        .load(
                            plugin_id.as_str(),
                            component_key.as_str(),
                            content_sha256.as_str(),
                            &permission_snapshot,
                            arguments.as_deref(),
                        )
                        .map_err(|error| (409, error.to_string()))?;
                    validate_prepared_release(
                        command.release_id.as_str(),
                        command.artifact_sha256.as_str(),
                        release_id.as_str(),
                        artifact_sha256.as_str(),
                    )?;
                    if command.requires_confirmation {
                        let state = self.state_snapshot().await?;
                        let mut approval_args = vec![plugin_id.clone(), component_key.clone()];
                        if let Some(arguments) = command.arguments.as_ref() {
                            approval_args.push(arguments.clone());
                        }
                        let approval = self
                        .approval_service()?
                        .approve_interactive(CommandApprovalRequest {
                            request_id: request.request_id.clone(),
                            project_key: approval_project_key_for_relay_scope(&state, request),
                            command: "plugin-command".to_string(),
                            args: approval_args,
                            redact_arguments_in_history: true,
                            cwd: ".".to_string(),
                            source: "plugin_command".to_string(),
                            requested_permissions: None,
                            session_id: Some(adapter_session_id.clone()),
                            action_audit: Some(ApprovalActionAudit {
                                kind: "plugin_command".to_string(),
                                operation: component_key.clone(),
                                details: vec![
                                    ApprovalActionAuditDetail {
                                        key: "plugin_id".to_string(),
                                        value: plugin_id.clone(),
                                    },
                                    ApprovalActionAuditDetail {
                                        key: "command_id".to_string(),
                                        value: component_key.clone(),
                                    },
                                    ApprovalActionAuditDetail {
                                        key: "arguments_sha256".to_string(),
                                        value: command.arguments_sha256.clone(),
                                    },
                                ],
                                privacy: Some(
                                    "Command arguments are visible only in the pending local approval and are redacted from approval history."
                                        .to_string(),
                                ),
                                safety: Some(
                                    "Approval authorizes only this exact signed Plugin Command snapshot for the current Run."
                                        .to_string(),
                                ),
                                recovery: Some(
                                    "Deny the request to prevent this Command prompt from entering the Run."
                                        .to_string(),
                                ),
                            }),
                        })
                        .await
                        .map_err(internal_error)?;
                        if let ApprovalDecision::Denied { reason, .. } = approval {
                            return Err((
                                403,
                                format!("Plugin Command was not approved: {reason}"),
                            ));
                        }
                        let reloaded = self
                            .command_loader
                            .load(
                                plugin_id.as_str(),
                                component_key.as_str(),
                                content_sha256.as_str(),
                                &permission_snapshot,
                                arguments.as_deref(),
                            )
                            .map_err(|error| (409, error.to_string()))?;
                        validate_prepared_release(
                            reloaded.release_id.as_str(),
                            reloaded.artifact_sha256.as_str(),
                            release_id.as_str(),
                            artifact_sha256.as_str(),
                        )?;
                        if reloaded != command {
                            return Err((
                                409,
                                "Plugin Command snapshot changed while awaiting approval"
                                    .to_string(),
                            ));
                        }
                        command.confirmation_approved = true;
                    }
                    let version = command.version.clone();
                    (
                        BTreeMap::new(),
                        BTreeMap::new(),
                        BTreeMap::from([(component_key.clone(), command)]),
                        BTreeMap::new(),
                        None,
                        None,
                        None,
                        version,
                        Vec::new(),
                    )
                }
                PluginComponentKind::Agent => {
                    let content_sha256 = required_sha256(&request.body, "content_sha256")?;
                    let agent = self
                        .agent_loader
                        .load(
                            plugin_id.as_str(),
                            component_key.as_str(),
                            content_sha256.as_str(),
                            &permission_snapshot,
                        )
                        .map_err(|error| (409, error.to_string()))?;
                    validate_prepared_release(
                        agent.release_id.as_str(),
                        agent.artifact_sha256.as_str(),
                        release_id.as_str(),
                        artifact_sha256.as_str(),
                    )?;
                    let version = agent.version.clone();
                    (
                        BTreeMap::new(),
                        BTreeMap::from([(component_key.clone(), agent)]),
                        BTreeMap::new(),
                        BTreeMap::new(),
                        None,
                        None,
                        None,
                        version,
                        Vec::new(),
                    )
                }
                PluginComponentKind::HookSet => {
                    let content_sha256 = required_sha256(&request.body, "content_sha256")?;
                    let hook_set = self
                        .hook_loader
                        .load(
                            plugin_id.as_str(),
                            component_key.as_str(),
                            content_sha256.as_str(),
                            &permission_snapshot,
                        )
                        .map_err(|error| (409, error.to_string()))?;
                    validate_prepared_release(
                        hook_set.release_id.as_str(),
                        hook_set.artifact_sha256.as_str(),
                        release_id.as_str(),
                        artifact_sha256.as_str(),
                    )?;
                    let version = hook_set.version.clone();
                    (
                        BTreeMap::new(),
                        BTreeMap::new(),
                        BTreeMap::new(),
                        BTreeMap::from([(component_key.clone(), hook_set)]),
                        None,
                        None,
                        None,
                        version,
                        vec![self.hook_loader.operation()],
                    )
                }
                PluginComponentKind::UiContribution => {
                    let content_sha256 = required_sha256(&request.body, "content_sha256")?;
                    let ui = self
                        .ui_loader
                        .load(
                            plugin_id.as_str(),
                            component_key.as_str(),
                            content_sha256.as_str(),
                            &permission_snapshot,
                        )
                        .map_err(|error| (409, error.to_string()))?;
                    validate_prepared_release(
                        ui.release_id.as_str(),
                        ui.artifact_sha256.as_str(),
                        release_id.as_str(),
                        artifact_sha256.as_str(),
                    )?;
                    let version = ui.version.clone();
                    (
                        BTreeMap::new(),
                        BTreeMap::new(),
                        BTreeMap::new(),
                        BTreeMap::new(),
                        Some(ui),
                        None,
                        None,
                        version,
                        Vec::new(),
                    )
                }
                _ => {
                    return Err((
                        409,
                        "Plugin component runtime is not implemented by this Host".to_string(),
                    ));
                }
            };
        let expires_at = Utc::now().timestamp() + PLUGIN_SESSION_TTL_SECONDS;
        let session = PreparedPluginSession {
            run_id: run_id.clone(),
            owner_user_id,
            device_id,
            workspace_id: request.workspace_id.trim().to_string(),
            plugin_id: plugin_id.clone(),
            release_id: release_id.clone(),
            version: version.clone(),
            artifact_sha256: artifact_sha256.clone(),
            component_key: component_key.clone(),
            permission_snapshot: permission_snapshot.clone(),
            skills: skills.clone(),
            agents: agents.clone(),
            commands: commands.clone(),
            hooks: hooks.clone(),
            ui: ui.clone(),
            native_skill: native_skill.clone(),
            native_action_lock: Arc::new(AsyncMutex::new(())),
            hook_dispatch_lock: Arc::new(AsyncMutex::new(())),
            native_action_cancelled: Arc::new(AtomicBool::new(false)),
            mcp,
            expires_at,
        };
        if let Some(ui) = session.ui.clone() {
            self.artifact_store
                .register_ui_grant(PluginUiArtifactGrant {
                    owner_user_id: session.owner_user_id.clone(),
                    device_id: session.device_id.clone(),
                    workspace_id: session.workspace_id.clone(),
                    run_id: session.run_id.clone(),
                    plugin_id: session.plugin_id.clone(),
                    release_id: session.release_id.clone(),
                    artifact_sha256: session.artifact_sha256.clone(),
                    component_key: session.component_key.clone(),
                    adapter_session_id: adapter_session_id.clone(),
                    ui,
                    permission_snapshot: session.permission_snapshot.clone(),
                    expires_at,
                })
                .map_err(internal_error)?;
        }
        let mcp_snapshot = session.mcp.as_ref().map(|mcp| mcp.snapshot().clone());
        let mcp_health = session
            .mcp
            .as_ref()
            .map(PreparedPluginMcp::health_snapshot)
            .transpose()
            .map_err(internal_error)?;
        let session_sha256 = session_audit_hash(&session);
        self.prune_expired_sessions();
        let mut sessions = self.sessions()?;
        sessions.insert(adapter_session_id.clone(), session);
        let skills = skills.into_values().collect::<Vec<_>>();
        let agents = agents.into_values().collect::<Vec<_>>();
        let commands = commands.into_values().collect::<Vec<_>>();
        let hooks = hooks.into_values().collect::<Vec<_>>();
        let ui = ui.into_iter().collect::<Vec<_>>();
        Ok(json!({
            "run_id": run_id,
            "plugin_id": plugin_id,
            "release_id": release_id,
            "version": version,
            "artifact_sha256": artifact_sha256,
            "component_key": component_key,
            "skills": skills,
            "agents": agents,
            "commands": commands,
            "hooks": hooks,
            "ui": ui,
            "native_skill": native_skill,
            "mcp": mcp_snapshot,
            "mcp_health": mcp_health,
            "operations": operations,
            "permission_snapshot": permission_snapshot,
            "adapter_session_id": adapter_session_id,
            "session_sha256": session_sha256,
            "expires_at": expires_at,
        }))
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

    fn read_ui_asset(&self, request: &RelayRequest) -> Result<Value, (u16, String)> {
        let access = artifact_ui_access_from_body(&request.body)?;
        let relative_path = required_body_text(&request.body, "relative_path")?;
        let grant = self.artifact_store.ui_grant(request, &access, "")?;
        self.validate_current_ui_grant(&grant)?;
        let ui = &grant.ui;
        if relative_path != ui.relative_source_path
            && !ui
                .assets
                .iter()
                .any(|asset| asset.relative_path == relative_path)
        {
            return Err((
                403,
                "Plugin UI asset was not published during prepare".to_string(),
            ));
        }
        let asset = self
            .ui_loader
            .read_asset(ui, &grant.permission_snapshot, relative_path.as_str())
            .map_err(|error| (409, error.to_string()))?;
        serde_json::to_value(PluginUiAssetReadResponse {
            run_id: grant.run_id,
            owner_user_id: grant.owner_user_id,
            plugin_id: grant.plugin_id,
            release_id: grant.release_id,
            artifact_sha256: grant.artifact_sha256,
            component_key: grant.component_key,
            adapter_session_id: grant.adapter_session_id,
            ui_snapshot_sha256: access.ui_snapshot_sha256,
            kind: asset.kind,
            relative_path: asset.relative_path,
            media_type: asset.media_type,
            size_bytes: asset.size_bytes,
            sha256: asset.sha256,
            body_base64: BASE64_STANDARD.encode(asset.bytes),
        })
        .map_err(|error| internal_error(error.into()))
    }

    fn list_artifacts(&self, request: &RelayRequest) -> Result<Value, (u16, String)> {
        let list: PluginArtifactListRequest = serde_json::from_value(request.body.clone())
            .map_err(|error| {
                (
                    400,
                    format!("Plugin Artifact list request is invalid: {error}"),
                )
            })?;
        let grant = self.artifact_store.ui_grant(
            request,
            &list.access,
            PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_LIST,
        )?;
        self.validate_current_ui_grant(&grant)?;
        serde_json::to_value(self.artifact_store.list(&grant, list.access)?)
            .map_err(|error| internal_error(error.into()))
    }

    async fn read_artifact(&self, request: &RelayRequest) -> Result<Value, (u16, String)> {
        let read: PluginArtifactReadRequest = serde_json::from_value(request.body.clone())
            .map_err(|error| {
                (
                    400,
                    format!("Plugin Artifact read request is invalid: {error}"),
                )
            })?;
        let capability = match read.mode {
            PluginArtifactReadMode::Inline => PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_READ,
            PluginArtifactReadMode::Download => PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_DOWNLOAD,
        };
        let grant = self
            .artifact_store
            .ui_grant(request, &read.access, capability)?;
        self.validate_current_ui_grant(&grant)?;
        let state = self.state_snapshot().await?;
        serde_json::to_value(self.artifact_store.read(
            &state,
            request,
            &grant,
            read.access,
            read.artifact_id.as_str(),
            read.mode,
        )?)
        .map_err(|error| internal_error(error.into()))
    }

    async fn create_artifact(&self, request: &RelayRequest) -> Result<Value, (u16, String)> {
        let create: PluginArtifactCreateRequest = serde_json::from_value(request.body.clone())
            .map_err(|error| {
                (
                    400,
                    format!("Plugin Artifact create request is invalid: {error}"),
                )
            })?;
        let grant = self.artifact_store.ui_grant(
            request,
            &create.access,
            PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE,
        )?;
        self.validate_current_ui_grant(&grant)?;
        let bytes = decode_artifact_write_body(create.body_base64.as_str())?;
        let state = self
            .approve_artifact_write(
                request,
                &grant,
                &create.access,
                PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE,
                create.display_name.as_str(),
                create.media_type.as_str(),
                None,
                bytes.as_slice(),
            )
            .await?;
        serde_json::to_value(self.artifact_store.create(
            &state,
            request,
            &grant,
            create.access,
            create.display_name.as_str(),
            create.media_type.as_str(),
            bytes.as_slice(),
        )?)
        .map_err(|error| internal_error(error.into()))
    }

    async fn update_artifact(&self, request: &RelayRequest) -> Result<Value, (u16, String)> {
        let update: PluginArtifactUpdateRequest = serde_json::from_value(request.body.clone())
            .map_err(|error| {
                (
                    400,
                    format!("Plugin Artifact update request is invalid: {error}"),
                )
            })?;
        let grant = self.artifact_store.ui_grant(
            request,
            &update.access,
            PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE,
        )?;
        self.validate_current_ui_grant(&grant)?;
        let bytes = decode_artifact_write_body(update.body_base64.as_str())?;
        let state = self
            .approve_artifact_write(
                request,
                &grant,
                &update.access,
                PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE,
                update.artifact_id.as_str(),
                "registered-mime-type",
                Some(update.expected_sha256.as_str()),
                bytes.as_slice(),
            )
            .await?;
        serde_json::to_value(self.artifact_store.update(
            &state,
            request,
            &grant,
            update.access,
            update.artifact_id.as_str(),
            update.expected_sha256.as_str(),
            bytes.as_slice(),
        )?)
        .map_err(|error| internal_error(error.into()))
    }

    #[allow(clippy::too_many_arguments)]
    async fn approve_artifact_write(
        &self,
        request: &RelayRequest,
        grant: &PluginUiArtifactGrant,
        access: &PluginArtifactUiAccess,
        operation: &str,
        target: &str,
        media_type: &str,
        expected_sha256: Option<&str>,
        bytes: &[u8],
    ) -> Result<LocalState, (u16, String)> {
        let state = self.state_snapshot().await?;
        let workspace = state
            .workspace_by_id(grant.workspace_id.as_str())
            .cloned()
            .ok_or_else(|| {
                (
                    409,
                    "Plugin Artifact workspace is not registered locally".to_string(),
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
        let body_sha256 = hex::encode(Sha256::digest(bytes));
        let mut args = vec![
            operation.to_string(),
            grant.plugin_id.clone(),
            grant.component_key.clone(),
            target.to_string(),
            media_type.to_string(),
            bytes.len().to_string(),
            body_sha256.clone(),
        ];
        if let Some(expected_sha256) = expected_sha256 {
            args.push(expected_sha256.to_string());
        }
        let approval = self
            .approval_service()?
            .approve_interactive(CommandApprovalRequest {
                request_id: format!("{}:{operation}", request.request_id),
                project_key: approval_project_key_for_relay_scope(&state, request),
                command: "plugin-artifact-write".to_string(),
                args,
                redact_arguments_in_history: true,
                cwd: ".".to_string(),
                source: "plugin_artifact_write".to_string(),
                requested_permissions: Some(requested_permissions),
                session_id: Some(grant.adapter_session_id.clone()),
                action_audit: Some(ApprovalActionAudit {
                    kind: "plugin_artifact_write".to_string(),
                    operation: operation.to_string(),
                    details: vec![
                        ApprovalActionAuditDetail {
                            key: "plugin_id".to_string(),
                            value: grant.plugin_id.clone(),
                        },
                        ApprovalActionAuditDetail {
                            key: "component_key".to_string(),
                            value: grant.component_key.clone(),
                        },
                        ApprovalActionAuditDetail {
                            key: "workspace_id".to_string(),
                            value: grant.workspace_id.clone(),
                        },
                        ApprovalActionAuditDetail {
                            key: "body_size_bytes".to_string(),
                            value: bytes.len().to_string(),
                        },
                        ApprovalActionAuditDetail {
                            key: "body_sha256".to_string(),
                            value: body_sha256,
                        },
                    ],
                    privacy: Some(
                        "The approval and persistent history omit the Artifact body and redact request arguments."
                            .to_string(),
                    ),
                    safety: Some(
                        "Approval authorizes one exact UI Artifact create/update inside a Host-generated workspace path; Plugin roots, .git, network, and arbitrary paths remain unavailable."
                            .to_string(),
                    ),
                    recovery: Some(
                        "Deny to skip the write. Mutable updates require the previously registered SHA-256 and can be reviewed or reverted with the workspace's normal tools."
                            .to_string(),
                    ),
                }),
            })
            .await
            .map_err(internal_error)?;
        match approval {
            ApprovalDecision::Approved {
                granted_permissions,
                permission_scope,
                ..
            } if granted_permissions.as_ref() == Some(&expected_grant)
                && permission_scope == PermissionGrantScope::Turn => {}
            ApprovalDecision::Approved { .. } => {
                return Err((
                    403,
                    "Plugin Artifact approval did not grant the exact one-write workspace scope"
                        .to_string(),
                ));
            }
            ApprovalDecision::Denied { reason, .. } => {
                return Err((
                    403,
                    format!("Plugin Artifact write approval was denied: {reason}"),
                ));
            }
        }
        let current_grant = self.artifact_store.ui_grant(request, access, operation)?;
        if current_grant != *grant {
            return Err((
                409,
                "Plugin Artifact UI grant changed during approval".to_string(),
            ));
        }
        self.validate_current_ui_grant(&current_grant)?;
        let current_state = self.state_snapshot().await?;
        let current_workspace = current_state
            .workspace_by_id(grant.workspace_id.as_str())
            .ok_or_else(|| {
                (
                    409,
                    "Plugin Artifact workspace registration changed during approval".to_string(),
                )
            })?;
        let current_workspace_root =
            approved_workspace_root(current_workspace.absolute_root.as_path())?;
        let current_identity = crate::workspace::trust::workspace_project_config_trust_fingerprint(
            current_workspace_root.as_path(),
        )
        .map_err(internal_error)?;
        if current_workspace_root != workspace_root || current_identity != workspace_identity {
            return Err((
                409,
                "Plugin Artifact workspace registration or identity changed during approval"
                    .to_string(),
            ));
        }
        Ok(current_state)
    }

    fn validate_current_ui_grant(
        &self,
        grant: &PluginUiArtifactGrant,
    ) -> Result<(), (u16, String)> {
        if self
            .disabled_plugins
            .lock()
            .map_err(|_| {
                (
                    500,
                    "Plugin disabled-state store is unavailable".to_string(),
                )
            })?
            .contains(grant.plugin_id.as_str())
        {
            return Err((403, "Plugin is disabled by the user".to_string()));
        }
        let current = self
            .ui_loader
            .load(
                grant.plugin_id.as_str(),
                grant.component_key.as_str(),
                grant.ui.content_sha256.as_str(),
                &grant.permission_snapshot,
            )
            .map_err(|error| (409, error.to_string()))?;
        if current != grant.ui {
            return Err((
                409,
                "Plugin UI no longer matches the prepared immutable Release".to_string(),
            ));
        }
        Ok(())
    }

    async fn execute_prepared(
        &self,
        request: &RelayRequest,
        adapter_session_id: &str,
        operation: &str,
        session: &PreparedPluginSession,
    ) -> Result<Value, (u16, String)> {
        match operation {
            operation if operation == self.hook_loader.operation() => {
                let event = request
                    .body
                    .get("event")
                    .cloned()
                    .ok_or_else(|| (400, "Plugin Hook event is required".to_string()))
                    .and_then(|value| {
                        serde_json::from_value(value).map_err(|error| {
                            (400, format!("Plugin Hook event is invalid: {error}"))
                        })
                    })?;
                let context = request
                    .body
                    .get("context")
                    .cloned()
                    .map(serde_json::from_value)
                    .transpose()
                    .map_err(|error| {
                        (
                            400,
                            format!("Plugin Hook event context is invalid: {error}"),
                        )
                    })?
                    .unwrap_or_default();
                let hook_set = session
                    .hooks
                    .get(session.component_key.as_str())
                    .ok_or_else(|| {
                        (
                            403,
                            "Plugin Hook operation was not published during prepare".to_string(),
                        )
                    })?;
                let workspace_write_decisions = self
                    .approve_hook_workspace_writes(
                        request,
                        adapter_session_id,
                        session,
                        hook_set,
                        event,
                        &context,
                    )
                    .await?;
                let result = self
                    .hook_loader
                    .dispatch(
                        hook_set,
                        &session.permission_snapshot,
                        session.run_id.as_str(),
                        event,
                        &context,
                        &workspace_write_decisions,
                    )
                    .await
                    .map_err(|error| (409, error.to_string()))?;
                Ok(json!({
                    "plugin_id": session.plugin_id,
                    "release_id": session.release_id,
                    "version": session.version,
                    "artifact_sha256": session.artifact_sha256,
                    "component_key": session.component_key,
                    "adapter_session_id": adapter_session_id,
                    "operation": operation,
                    "result": result,
                }))
            }
            LOAD_SKILL_RESOURCE_OPERATION => {
                let skill_key = required_body_text(&request.body, "skill_key")?;
                let relative_path = required_body_text(&request.body, "relative_path")?;
                let skill = session.skills.get(skill_key.as_str()).ok_or_else(|| {
                    (
                        403,
                        "Plugin Skill was not selected during prepare".to_string(),
                    )
                })?;
                let resource = skill
                    .resources
                    .iter()
                    .find(|resource| resource.relative_path == relative_path)
                    .ok_or_else(|| {
                        (
                            403,
                            "Plugin Skill resource was not published during prepare".to_string(),
                        )
                    })?;
                let content = self
                    .skill_loader
                    .load_text_resource(skill, relative_path.as_str())
                    .map_err(|error| (409, error.to_string()))?;
                Ok(json!({
                    "plugin_id": session.plugin_id,
                    "release_id": session.release_id,
                    "version": session.version,
                    "artifact_sha256": session.artifact_sha256,
                    "component_key": session.component_key,
                    "skill_key": skill_key,
                    "relative_path": relative_path,
                    "content_sha256": resource.sha256,
                    "content": content,
                    "adapter_session_id": adapter_session_id,
                    "operation": operation,
                }))
            }
            NATIVE_SKILL_TOOL_CALL_OPERATION => {
                let tool_name = required_body_text(&request.body, "tool_name")?;
                let arguments = request
                    .body
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                if !arguments.is_object() {
                    return Err((
                        400,
                        "native Plugin Skill tool arguments must be an object".to_string(),
                    ));
                }
                let native_skill = session.native_skill.as_ref().ok_or_else(|| {
                    (
                        403,
                        "native Plugin Skill operation was not published during prepare"
                            .to_string(),
                    )
                })?;
                if !native_skill.publishes_tool(tool_name.as_str()) {
                    return Err((
                        403,
                        format!(
                            "native Plugin Skill tool was not published during prepare: {tool_name}"
                        ),
                    ));
                }
                self.skill_loader
                    .validate_bundled_native_binding(&native_skill.binding)
                    .map_err(|error| (409, error.to_string()))?;
                let requires_interactive_approval =
                    crate::skills::native::requires_interactive_approval(
                        native_skill.binding.skill_id.as_str(),
                        tool_name.as_str(),
                    );
                let _action_guard = if requires_interactive_approval {
                    let guard = session.native_action_lock.lock().await;
                    self.load_exact_session(request, adapter_session_id)?;
                    Some(guard)
                } else {
                    None
                };
                let mut approved_command_args = None;
                if requires_interactive_approval {
                    let state = self.state_snapshot().await?;
                    let approval_command = crate::skills::native::approval_command(
                        native_skill.binding.skill_id.as_str(),
                        tool_name.as_str(),
                        &arguments,
                    )
                    .map_err(|error| (400, error.to_string()))?;
                    let command = approval_command.command;
                    let args = approval_command.args;
                    approved_command_args = Some(args.clone());
                    let approval = self
                        .approval_service()?
                        .approve_interactive(CommandApprovalRequest {
                            request_id: request.request_id.clone(),
                            project_key: approval_project_key_for_relay_scope(&state, request),
                            command,
                            args,
                            redact_arguments_in_history:
                                crate::skills::native::redact_approval_arguments(
                                    native_skill.binding.skill_id.as_str(),
                                    tool_name.as_str(),
                                ),
                            cwd: ".".to_string(),
                            source: match native_skill.binding.skill_id.as_str() {
                                "internal_skill_computer_use" => "plugin_computer_use",
                                "internal_skill_chrome" => "plugin_chrome_existing_session",
                                _ => "plugin_privileged_browser",
                            }
                            .to_string(),
                            requested_permissions: None,
                            session_id: Some(adapter_session_id.to_string()),
                            action_audit: approval_command.action_audit,
                        })
                        .await
                        .map_err(internal_error)?;
                    if let ApprovalDecision::Denied { reason, .. } = approval {
                        return Err((
                            403,
                            format!("Privileged local action was not approved: {reason}"),
                        ));
                    }
                    self.load_exact_session(request, adapter_session_id)?;
                }
                let artifact_arguments = arguments.clone();
                let state = self.state_snapshot().await?;
                let mut result = if requires_interactive_approval {
                    let skill_id = native_skill.binding.skill_id.clone();
                    let tool_name = tool_name.clone();
                    let request = request.clone();
                    let action_cancelled = session.native_action_cancelled.clone();
                    tokio::task::spawn_blocking(move || {
                        crate::skills::native::execute_approved(
                            skill_id.as_str(),
                            tool_name.as_str(),
                            &arguments,
                            &state,
                            &request,
                            approved_command_args.as_deref(),
                            Some(action_cancelled.as_ref()),
                        )
                    })
                    .await
                    .map_err(|error| {
                        (
                            500,
                            format!("Privileged local action worker failed: {error}"),
                        )
                    })?
                } else {
                    let skill_id = native_skill.binding.skill_id.clone();
                    let tool_name = tool_name.clone();
                    let request = request.clone();
                    let action_cancelled = session.native_action_cancelled.clone();
                    tokio::task::spawn_blocking(move || {
                        crate::skills::native::execute_with_cancellation(
                            skill_id.as_str(),
                            tool_name.as_str(),
                            &arguments,
                            &state,
                            &request,
                            Some(action_cancelled.as_ref()),
                        )
                    })
                    .await
                    .map_err(|error| (500, format!("Native local action worker failed: {error}")))?
                }
                .map_err(|error| (400, error.to_string()))?;
                let artifact_state = self.state_snapshot().await?;
                let artifacts = self
                    .artifact_store
                    .register_native_outputs(
                        &artifact_state,
                        request,
                        PluginArtifactProducer {
                            owner_user_id: session.owner_user_id.as_str(),
                            device_id: session.device_id.as_str(),
                            workspace_id: session.workspace_id.as_str(),
                            run_id: session.run_id.as_str(),
                            plugin_id: session.plugin_id.as_str(),
                            release_id: session.release_id.as_str(),
                            artifact_sha256: session.artifact_sha256.as_str(),
                            component_key: session.component_key.as_str(),
                            adapter_session_id,
                            skill_id: native_skill.binding.skill_id.as_str(),
                            tool_name: tool_name.as_str(),
                        },
                        &artifact_arguments,
                        &result,
                    )
                    .map_err(|error| (409, error.to_string()))?;
                if !artifacts.is_empty() {
                    result
                        .as_object_mut()
                        .ok_or_else(|| {
                            (
                                500,
                                "native Plugin Artifact result must be an object".to_string(),
                            )
                        })?
                        .insert("_plugin_artifacts".to_string(), json!(artifacts));
                }
                Ok(json!({
                    "plugin_id": session.plugin_id,
                    "release_id": session.release_id,
                    "version": session.version,
                    "artifact_sha256": session.artifact_sha256,
                    "component_key": session.component_key,
                    "skill_id": native_skill.binding.skill_id,
                    "bundle_id": native_skill.binding.bundle_id,
                    "bundle_version": native_skill.binding.bundle_version,
                    "bundle_hash": native_skill.binding.bundle_hash,
                    "tool_name": tool_name,
                    "result": result,
                    "adapter_session_id": adapter_session_id,
                    "operation": operation,
                }))
            }
            operation
                if session
                    .mcp
                    .as_ref()
                    .is_some_and(|mcp| mcp.health_operation() == operation) =>
            {
                let mcp = session
                    .mcp
                    .as_ref()
                    .context("prepared Plugin MCP session is unavailable")
                    .map_err(internal_error)?;
                mcp.validate_active()
                    .map_err(|error| (409, error.to_string()))?;
                let health = mcp.check_health().await.map_err(internal_error)?;
                Ok(json!({
                    "plugin_id": session.plugin_id,
                    "release_id": session.release_id,
                    "version": session.version,
                    "artifact_sha256": session.artifact_sha256,
                    "component_key": session.component_key,
                    "mcp_health": health,
                    "adapter_session_id": adapter_session_id,
                    "operation": operation,
                }))
            }
            operation
                if session
                    .mcp
                    .as_ref()
                    .is_some_and(|mcp| mcp.operation() == operation) =>
            {
                let tool_name = required_body_text(&request.body, "tool_name")?;
                let arguments = request
                    .body
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                if !arguments.is_object() {
                    return Err((
                        400,
                        "Plugin MCP tool arguments must be an object".to_string(),
                    ));
                }
                let mcp = session
                    .mcp
                    .as_ref()
                    .context("prepared Plugin MCP session is unavailable")
                    .map_err(internal_error)?;
                if !mcp.publishes_tool(tool_name.as_str()) {
                    return Err((
                        403,
                        format!("Plugin MCP tool was not published during prepare: {tool_name}"),
                    ));
                }
                mcp.validate_active()
                    .map_err(|error| (409, error.to_string()))?;
                let result = mcp
                    .call_tool(tool_name.as_str(), arguments)
                    .await
                    .map_err(|error| (502, error.to_string()))?;
                let health = mcp.health_snapshot().map_err(internal_error)?;
                Ok(json!({
                    "plugin_id": session.plugin_id,
                    "release_id": session.release_id,
                    "version": session.version,
                    "artifact_sha256": session.artifact_sha256,
                    "component_key": session.component_key,
                    "tool_name": tool_name,
                    "result": result,
                    "mcp_health": health,
                    "adapter_session_id": adapter_session_id,
                    "operation": operation,
                }))
            }
            _ => Err((
                403,
                format!("Plugin operation was not published during prepare: {operation}"),
            )),
        }
    }

    async fn cancel(&self, request: &RelayRequest) -> Result<Value, (u16, String)> {
        let adapter_session_id = required_body_text(&request.body, "adapter_session_id")?;
        let expected = ExactSessionIdentity::from_request(request)?;
        self.prune_expired_sessions();
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

fn decode_artifact_write_body(body_base64: &str) -> Result<Vec<u8>, (u16, String)> {
    let encoded_limit = PLUGIN_ARTIFACT_WRITE_MAX_BYTES
        .div_ceil(3)
        .saturating_mul(4) as usize;
    if body_base64.len() > encoded_limit {
        return Err((
            413,
            "Plugin Artifact write body exceeds the encoded size limit".to_string(),
        ));
    }
    let bytes = BASE64_STANDARD.decode(body_base64).map_err(|_| {
        (
            400,
            "Plugin Artifact write body is not valid canonical Base64".to_string(),
        )
    })?;
    if bytes.len() as u64 > PLUGIN_ARTIFACT_WRITE_MAX_BYTES
        || BASE64_STANDARD.encode(bytes.as_slice()) != body_base64
    {
        return Err((
            400,
            "Plugin Artifact write body is not canonical or exceeds the size limit".to_string(),
        ));
    }
    Ok(bytes)
}

fn approved_workspace_root(path: &std::path::Path) -> Result<PathBuf, (u16, String)> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| {
        (
            409,
            format!("read Plugin workspace metadata failed: {error}"),
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err((
            409,
            "Plugin workspace must be a non-symlink directory".to_string(),
        ));
    }
    crate::workspace::paths::canonicalize_existing_dir(path).map_err(internal_error)
}

fn workspace_write_permission_request(
    workspace_root: &std::path::Path,
) -> RequestPermissionProfile {
    let workspace_root = workspace_root.to_string_lossy().into_owned();
    RequestPermissionProfile {
        file_system: Some(AdditionalFileSystemPermissions {
            entries: Some(vec![
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Write,
                    path: FileSystemPath::Path {
                        path: workspace_root.clone(),
                    },
                },
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Deny,
                    path: FileSystemPath::Path {
                        path: std::path::Path::new(workspace_root.as_str())
                            .join(".git")
                            .to_string_lossy()
                            .into_owned(),
                    },
                },
            ]),
            ..AdditionalFileSystemPermissions::default()
        }),
        network: None,
    }
}

struct ExactSessionIdentity {
    run_id: String,
    owner_user_id: String,
    device_id: String,
    workspace_id: String,
    plugin_id: String,
    release_id: String,
    artifact_sha256: String,
    component_key: String,
}

impl ExactSessionIdentity {
    fn from_request(request: &RelayRequest) -> Result<Self, (u16, String)> {
        Ok(Self {
            run_id: required_body_text(&request.body, "run_id")?,
            owner_user_id: required_envelope_text(
                request.owner_user_id.as_deref(),
                "owner_user_id",
            )?,
            device_id: required_envelope_text(request.device_id.as_deref(), "device_id")?,
            workspace_id: request.workspace_id.trim().to_string(),
            plugin_id: required_body_text(&request.body, "plugin_id")?,
            release_id: required_body_text(&request.body, "release_id")?,
            artifact_sha256: required_sha256(&request.body, "artifact_sha256")?,
            component_key: required_body_text(&request.body, "component_key")?,
        })
    }

    fn validate(&self, session: &PreparedPluginSession) -> Result<(), (u16, String)> {
        if self.run_id != session.run_id
            || self.owner_user_id != session.owner_user_id
            || self.device_id != session.device_id
            || self.workspace_id != session.workspace_id
            || self.plugin_id != session.plugin_id
            || self.release_id != session.release_id
            || self.artifact_sha256 != session.artifact_sha256
            || self.component_key != session.component_key
        {
            return Err((
                409,
                "Plugin request snapshot does not match the prepared session".to_string(),
            ));
        }
        Ok(())
    }
}

impl PreparedPluginSession {
    fn telemetry_identity(&self) -> PluginRuntimeTelemetryIdentity {
        PluginRuntimeTelemetryIdentity {
            run_id: self.run_id.clone(),
            plugin_id: self.plugin_id.clone(),
            release_id: self.release_id.clone(),
            component_key: self.component_key.clone(),
        }
    }
}

fn telemetry_identity_from_request(
    request: &RelayRequest,
) -> Result<PluginRuntimeTelemetryIdentity, (u16, String)> {
    Ok(PluginRuntimeTelemetryIdentity {
        run_id: required_body_text(&request.body, "run_id")?,
        plugin_id: required_body_text(&request.body, "plugin_id")?,
        release_id: required_body_text(&request.body, "release_id")?,
        component_key: required_body_text(&request.body, "component_key")?,
    })
}

fn elapsed_millis(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn artifact_ui_access_from_body(body: &Value) -> Result<PluginArtifactUiAccess, (u16, String)> {
    let access = PluginArtifactUiAccess {
        run_id: required_body_text(body, "run_id")?,
        plugin_id: required_body_text(body, "plugin_id")?,
        release_id: required_body_text(body, "release_id")?,
        artifact_sha256: required_sha256(body, "artifact_sha256")?,
        component_key: required_body_text(body, "component_key")?,
        adapter_session_id: required_body_text(body, "adapter_session_id")?,
        ui_snapshot_sha256: required_sha256(body, "ui_snapshot_sha256")?,
    };
    Ok(access)
}

fn plugin_disabled_summary_sha256(
    installation: &crate::plugins::ActivePluginInstallation,
) -> String {
    hex::encode(Sha256::digest(
        format!(
            "chatos.plugin.disabled.v1\n{}\n{}\n{}",
            installation.plugin_id,
            installation.version.release_id,
            installation.version.artifact_sha256,
        )
        .as_bytes(),
    ))
}

fn session_audit_hash(session: &PreparedPluginSession) -> String {
    let mut payload = format!(
        "chatos.plugin.session.v4\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        session.run_id,
        session.owner_user_id,
        session.device_id,
        session.workspace_id,
        session.plugin_id,
        session.release_id,
        session.artifact_sha256,
        session.component_key,
    );
    for skill in session.skills.values() {
        payload.push('\n');
        payload.push_str(skill.snapshot_sha256.as_str());
    }
    for agent in session.agents.values() {
        payload.push('\n');
        payload.push_str(agent.snapshot_sha256.as_str());
    }
    for command in session.commands.values() {
        payload.push('\n');
        payload.push_str(command.snapshot_sha256.as_str());
    }
    for hook in session.hooks.values() {
        payload.push('\n');
        payload.push_str(hook.snapshot_sha256.as_str());
    }
    if let Some(ui) = &session.ui {
        payload.push('\n');
        payload.push_str(ui.snapshot_sha256.as_str());
    }
    if let Some(mcp) = &session.mcp {
        payload.push('\n');
        payload.push_str(mcp.snapshot().snapshot_sha256.as_str());
    }
    if let Some(native_skill) = &session.native_skill {
        payload.push('\n');
        payload.push_str(native_skill.binding.snapshot_sha256.as_str());
        payload.push('\n');
        payload.push_str(native_skill.tool_snapshot_sha256.as_str());
    }
    for permission in &session.permission_snapshot {
        payload.push('\n');
        payload.push_str(permission.as_str());
    }
    hex::encode(Sha256::digest(payload.as_bytes()))
}

fn native_tool_snapshot_sha256(
    binding: &PluginNativeSkillBindingSnapshot,
    tools: &[Value],
) -> Result<String> {
    let mut payload = format!(
        "chatos.plugin.native-tools.snapshot.v1\n{}",
        binding.snapshot_sha256
    );
    for tool in tools {
        payload.push('\n');
        payload.push_str(serde_json::to_string(tool)?.as_str());
    }
    Ok(hex::encode(Sha256::digest(payload.as_bytes())))
}
