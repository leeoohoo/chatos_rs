// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use async_trait::async_trait;
use chatos_mcp_runtime::{
    BuiltinToolProvider, McpBuiltinServer, ToolCallContext, ToolLifecycleEvent, ToolLifecycleHook,
    ToolLifecycleOutcome, ToolStreamChunkCallback,
};
use chatos_plugin_management_sdk::{
    normalized_plugin_hook_set_sha256, parse_plugin_hook_set, plugin_agent_snapshot_sha256,
    plugin_command_snapshot_sha256, plugin_hook_snapshot_sha256, plugin_ui_snapshot_sha256,
    PluginArtifactDescriptor, PluginArtifactReadyEventPayload, PluginComponentKind,
    PluginHookEvent, PluginHookEventContext, PluginHookSet, PluginUiReadyEventPayload,
    PluginUiSnapshot, RunPluginComponentSnapshot, RunPluginSnapshot, SystemAgentKey,
    PLUGIN_AGENT_MAX_ITERATIONS, PLUGIN_ARTIFACT_MAX_BYTES, PLUGIN_ARTIFACT_READY_EVENT_VERSION_V1,
    PLUGIN_COMMAND_MAX_ALLOWED_TOOLS, PLUGIN_COMMAND_MAX_TOOL_NAME_BYTES,
    PLUGIN_UI_ASSET_MAX_BYTES, PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1, PLUGIN_UI_HOST_CSP_V1,
    PLUGIN_UI_IFRAME_SANDBOX_V1, PLUGIN_UI_READY_EVENT_VERSION_V1, PLUGIN_UI_SURFACE_DETAIL_PANEL,
    PLUGIN_UI_TOTAL_ASSET_MAX_BYTES,
};
use chatos_service_runtime::http_body::read_response_bytes_limited;
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use super::RunService;
use crate::models::{TaskRecord, TaskRunEventRecord, TaskRunRecord};
use crate::store::AppStore;

const PLUGIN_RELAY_SCOPE: &str = "plugin.execute";
const LOCAL_CONNECTOR_TOKEN_AUDIENCE: &str = "local-connector-service";
const PLUGIN_RELAY_RESPONSE_LIMIT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Default)]
pub(in crate::services) struct PreparedPluginRuntime {
    pub builtin_servers: Vec<McpBuiltinServer>,
    pub providers: Vec<Arc<dyn BuiltinToolProvider>>,
    pub prompt_items: Vec<Value>,
    pub sessions: Vec<PreparedPluginSession>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::services) struct PluginHookLifecycleOutcome {
    pub blocking_failure: bool,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::services) struct PluginCommandExecutionConstraints {
    pub target_agent: Option<String>,
    pub tool_allowlists: Vec<Vec<String>>,
    pub agent_identity: Option<String>,
    pub max_iterations: Option<usize>,
}

pub(in crate::services) fn plugin_command_execution_constraints(
    run: &TaskRunRecord,
) -> Result<PluginCommandExecutionConstraints, String> {
    let mut constraints = PluginCommandExecutionConstraints::default();
    for plugin in &run.plugin_snapshots {
        for component in &plugin.component_snapshots {
            let identity = format!("{}:{}", plugin.plugin_id, component.component_key);
            match component.kind {
                PluginComponentKind::Command => {
                    let metadata = match component.runtime.get("metadata") {
                        Some(value) => Some(value.as_object().ok_or_else(|| {
                            format!("Plugin Command metadata must be an object: {identity}")
                        })?),
                        None => None,
                    };
                    let target_agent = command_target_agent(
                        metadata.and_then(|value| value.get("target_agent")),
                        identity.as_str(),
                    )?;
                    merge_target_agent(&mut constraints, target_agent, "Plugin Command")?;
                    let allowed_tools = component_allowed_tools(
                        metadata.and_then(|value| value.get("allowed_tools")),
                        identity.as_str(),
                        "Plugin Command",
                    )?;
                    if !allowed_tools.is_empty() {
                        constraints
                            .tool_allowlists
                            .push(allowed_tools.into_iter().collect());
                    }
                }
                PluginComponentKind::Agent => {
                    if let Some(existing) = constraints.agent_identity.as_deref() {
                        return Err(format!(
                            "a Run may select at most one Plugin Agent: {existing} and {identity}"
                        ));
                    }
                    let metadata = component
                        .runtime
                        .get("metadata")
                        .and_then(Value::as_object)
                        .ok_or_else(|| {
                            format!("Plugin Agent metadata must be an object: {identity}")
                        })?;
                    let base_agent =
                        agent_base_agent(metadata.get("base_agent"), identity.as_str())?;
                    merge_target_agent(&mut constraints, Some(base_agent), "Plugin Agent")?;
                    let allowed_tools = component_allowed_tools(
                        metadata.get("allowed_tools"),
                        identity.as_str(),
                        "Plugin Agent",
                    )?;
                    if !allowed_tools.is_empty() {
                        constraints
                            .tool_allowlists
                            .push(allowed_tools.into_iter().collect());
                    }
                    constraints.max_iterations = Some(agent_max_iterations(
                        metadata.get("max_iterations"),
                        identity.as_str(),
                    )?);
                    constraints.agent_identity = Some(identity);
                }
                _ => {}
            }
        }
    }
    Ok(constraints)
}

impl PreparedPluginRuntime {
    pub async fn cancel_all(&self) {
        for session in &self.sessions {
            let _ = session.cancel().await;
        }
    }

    pub async fn dispatch_hook_event(
        &self,
        event: PluginHookEvent,
        context: &PluginHookEventContext,
    ) -> PluginHookLifecycleOutcome {
        let mut outcome = PluginHookLifecycleOutcome::default();
        for session in self
            .sessions
            .iter()
            .filter(|session| session.component_kind == PluginComponentKind::HookSet)
        {
            match session.dispatch_hook_event(event, context).await {
                Ok(blocking_failure) => outcome.blocking_failure |= blocking_failure,
                Err(error) => {
                    outcome.blocking_failure = true;
                    outcome.errors.push(error);
                }
            }
        }
        outcome
    }

    pub fn tool_lifecycle_hook(&self, agent_key: &str) -> Option<Arc<dyn ToolLifecycleHook>> {
        self.sessions
            .iter()
            .any(|session| session.component_kind == PluginComponentKind::HookSet)
            .then(|| {
                Arc::new(PluginToolLifecycleHook {
                    sessions: self.sessions.clone(),
                    agent_key: agent_key.to_string(),
                    component_by_server: self
                        .sessions
                        .iter()
                        .filter(|session| session.component_kind != PluginComponentKind::HookSet)
                        .map(|session| {
                            (
                                plugin_server_name_from_identity(
                                    session.plugin_id.as_str(),
                                    session.component_key.as_str(),
                                ),
                                session.component_key.clone(),
                            )
                        })
                        .collect(),
                }) as Arc<dyn ToolLifecycleHook>
            })
    }
}

pub(in crate::services) async fn dispatch_prepared_plugin_hooks(
    sessions: &[PreparedPluginSession],
    event: PluginHookEvent,
    context: &PluginHookEventContext,
) -> PluginHookLifecycleOutcome {
    let runtime = PreparedPluginRuntime {
        sessions: sessions.to_vec(),
        ..PreparedPluginRuntime::default()
    };
    runtime.dispatch_hook_event(event, context).await
}

#[derive(Clone)]
struct PluginToolLifecycleHook {
    sessions: Vec<PreparedPluginSession>,
    agent_key: String,
    component_by_server: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PluginToolLifecycleStage {
    Pre,
    Post,
}

impl std::fmt::Debug for PluginToolLifecycleHook {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PluginToolLifecycleHook")
            .field("session_count", &self.sessions.len())
            .field("agent_key", &self.agent_key)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl ToolLifecycleHook for PluginToolLifecycleHook {
    async fn before_tool_use(&self, event: &ToolLifecycleEvent) -> Result<(), String> {
        let (hook_event, context) = self.map_event(event, PluginToolLifecycleStage::Pre);
        self.dispatch(hook_event, context).await
    }

    async fn after_tool_use(&self, event: &ToolLifecycleEvent) -> Result<(), String> {
        let (hook_event, context) = self.map_event(event, PluginToolLifecycleStage::Post);
        self.dispatch(hook_event, context).await
    }
}

impl PluginToolLifecycleHook {
    fn map_event(
        &self,
        event: &ToolLifecycleEvent,
        stage: PluginToolLifecycleStage,
    ) -> (PluginHookEvent, PluginHookEventContext) {
        let (hook_event, outcome, summary_sha256) = match stage {
            PluginToolLifecycleStage::Pre => (
                PluginHookEvent::PreToolUse,
                None,
                Some(event.arguments_sha256.clone()),
            ),
            PluginToolLifecycleStage::Post => (
                PluginHookEvent::PostToolUse,
                event.outcome.map(|outcome| match outcome {
                    ToolLifecycleOutcome::Succeeded => {
                        chatos_plugin_management_sdk::PluginHookOutcome::Succeeded
                    }
                    ToolLifecycleOutcome::Failed => {
                        chatos_plugin_management_sdk::PluginHookOutcome::Failed
                    }
                }),
                event.result_sha256.clone(),
            ),
        };
        (
            hook_event,
            PluginHookEventContext {
                agent_key: Some(self.agent_key.clone()),
                tool_name: Some(event.tool_name.clone()),
                tool_kind: Some(event.server_type.clone()),
                component_key: self.component_by_server.get(&event.server_name).cloned(),
                outcome,
                summary_sha256,
            },
        )
    }

    async fn dispatch(
        &self,
        event: PluginHookEvent,
        context: PluginHookEventContext,
    ) -> Result<(), String> {
        let outcome =
            dispatch_prepared_plugin_hooks(self.sessions.as_slice(), event, &context).await;
        if outcome.blocking_failure {
            let message = sanitize_runtime_error(hook_lifecycle_error(event, &outcome).as_str());
            if let Some(session) = self.sessions.first() {
                session
                    .relay
                    .store
                    .append_run_event_sync(TaskRunEventRecord::new(
                        session.relay.run_id.clone(),
                        "plugin_hook_blocked",
                        Some(message.clone()),
                        Some(json!({
                            "event": event,
                            "blocking_failure": true,
                            "tool_name": context.tool_name,
                            "tool_kind": context.tool_kind,
                            "component_key": context.component_key,
                            "summary_sha256": context.summary_sha256,
                        })),
                    ));
            }
            Err(message)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone)]
pub(in crate::services) struct PreparedPluginSession {
    relay: PluginRelayClient,
    plugin_id: String,
    release_id: String,
    artifact_sha256: String,
    component_key: String,
    adapter_session_id: String,
    component_kind: PluginComponentKind,
    operations: BTreeSet<String>,
    hook_snapshot_sha256: Option<String>,
    ui_snapshot: Option<PluginUiSnapshot>,
}

impl PreparedPluginSession {
    fn identity_body(&self) -> Map<String, Value> {
        Map::from_iter([
            ("plugin_id".to_string(), json!(self.plugin_id)),
            ("release_id".to_string(), json!(self.release_id)),
            ("artifact_sha256".to_string(), json!(self.artifact_sha256)),
            ("component_key".to_string(), json!(self.component_key)),
            (
                "adapter_session_id".to_string(),
                json!(self.adapter_session_id),
            ),
        ])
    }

    async fn execute_tool(
        &self,
        operation: &str,
        tool_name: &str,
        args: Value,
    ) -> Result<Value, String> {
        let mut body = self.identity_body();
        body.insert("operation".to_string(), json!(operation));
        body.insert("tool_name".to_string(), json!(tool_name));
        body.insert("arguments".to_string(), args);
        let response = self.relay.request("execute", Value::Object(body)).await?;
        response
            .get("result")
            .cloned()
            .ok_or_else(|| "Plugin execute response is missing result".to_string())
    }

    async fn dispatch_hook_event(
        &self,
        event: PluginHookEvent,
        context: &PluginHookEventContext,
    ) -> Result<bool, String> {
        if !self.operations.contains("dispatch_hook_event") {
            return Err(format!(
                "Plugin Hook session did not publish dispatch_hook_event: {}:{}",
                self.plugin_id, self.component_key
            ));
        }
        let mut body = self.identity_body();
        body.insert("operation".to_string(), json!("dispatch_hook_event"));
        body.insert("event".to_string(), json!(event));
        body.insert("context".to_string(), json!(context));
        let response = self.relay.request("execute", Value::Object(body)).await?;
        let result = response
            .get("result")
            .and_then(Value::as_object)
            .ok_or_else(|| "Plugin Hook execute response is missing result".to_string())?;
        if result.get("event") != Some(&json!(event))
            || result.get("snapshot_sha256").and_then(Value::as_str)
                != self.hook_snapshot_sha256.as_deref()
        {
            return Err(
                "Plugin Hook execute response does not match the prepared Hook snapshot"
                    .to_string(),
            );
        }
        result
            .get("blocking_failure")
            .and_then(Value::as_bool)
            .ok_or_else(|| "Plugin Hook execute response is missing blocking_failure".to_string())
    }

    async fn cancel(&self) -> Result<(), String> {
        self.relay
            .request("cancel", Value::Object(self.identity_body()))
            .await
            .map(|_| ())
    }

    fn record_ui_ready(&self) {
        let Some(ui) = self.ui_snapshot.as_ref() else {
            return;
        };
        let payload = PluginUiReadyEventPayload {
            event_schema_version: PLUGIN_UI_READY_EVENT_VERSION_V1,
            run_id: self.relay.run_id.clone(),
            device_id: self.relay.device_id.clone(),
            workspace_id: self.relay.workspace_id.clone(),
            plugin_id: self.plugin_id.clone(),
            release_id: self.release_id.clone(),
            artifact_sha256: self.artifact_sha256.clone(),
            component_key: self.component_key.clone(),
            adapter_session_id: self.adapter_session_id.clone(),
            ui: ui.clone(),
        };
        let Ok(payload) = serde_json::to_value(payload) else {
            return;
        };
        self.relay
            .store
            .append_run_event_sync(TaskRunEventRecord::new(
                self.relay.run_id.clone(),
                "plugin_ui_ready",
                Some(format!(
                    "Plugin UI ready: {} / {}",
                    self.plugin_id, self.component_key
                )),
                Some(payload),
            ));
    }

    fn record_artifacts_ready(&self, tool_name: &str, value: Option<Value>) -> Result<(), String> {
        let Some(value) = value else {
            return Ok(());
        };
        let artifacts =
            serde_json::from_value::<Vec<PluginArtifactDescriptor>>(value).map_err(|error| {
                format!("Plugin Artifact registration metadata is invalid: {error}")
            })?;
        if artifacts.is_empty() || artifacts.len() > 2 {
            return Err(
                "Plugin Artifact registration must contain between one and two files".to_string(),
            );
        }
        for artifact in artifacts {
            validate_registered_artifact(self, tool_name, &artifact)?;
            let payload = serde_json::to_value(PluginArtifactReadyEventPayload {
                event_schema_version: PLUGIN_ARTIFACT_READY_EVENT_VERSION_V1,
                artifact: artifact.clone(),
            })
            .map_err(|error| format!("encode Plugin Artifact event failed: {error}"))?;
            self.relay
                .store
                .append_run_event_sync(TaskRunEventRecord::new(
                    self.relay.run_id.clone(),
                    "plugin_artifact_ready",
                    Some(format!(
                        "Plugin Artifact ready: {} / {}",
                        self.plugin_id, artifact.display_name
                    )),
                    Some(payload),
                ));
        }
        Ok(())
    }
}

pub(in crate::services) async fn cancel_prepared_plugin_sessions(
    sessions: &[PreparedPluginSession],
) {
    for session in sessions {
        let _ = session.cancel().await;
    }
}

#[derive(Clone)]
struct PluginRelayClient {
    http: reqwest::Client,
    base_url: String,
    internal_secret: String,
    owner_user_id: String,
    device_id: String,
    workspace_id: Option<String>,
    run_id: String,
    store: AppStore,
    hook_dispatch_timeout: Duration,
}

impl PluginRelayClient {
    fn from_task(
        service: &RunService,
        task: &TaskRecord,
        run: &TaskRunRecord,
    ) -> Result<Self, String> {
        let base_url = plugin_relay_base_url()?;
        let internal_secret = service
            .config
            .local_connector_internal_api_secret
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                "TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET is required for Plugin execution"
                    .to_string()
            })?
            .to_string();
        let owner_user_id = task
            .owner_user_id
            .as_deref()
            .or(task.creator_user_id.as_deref())
            .or(Some(task.subject_id.as_str()))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "task owner user id is required for Plugin execution".to_string())?
            .to_string();
        let device_id = task
            .plugin_config
            .device_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "Plugin device_id is required for execution".to_string())?
            .to_string();
        let workspace_id = task
            .plugin_config
            .workspace_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let timeout_ms = std::env::var("TASK_RUNNER_PLUGIN_RELAY_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(60_000)
            .clamp(1_000, 120_000);
        let http = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| format!("build Plugin relay HTTP client failed: {error}"))?;
        let hook_dispatch_timeout_ms = std::env::var("TASK_RUNNER_PLUGIN_HOOK_RELAY_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(5 * 60 * 1_000 + 30_000)
            .clamp(45_000, 10 * 60 * 1_000);
        Ok(Self {
            http,
            base_url,
            internal_secret,
            owner_user_id,
            device_id,
            workspace_id,
            run_id: run.id.clone(),
            store: service.store.clone(),
            hook_dispatch_timeout: Duration::from_millis(hook_dispatch_timeout_ms),
        })
    }

    async fn request(&self, action: &str, mut body: Value) -> Result<Value, String> {
        let object = body
            .as_object_mut()
            .ok_or_else(|| "Plugin relay request body must be an object".to_string())?;
        object.insert("run_id".to_string(), json!(self.run_id));
        self.record_runtime_event(action, "started", &body, None, None, None);
        let started = Instant::now();
        let result = self.send_request(action, &body).await;
        match &result {
            Ok(response) => self.record_runtime_event(
                action,
                "succeeded",
                &body,
                Some(response),
                Some(elapsed_millis(started)),
                None,
            ),
            Err(error) => self.record_runtime_event(
                action,
                "failed",
                &body,
                None,
                Some(elapsed_millis(started)),
                Some(error.as_str()),
            ),
        }
        result
    }

    async fn send_request(&self, action: &str, body: &Value) -> Result<Value, String> {
        let token = chatos_service_runtime::issue_internal_service_token(
            self.internal_secret.as_str(),
            "task-runner",
            LOCAL_CONNECTOR_TOKEN_AUDIENCE,
            PLUGIN_RELAY_SCOPE,
            60,
        )
        .map_err(|error| format!("issue Plugin relay token failed: {error}"))?;
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-local-connector-caller",
            HeaderValue::from_static("task-runner"),
        );
        headers.insert(
            "x-local-connector-internal-token",
            HeaderValue::from_str(token.as_str())
                .map_err(|_| "Plugin relay token is not a valid header".to_string())?,
        );
        headers.insert(
            "x-local-connector-owner-user-id",
            HeaderValue::from_str(self.owner_user_id.as_str())
                .map_err(|_| "Plugin owner user id is not a valid header".to_string())?,
        );
        let mut url = format!(
            "{}/api/local-connectors/relay/{}/plugins/{}",
            self.base_url,
            urlencoding::encode(self.device_id.as_str()),
            action
        );
        if let Some(workspace_id) = self.workspace_id.as_deref() {
            url.push_str("?workspace_id=");
            url.push_str(urlencoding::encode(workspace_id).as_ref());
        }
        let mut request = self.http.post(url).headers(headers).json(body);
        if is_plugin_hook_dispatch(action, body) {
            request = request.timeout(self.hook_dispatch_timeout);
        }
        let response = request
            .send()
            .await
            .map_err(|error| format!("Plugin {action} relay request failed: {error}"))?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, PLUGIN_RELAY_RESPONSE_LIMIT_BYTES)
            .await
            .map_err(|error| format!("read Plugin {action} response failed: {error}"))?;
        let value = serde_json::from_slice::<Value>(bytes.as_slice())
            .map_err(|error| format!("decode Plugin {action} response failed: {error}"))?;
        if !status.is_success() {
            let message = value
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("Plugin relay rejected the request");
            return Err(format!("Plugin {action} failed with {status}: {message}"));
        }
        Ok(value)
    }

    fn record_runtime_event(
        &self,
        action: &str,
        status: &str,
        body: &Value,
        response: Option<&Value>,
        duration_ms: Option<u64>,
        error: Option<&str>,
    ) {
        let phase = if action == "execute"
            && body.get("operation").and_then(Value::as_str) == Some("mcp_health_check")
        {
            "health"
        } else {
            action
        };
        let mut payload = Map::from_iter([
            ("run_id".to_string(), json!(self.run_id)),
            ("phase".to_string(), json!(phase)),
            ("status".to_string(), json!(status)),
        ]);
        for field in [
            "plugin_id",
            "release_id",
            "component_key",
            "adapter_session_id",
            "operation",
            "tool_name",
        ] {
            let value = body
                .get(field)
                .and_then(Value::as_str)
                .or_else(|| response.and_then(|value| value.get(field).and_then(Value::as_str)));
            if let Some(value) = value {
                payload.insert(field.to_string(), json!(value));
            }
        }
        if let Some(health_status) = response
            .and_then(|value| value.pointer("/mcp_health/status"))
            .and_then(Value::as_str)
        {
            payload.insert("health_status".to_string(), json!(health_status));
        }
        if let Some(duration_ms) = duration_ms {
            payload.insert("duration_ms".to_string(), json!(duration_ms));
        }
        if let Some(error) = error {
            payload.insert("error".to_string(), json!(sanitize_runtime_error(error)));
        }
        if let Some(hook_dispatch) = response.and_then(|value| value.get("result")) {
            if body.get("operation").and_then(Value::as_str) == Some("dispatch_hook_event") {
                payload.insert("hook_dispatch".to_string(), hook_dispatch.clone());
            }
        }
        self.store.append_run_event_sync(TaskRunEventRecord::new(
            self.run_id.clone(),
            "plugin_runtime",
            Some(format!("Plugin {phase} {status}")),
            Some(Value::Object(payload)),
        ));
    }
}

fn is_plugin_hook_dispatch(action: &str, body: &Value) -> bool {
    action == "execute"
        && body.get("operation").and_then(Value::as_str) == Some("dispatch_hook_event")
}

#[derive(Clone)]
struct PluginRelayToolProvider {
    server_name: String,
    session: PreparedPluginSession,
    operation: String,
    tools: Vec<Value>,
}

#[async_trait]
impl BuiltinToolProvider for PluginRelayToolProvider {
    fn server_name(&self) -> &str {
        self.server_name.as_str()
    }

    fn list_tools(&self) -> Vec<Value> {
        self.tools.clone()
    }

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        context: ToolCallContext,
        _on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        let mut result = self
            .session
            .execute_tool(self.operation.as_str(), name, args)
            .await?;
        let artifacts = result
            .as_object_mut()
            .and_then(|result| result.remove("_plugin_artifacts"));
        self.session.record_artifacts_ready(name, artifacts)?;
        Ok(filter_transient_model_input_for_runtime(
            result,
            context
                .caller_model_runtime
                .as_ref()
                .and_then(|runtime| runtime.supports_images),
        ))
    }
}

fn validate_registered_artifact(
    session: &PreparedPluginSession,
    tool_name: &str,
    artifact: &PluginArtifactDescriptor,
) -> Result<(), String> {
    let owner = &artifact.owner;
    let workspace_id = session
        .relay
        .workspace_id
        .as_deref()
        .ok_or_else(|| "Plugin Artifact registration requires a workspace".to_string())?;
    if owner.owner_user_id != session.relay.owner_user_id
        || owner.run_id != session.relay.run_id
        || owner.device_id != session.relay.device_id
        || owner.workspace_id != workspace_id
        || owner.plugin_id != session.plugin_id
        || owner.release_id != session.release_id
        || owner.artifact_sha256 != session.artifact_sha256
        || owner.component_key != session.component_key
        || owner.adapter_session_id != session.adapter_session_id
        || artifact.producer_tool_name != tool_name
    {
        return Err("Plugin Artifact ownership does not match the prepared session".to_string());
    }
    if artifact.artifact_id.len() != 35
        || !artifact.artifact_id.starts_with("pa_")
        || !artifact.artifact_id[3..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        || artifact.display_name.trim().is_empty()
        || artifact.media_type.trim().is_empty()
        || artifact.size_bytes > PLUGIN_ARTIFACT_MAX_BYTES
        || !is_lower_sha256(artifact.sha256.as_str())
        || !artifact.downloadable
        || artifact.mutable
        || chrono::DateTime::parse_from_rfc3339(artifact.created_at.as_str()).is_err()
    {
        return Err("Plugin Artifact descriptor is invalid".to_string());
    }
    let path = Path::new(artifact.workspace_relative_path.as_str());
    if path.is_absolute()
        || artifact.workspace_relative_path.len() > 4_096
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || path.file_name().and_then(|value| value.to_str()) != Some(artifact.display_name.as_str())
    {
        return Err("Plugin Artifact workspace path is invalid".to_string());
    }
    Ok(())
}

fn filter_transient_model_input_for_runtime(
    mut result: Value,
    supports_images: Option<bool>,
) -> Value {
    if result.get("_model_input").is_some() && supports_images != Some(true) {
        if let Some(result) = result.as_object_mut() {
            result.remove("_model_input");
            result.insert(
                "text".to_string(),
                Value::String(
                    "The screenshot was captured, but the selected model does not declare image input support, so the image was not attached to the next model step."
                        .to_string(),
                ),
            );
            result.insert(
                "model_image_delivery".to_string(),
                json!({
                    "delivered": false,
                    "reason": "the selected model does not declare image input support"
                }),
            );
        }
    }
    result
}

fn elapsed_millis(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn sanitize_runtime_error(value: &str) -> String {
    const MAX_ERROR_BYTES: usize = 1024;
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut sanitized = String::new();
    for token in normalized.split(' ') {
        let lower = token.to_ascii_lowercase();
        let replacement = if lower.contains("://") {
            "<redacted-url>"
        } else if lower.contains("access_token")
            || lower.contains("refresh_token")
            || lower.contains("client_secret")
            || lower.starts_with("bearer")
            || lower.starts_with("password=")
        {
            "<redacted-secret>"
        } else {
            token
        };
        let separator = usize::from(!sanitized.is_empty());
        if sanitized
            .len()
            .saturating_add(separator)
            .saturating_add(replacement.len())
            > MAX_ERROR_BYTES
        {
            break;
        }
        if separator == 1 {
            sanitized.push(' ');
        }
        sanitized.push_str(replacement);
    }
    sanitized
}

impl RunService {
    pub(in crate::services) async fn prepare_plugin_runtime(
        &self,
        task: &TaskRecord,
        run: &TaskRunRecord,
        effective_workspace_dir: &str,
    ) -> Result<PreparedPluginRuntime, String> {
        if run.plugin_snapshots.is_empty() {
            return Ok(PreparedPluginRuntime::default());
        }
        let relay = PluginRelayClient::from_task(self, task, run)?;
        let mut prepared = PreparedPluginRuntime::default();
        for plugin in &run.plugin_snapshots {
            if plugin.device_id != relay.device_id
                || plugin.workspace_id.as_deref() != relay.workspace_id.as_deref()
            {
                prepared.cancel_all().await;
                return Err(format!(
                    "Run Plugin snapshot does not match selected device/workspace: {}",
                    plugin.plugin_id
                ));
            }
        }
        for plugin in &run.plugin_snapshots {
            for component in plugin
                .component_snapshots
                .iter()
                .filter(|component| component.kind == PluginComponentKind::HookSet)
            {
                match prepare_component(relay.clone(), plugin, component, effective_workspace_dir)
                    .await
                {
                    Ok(component) => prepared.extend(component),
                    Err(error) => {
                        prepared.cancel_all().await;
                        return Err(error);
                    }
                }
            }
        }
        let agent_key = crate::models::task_runner_agent_key_for(
            task.task_profile.as_str(),
            task.mcp_config.requires_execution,
        );
        let before_prepare = prepared
            .dispatch_hook_event(
                PluginHookEvent::BeforePluginPrepare,
                &PluginHookEventContext {
                    agent_key: Some(agent_key.as_str().to_string()),
                    ..PluginHookEventContext::default()
                },
            )
            .await;
        if before_prepare.blocking_failure {
            prepared.cancel_all().await;
            return Err(hook_lifecycle_error(
                PluginHookEvent::BeforePluginPrepare,
                &before_prepare,
            ));
        }
        for plugin in &run.plugin_snapshots {
            for component in plugin
                .component_snapshots
                .iter()
                .filter(|component| component.kind != PluginComponentKind::HookSet)
            {
                match prepare_component(relay.clone(), plugin, component, effective_workspace_dir)
                    .await
                {
                    Ok(component) => prepared.extend(component),
                    Err(error) => {
                        prepared.cancel_all().await;
                        return Err(error);
                    }
                }
            }
        }
        let session_start = prepared
            .dispatch_hook_event(
                PluginHookEvent::SessionStart,
                &PluginHookEventContext {
                    agent_key: Some(agent_key.as_str().to_string()),
                    ..PluginHookEventContext::default()
                },
            )
            .await;
        if session_start.blocking_failure {
            prepared.cancel_all().await;
            return Err(hook_lifecycle_error(
                PluginHookEvent::SessionStart,
                &session_start,
            ));
        }
        for session in &prepared.sessions {
            session.record_ui_ready();
        }
        Ok(prepared)
    }
}

fn hook_lifecycle_error(event: PluginHookEvent, outcome: &PluginHookLifecycleOutcome) -> String {
    if outcome.errors.is_empty() {
        format!("Plugin Hook {} failed with fail_run policy", event.as_str())
    } else {
        format!(
            "Plugin Hook {} dispatch failed: {}",
            event.as_str(),
            outcome.errors.join("; ")
        )
    }
}

struct PreparedComponent {
    server: Option<McpBuiltinServer>,
    provider: Option<Arc<dyn BuiltinToolProvider>>,
    prompt_items: Vec<Value>,
    session: PreparedPluginSession,
}

impl PreparedPluginRuntime {
    fn extend(&mut self, component: PreparedComponent) {
        if let Some(server) = component.server {
            self.builtin_servers.push(server);
        }
        if let Some(provider) = component.provider {
            self.providers.push(provider);
        }
        self.prompt_items.extend(component.prompt_items);
        self.sessions.push(component.session);
    }
}

async fn prepare_component(
    relay: PluginRelayClient,
    plugin: &RunPluginSnapshot,
    component: &RunPluginComponentSnapshot,
    effective_workspace_dir: &str,
) -> Result<PreparedComponent, String> {
    let mut body = Map::from_iter([
        ("plugin_id".to_string(), json!(plugin.plugin_id)),
        ("release_id".to_string(), json!(plugin.release_id)),
        ("artifact_sha256".to_string(), json!(plugin.artifact_sha256)),
        ("component_key".to_string(), json!(component.component_key)),
        (
            "permission_snapshot".to_string(),
            json!(plugin.permission_snapshot),
        ),
    ]);
    match component.kind {
        PluginComponentKind::SkillCollection => {
            let skill_keys = component
                .runtime
                .get("skill_keys")
                .and_then(Value::as_array)
                .filter(|values| !values.is_empty())
                .ok_or_else(|| {
                    format!(
                        "Plugin Skill component is missing immutable skill_keys: {}:{}",
                        plugin.plugin_id, component.component_key
                    )
                })?;
            body.insert("skill_keys".to_string(), Value::Array(skill_keys.clone()));
            let runtime_kind = component
                .runtime
                .get("runtime_kind")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    format!(
                        "Plugin Skill component is missing immutable runtime_kind: {}:{}",
                        plugin.plugin_id, component.component_key
                    )
                })?;
            body.insert("runtime_kind".to_string(), json!(runtime_kind));
            if let Some(metadata) = component.runtime.get("metadata") {
                body.insert("runtime_metadata".to_string(), metadata.clone());
            }
            body.insert(
                "content_sha256".to_string(),
                json!(component.content_sha256),
            );
        }
        PluginComponentKind::McpServer => {
            if let Some(server_key) = component.runtime.get("server_key") {
                body.insert("server_key".to_string(), server_key.clone());
            }
            for key in ["tool_allowlist", "tool_blocklist"] {
                if let Some(value) = component.runtime.get(key) {
                    body.insert(key.to_string(), value.clone());
                }
            }
        }
        PluginComponentKind::Command => {
            body.insert(
                "content_sha256".to_string(),
                json!(component.content_sha256),
            );
            if let Some(arguments) = component.runtime.get("arguments") {
                body.insert("arguments".to_string(), arguments.clone());
            }
        }
        PluginComponentKind::Agent => {
            body.insert(
                "content_sha256".to_string(),
                json!(component.content_sha256),
            );
        }
        PluginComponentKind::HookSet => {
            body.insert(
                "content_sha256".to_string(),
                json!(component.content_sha256),
            );
        }
        PluginComponentKind::UiContribution => {
            body.insert(
                "content_sha256".to_string(),
                json!(component.content_sha256),
            );
        }
        _ => {
            return Err(format!(
                "Plugin component runtime is not supported by Task Runner: {}:{}",
                plugin.plugin_id, component.component_key
            ));
        }
    }
    let response = relay.request("prepare", Value::Object(body)).await?;
    if response.get("run_id").and_then(Value::as_str) != Some(relay.run_id.as_str()) {
        return Err("Plugin prepare response run_id does not match the active Run".to_string());
    }
    validate_prepare_response(plugin, component, &response)?;
    let adapter_session_id = required_response_text(&response, "adapter_session_id")?;
    let operations = response
        .get("operations")
        .and_then(Value::as_array)
        .ok_or_else(|| "Plugin prepare response is missing operations".to_string())?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or_else(|| "Plugin prepare response contains an invalid operation".to_string())
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    let hook_snapshot_sha256 = if component.kind == PluginComponentKind::HookSet {
        Some(validate_hook_response(plugin, component, &response)?)
    } else {
        None
    };
    let ui_snapshot = if component.kind == PluginComponentKind::UiContribution {
        Some(validate_ui_response(plugin, component, &response)?)
    } else {
        None
    };
    if component.kind == PluginComponentKind::UiContribution && !operations.is_empty() {
        return Err(
            "Plugin UI prepare response must not publish executable operations before the isolated Workbench host is attached"
                .to_string(),
        );
    }
    let session = PreparedPluginSession {
        relay,
        plugin_id: plugin.plugin_id.clone(),
        release_id: plugin.release_id.clone(),
        artifact_sha256: plugin.artifact_sha256.clone(),
        component_key: component.component_key.clone(),
        adapter_session_id,
        component_kind: component.kind,
        operations,
        hook_snapshot_sha256,
        ui_snapshot,
    };
    let mut prompt_items = Vec::new();
    if let Some(skills) = response.get("skills").and_then(Value::as_array) {
        for skill in skills {
            let Some(instructions) = skill
                .get("instructions")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            let skill_key = skill
                .get("skill_key")
                .and_then(Value::as_str)
                .unwrap_or("plugin-skill");
            prompt_items.push(json!({
                "type": "message",
                "role": "system",
                "content": [{
                    "type": "input_text",
                    "text": format!(
                        "[Plugin Skill: {} / {} / {}]\n{}",
                        plugin.plugin_id, component.component_key, skill_key, instructions
                    )
                }]
            }));
        }
    }
    if component.kind == PluginComponentKind::Command {
        let commands = response
            .get("commands")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                "Plugin Command prepare response is missing the Command snapshot".to_string()
            })?;
        if commands.len() != 1 {
            return Err(
                "Plugin Command prepare response must contain exactly one Command".to_string(),
            );
        }
        let command = &commands[0];
        validate_command_response(plugin, component, command)?;
        let command_prompt = plugin_command_prompt_text(plugin, component, command)?;
        prompt_items.push(json!({
            "type": "message",
            "role": "system",
            "content": [{
                "type": "input_text",
                "text": command_prompt
            }]
        }));
        return Ok(PreparedComponent {
            server: None,
            provider: None,
            prompt_items,
            session,
        });
    }
    if component.kind == PluginComponentKind::HookSet {
        return Ok(PreparedComponent {
            server: None,
            provider: None,
            prompt_items,
            session,
        });
    }
    if component.kind == PluginComponentKind::UiContribution {
        return Ok(PreparedComponent {
            server: None,
            provider: None,
            prompt_items,
            session,
        });
    }
    if let Some(native_skill) = response
        .get("native_skill")
        .filter(|value| !value.is_null())
    {
        validate_native_skill_response(plugin, component, native_skill)?;
        let tools = native_skill
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .filter(|tools| !tools.is_empty())
            .ok_or_else(|| {
                "native Plugin Skill prepare response is missing executable tools".to_string()
            })?;
        let operation = response
            .get("operations")
            .and_then(Value::as_array)
            .and_then(|operations| {
                operations
                    .iter()
                    .filter_map(Value::as_str)
                    .find(|operation| *operation == "native_skill_tool_call")
            })
            .ok_or_else(|| {
                "native Plugin Skill prepare response did not publish native_skill_tool_call"
                    .to_string()
            })?
            .to_string();
        let server_name = plugin_server_name(plugin, component);
        let provider: Arc<dyn BuiltinToolProvider> = Arc::new(PluginRelayToolProvider {
            server_name: server_name.clone(),
            session: session.clone(),
            operation,
            tools,
        });
        let allow_writes = native_skill
            .get("permissions")
            .and_then(Value::as_array)
            .is_some_and(|permissions| {
                permissions
                    .iter()
                    .any(|permission| permission.as_str() == Some("workspace.write"))
            });
        return Ok(PreparedComponent {
            server: Some(plugin_relay_server(
                server_name,
                &session,
                effective_workspace_dir,
                allow_writes,
                "plugin_native_relay",
            )),
            provider: Some(provider),
            prompt_items,
            session,
        });
    }
    if component.kind == PluginComponentKind::Agent {
        let agents = response
            .get("agents")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                "Plugin Agent prepare response is missing the Agent snapshot".to_string()
            })?;
        if agents.len() != 1 {
            return Err("Plugin Agent prepare response must contain exactly one Agent".to_string());
        }
        let agent = &agents[0];
        validate_agent_response(plugin, component, agent)?;
        let agent_prompt = plugin_agent_prompt_text(plugin, component, agent)?;
        prompt_items.push(json!({
            "type": "message",
            "role": "system",
            "content": [{
                "type": "input_text",
                "text": agent_prompt
            }]
        }));
        return Ok(PreparedComponent {
            server: None,
            provider: None,
            prompt_items,
            session,
        });
    }
    let Some(mcp) = response.get("mcp").filter(|value| !value.is_null()) else {
        return Ok(PreparedComponent {
            server: None,
            provider: None,
            prompt_items,
            session,
        });
    };
    let tools = mcp
        .get("tools")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| "Plugin MCP prepare response is missing tools".to_string())?;
    let operation = response
        .get("operations")
        .and_then(Value::as_array)
        .and_then(|operations| {
            operations
                .iter()
                .filter_map(Value::as_str)
                .find(|operation| *operation == "mcp_tools_call")
        })
        .ok_or_else(|| "Plugin MCP prepare response did not publish mcp_tools_call".to_string())?
        .to_string();
    let server_name = plugin_server_name(plugin, component);
    let provider: Arc<dyn BuiltinToolProvider> = Arc::new(PluginRelayToolProvider {
        server_name: server_name.clone(),
        session: session.clone(),
        operation,
        tools,
    });
    Ok(PreparedComponent {
        server: Some(plugin_relay_server(
            server_name,
            &session,
            effective_workspace_dir,
            false,
            "plugin_relay",
        )),
        provider: Some(provider),
        prompt_items,
        session,
    })
}

fn plugin_relay_server(
    name: String,
    session: &PreparedPluginSession,
    effective_workspace_dir: &str,
    allow_writes: bool,
    kind: &str,
) -> McpBuiltinServer {
    let native_relay = kind == "plugin_native_relay";
    McpBuiltinServer {
        name,
        kind: kind.to_string(),
        workspace_dir: effective_workspace_dir.to_string(),
        user_id: Some(session.relay.owner_user_id.clone()),
        project_id: None,
        remote_connection_id: None,
        contact_agent_id: None,
        auto_create_task: false,
        allow_writes,
        max_file_bytes: if native_relay { 2 * 1024 * 1024 } else { 0 },
        max_write_bytes: if native_relay && allow_writes {
            2 * 1024 * 1024
        } else {
            0
        },
        search_limit: 0,
    }
}

fn validate_prepare_response(
    plugin: &RunPluginSnapshot,
    component: &RunPluginComponentSnapshot,
    response: &Value,
) -> Result<(), String> {
    for (field, expected) in [
        ("plugin_id", plugin.plugin_id.as_str()),
        ("release_id", plugin.release_id.as_str()),
        ("version", plugin.version.as_str()),
        ("artifact_sha256", plugin.artifact_sha256.as_str()),
        ("component_key", component.component_key.as_str()),
    ] {
        if response.get(field).and_then(Value::as_str) != Some(expected) {
            return Err(format!(
                "Plugin prepare response {field} does not match the immutable Run snapshot"
            ));
        }
    }
    let actual_permissions = response
        .get("permission_snapshot")
        .and_then(Value::as_array)
        .ok_or_else(|| "Plugin prepare response is missing permission_snapshot".to_string())?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or_else(|| {
                    "Plugin prepare response contains an invalid permission snapshot".to_string()
                })
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    let expected_permissions = plugin
        .permission_snapshot
        .iter()
        .map(|permission| permission.trim().to_string())
        .collect::<BTreeSet<_>>();
    if actual_permissions != expected_permissions {
        return Err(
            "Plugin prepare response permission_snapshot does not match the immutable Run snapshot"
                .to_string(),
        );
    }
    Ok(())
}

fn validate_hook_response(
    plugin: &RunPluginSnapshot,
    component: &RunPluginComponentSnapshot,
    response: &Value,
) -> Result<String, String> {
    let hooks = response
        .get("hooks")
        .and_then(Value::as_array)
        .ok_or_else(|| "Plugin Hook prepare response is missing the Hook snapshot".to_string())?;
    if hooks.len() != 1 {
        return Err("Plugin Hook prepare response must contain exactly one Hook set".to_string());
    }
    let hook = &hooks[0];
    for (field, expected) in [
        ("plugin_id", plugin.plugin_id.as_str()),
        ("release_id", plugin.release_id.as_str()),
        ("version", plugin.version.as_str()),
        ("artifact_sha256", plugin.artifact_sha256.as_str()),
        ("component_key", component.component_key.as_str()),
        ("content_sha256", component.content_sha256.as_str()),
    ] {
        if hook.get(field).and_then(Value::as_str) != Some(expected) {
            return Err(format!(
                "Plugin Hook prepare response {field} does not match the immutable Run snapshot"
            ));
        }
    }
    let expected_entrypoint = component
        .runtime
        .get("entrypoint")
        .and_then(Value::as_str)
        .ok_or_else(|| "Plugin Hook Run snapshot is missing entrypoint".to_string())?;
    if hook.get("relative_source_path").and_then(Value::as_str) != Some(expected_entrypoint) {
        return Err(
            "Plugin Hook source does not match the immutable component entrypoint".to_string(),
        );
    }
    let hook_set: PluginHookSet = serde_json::from_value(
        hook.get("hook_set")
            .cloned()
            .ok_or_else(|| "Plugin Hook prepare response is missing hook_set".to_string())?,
    )
    .map_err(|error| format!("Plugin Hook prepare response hook_set is invalid: {error}"))?;
    let canonical_hook_set = parse_plugin_hook_set(
        serde_json::to_string(&hook_set)
            .map_err(|error| format!("encode Plugin Hook set failed: {error}"))?
            .as_str(),
    )
    .map_err(|error| format!("Plugin Hook set validation failed: {error}"))?;
    if canonical_hook_set != hook_set {
        return Err("Plugin Hook prepare response is not canonically normalized".to_string());
    }
    let hook_set_sha256 = normalized_plugin_hook_set_sha256(&hook_set)
        .map_err(|error| format!("hash Plugin Hook set failed: {error}"))?;
    if hook.get("hook_set_sha256").and_then(Value::as_str) != Some(hook_set_sha256.as_str()) {
        return Err("Plugin Hook set hash does not match its normalized snapshot".to_string());
    }
    let command_sha256_by_hook = serde_json::from_value::<BTreeMap<String, String>>(
        hook.get("command_sha256_by_hook").cloned().ok_or_else(|| {
            "Plugin Hook prepare response is missing command_sha256_by_hook".to_string()
        })?,
    )
    .map_err(|error| format!("Plugin Hook command snapshot is invalid: {error}"))?;
    if command_sha256_by_hook.len() != hook_set.hooks.len()
        || command_sha256_by_hook
            .values()
            .any(|value| !is_lower_sha256(value))
        || hook_set
            .hooks
            .iter()
            .any(|definition| !command_sha256_by_hook.contains_key(definition.id.as_str()))
    {
        return Err("Plugin Hook command hashes do not cover the normalized Hook set".to_string());
    }
    let expected_snapshot_sha256 = plugin_hook_snapshot_sha256(
        plugin.plugin_id.as_str(),
        plugin.release_id.as_str(),
        component.component_key.as_str(),
        expected_entrypoint,
        component.content_sha256.as_str(),
        hook_set_sha256.as_str(),
        &command_sha256_by_hook,
    )
    .map_err(|error| format!("hash Plugin Hook snapshot failed: {error}"))?;
    if hook.get("snapshot_sha256").and_then(Value::as_str)
        != Some(expected_snapshot_sha256.as_str())
    {
        return Err(
            "Plugin Hook snapshot hash does not match the immutable Run snapshot".to_string(),
        );
    }
    let operations = response
        .get("operations")
        .and_then(Value::as_array)
        .ok_or_else(|| "Plugin Hook prepare response is missing operations".to_string())?;
    if !operations
        .iter()
        .any(|operation| operation.as_str() == Some("dispatch_hook_event"))
    {
        return Err("Plugin Hook prepare response did not publish dispatch_hook_event".to_string());
    }
    Ok(expected_snapshot_sha256)
}

fn validate_ui_response(
    plugin: &RunPluginSnapshot,
    component: &RunPluginComponentSnapshot,
    response: &Value,
) -> Result<PluginUiSnapshot, String> {
    let values = response
        .get("ui")
        .and_then(Value::as_array)
        .ok_or_else(|| "Plugin UI prepare response is missing the UI snapshot".to_string())?;
    if values.len() != 1 {
        return Err("Plugin UI prepare response must contain exactly one UI snapshot".to_string());
    }
    let snapshot: PluginUiSnapshot = serde_json::from_value(values[0].clone())
        .map_err(|error| format!("Plugin UI prepare response is invalid: {error}"))?;
    for (field, actual, expected) in [
        (
            "plugin_id",
            snapshot.plugin_id.as_str(),
            plugin.plugin_id.as_str(),
        ),
        (
            "release_id",
            snapshot.release_id.as_str(),
            plugin.release_id.as_str(),
        ),
        (
            "version",
            snapshot.version.as_str(),
            plugin.version.as_str(),
        ),
        (
            "artifact_sha256",
            snapshot.artifact_sha256.as_str(),
            plugin.artifact_sha256.as_str(),
        ),
        (
            "component_key",
            snapshot.component_key.as_str(),
            component.component_key.as_str(),
        ),
        (
            "content_sha256",
            snapshot.content_sha256.as_str(),
            component.content_sha256.as_str(),
        ),
    ] {
        if actual != expected {
            return Err(format!(
                "Plugin UI prepare response {field} does not match the immutable Run snapshot"
            ));
        }
    }
    let expected_entrypoint = component
        .runtime
        .get("entrypoint")
        .and_then(Value::as_str)
        .ok_or_else(|| "Plugin UI Run snapshot is missing entrypoint".to_string())?;
    if snapshot.relative_source_path != expected_entrypoint {
        return Err(
            "Plugin UI source does not match the immutable component entrypoint".to_string(),
        );
    }
    let metadata = component.runtime.get("metadata").and_then(Value::as_object);
    let expected_title = metadata
        .and_then(|metadata| metadata.get("title"))
        .and_then(Value::as_str)
        .unwrap_or(component.component_key.as_str());
    let expected_surface = metadata
        .and_then(|metadata| metadata.get("surface"))
        .and_then(Value::as_str)
        .unwrap_or(PLUGIN_UI_SURFACE_DETAIL_PANEL);
    let expected_assets = metadata_string_array(
        metadata.and_then(|metadata| metadata.get("assets")),
        "Plugin UI immutable asset paths",
    )?;
    let actual_assets = snapshot
        .assets
        .iter()
        .map(|asset| asset.relative_path.clone())
        .collect::<Vec<_>>();
    if actual_assets != expected_assets {
        return Err("Plugin UI assets do not match the immutable Run snapshot".to_string());
    }
    let expected_capabilities = metadata_string_array(
        metadata.and_then(|metadata| metadata.get("bridge_capabilities")),
        "Plugin UI immutable bridge capabilities",
    )?;
    let expected_mime_types = metadata_string_array(
        metadata.and_then(|metadata| metadata.get("artifact_mime_types")),
        "Plugin UI immutable artifact MIME types",
    )?;
    if snapshot.title != expected_title
        || snapshot.surface != expected_surface
        || snapshot.bridge_capabilities != expected_capabilities
        || snapshot.artifact_mime_types != expected_mime_types
    {
        return Err("Plugin UI metadata does not match the immutable Run snapshot".to_string());
    }
    if snapshot.bridge_protocol_version != PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1
        || snapshot.content_security_policy != PLUGIN_UI_HOST_CSP_V1
        || snapshot.iframe_sandbox != PLUGIN_UI_IFRAME_SANDBOX_V1
    {
        return Err("Plugin UI Host security contract is invalid".to_string());
    }
    let mut total_asset_bytes = 0_u64;
    let mut asset_paths = BTreeSet::new();
    for asset in &snapshot.assets {
        if !asset_paths.insert(asset.relative_path.as_str())
            || !is_lower_sha256(asset.sha256.as_str())
            || asset.size_bytes > PLUGIN_UI_ASSET_MAX_BYTES
            || asset.media_type.trim().is_empty()
        {
            return Err("Plugin UI asset snapshot is invalid".to_string());
        }
        total_asset_bytes = total_asset_bytes
            .checked_add(asset.size_bytes)
            .ok_or_else(|| "Plugin UI total asset size overflow".to_string())?;
    }
    if total_asset_bytes > PLUGIN_UI_TOTAL_ASSET_MAX_BYTES {
        return Err("Plugin UI assets exceed the total size limit".to_string());
    }
    if !is_lower_sha256(snapshot.content_sha256.as_str()) {
        return Err("Plugin UI entrypoint snapshot is invalid".to_string());
    }
    let expected_snapshot_sha256 = plugin_ui_snapshot_sha256(
        plugin.plugin_id.as_str(),
        plugin.release_id.as_str(),
        component.component_key.as_str(),
        snapshot.title.as_str(),
        snapshot.surface.as_str(),
        expected_entrypoint,
        component.content_sha256.as_str(),
        snapshot.assets.as_slice(),
        PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1,
        snapshot.bridge_capabilities.as_slice(),
        snapshot.artifact_mime_types.as_slice(),
        PLUGIN_UI_HOST_CSP_V1,
        PLUGIN_UI_IFRAME_SANDBOX_V1,
    )
    .map_err(|error| format!("hash Plugin UI snapshot failed: {error}"))?;
    if snapshot.snapshot_sha256 != expected_snapshot_sha256 {
        return Err(
            "Plugin UI snapshot hash does not match the immutable Run snapshot".to_string(),
        );
    }
    Ok(snapshot)
}

fn metadata_string_array(value: Option<&Value>, field: &str) -> Result<Vec<String>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| format!("{field} must be an array"))?;
    let mut result = Vec::with_capacity(values.len());
    for value in values {
        let value = value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("{field} contains an invalid item"))?;
        result.push(value.to_string());
    }
    Ok(result)
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn validate_command_response(
    plugin: &RunPluginSnapshot,
    component: &RunPluginComponentSnapshot,
    command: &Value,
) -> Result<(), String> {
    for (field, expected) in [
        ("plugin_id", plugin.plugin_id.as_str()),
        ("release_id", plugin.release_id.as_str()),
        ("version", plugin.version.as_str()),
        ("artifact_sha256", plugin.artifact_sha256.as_str()),
        ("component_key", component.component_key.as_str()),
        ("command_name", component.component_key.as_str()),
        ("content_sha256", component.content_sha256.as_str()),
    ] {
        if command.get(field).and_then(Value::as_str) != Some(expected) {
            return Err(format!(
                "Plugin Command prepare response {field} does not match the immutable Run snapshot"
            ));
        }
    }
    let expected_entrypoint = component
        .runtime
        .get("entrypoint")
        .and_then(Value::as_str)
        .ok_or_else(|| "Plugin Command Run snapshot is missing entrypoint".to_string())?;
    if command.get("relative_source_path").and_then(Value::as_str) != Some(expected_entrypoint) {
        return Err(
            "Plugin Command source does not match the immutable component entrypoint".to_string(),
        );
    }
    let metadata = component.runtime.get("metadata");
    for field in ["description", "argument_hint"] {
        let expected = metadata
            .and_then(|metadata| metadata.get(field))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let actual = command
            .get(field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if actual != expected {
            return Err(format!(
                "Plugin Command prepare response {field} does not match the immutable Run snapshot"
            ));
        }
    }
    let requires_confirmation = component
        .runtime
        .get("metadata")
        .and_then(|metadata| metadata.get("requires_confirmation"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if command
        .get("requires_confirmation")
        .and_then(Value::as_bool)
        != Some(requires_confirmation)
    {
        return Err(
            "Plugin Command confirmation requirement does not match the immutable Run snapshot"
                .to_string(),
        );
    }
    let expected_target_agent = command_target_agent(
        metadata.and_then(|value| value.get("target_agent")),
        "immutable Run snapshot",
    )?;
    let actual_target_agent = command_target_agent(
        Some(command.get("target_agent").ok_or_else(|| {
            "Plugin Command prepare response is missing target_agent".to_string()
        })?),
        "prepare response",
    )?;
    if actual_target_agent != expected_target_agent {
        return Err(
            "Plugin Command target Agent does not match the immutable Run snapshot".to_string(),
        );
    }
    let expected_allowed_tools = component_allowed_tools(
        metadata.and_then(|value| value.get("allowed_tools")),
        "immutable Run snapshot",
        "Plugin Command",
    )?;
    let actual_allowed_tools = component_allowed_tools(
        Some(command.get("allowed_tools").ok_or_else(|| {
            "Plugin Command prepare response is missing allowed_tools".to_string()
        })?),
        "prepare response",
        "Plugin Command",
    )?;
    if actual_allowed_tools != expected_allowed_tools {
        return Err(
            "Plugin Command allowed tools do not match the immutable Run snapshot".to_string(),
        );
    }
    let confirmation_approved = command
        .get("confirmation_approved")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if confirmation_approved != requires_confirmation {
        return Err(
            "Plugin Command confirmation result does not match the immutable Run snapshot"
                .to_string(),
        );
    }
    let arguments = component
        .runtime
        .get("arguments")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if command.get("arguments_present").and_then(Value::as_bool) != Some(arguments.is_some()) {
        return Err(
            "Plugin Command argument presence does not match the immutable Run snapshot"
                .to_string(),
        );
    }
    let expected_arguments_sha256 =
        hex::encode(Sha256::digest(arguments.unwrap_or_default().as_bytes()));
    if command.get("arguments_sha256").and_then(Value::as_str)
        != Some(expected_arguments_sha256.as_str())
    {
        return Err("Plugin Command arguments do not match the immutable Run snapshot".to_string());
    }
    if command.get("arguments").is_some() {
        return Err("Plugin Command prepare response must not echo arguments".to_string());
    }
    let prompt = required_response_text(command, "prompt")?;
    let allowed_tools = expected_allowed_tools.into_iter().collect::<Vec<_>>();
    let expected_snapshot_sha256 = plugin_command_snapshot_sha256(
        plugin.plugin_id.as_str(),
        plugin.release_id.as_str(),
        component.component_key.as_str(),
        expected_entrypoint,
        metadata
            .and_then(|value| value.get("description"))
            .and_then(Value::as_str),
        metadata
            .and_then(|value| value.get("argument_hint"))
            .and_then(Value::as_str),
        requires_confirmation,
        expected_target_agent,
        allowed_tools.as_slice(),
        component.content_sha256.as_str(),
        prompt.as_str(),
        expected_arguments_sha256.as_str(),
    )
    .map_err(|error| format!("hash Plugin Command snapshot failed: {error}"))?;
    if command.get("snapshot_sha256").and_then(Value::as_str)
        != Some(expected_snapshot_sha256.as_str())
    {
        return Err(
            "Plugin Command snapshot_sha256 does not match the immutable Run snapshot".to_string(),
        );
    }
    Ok(())
}

fn validate_agent_response(
    plugin: &RunPluginSnapshot,
    component: &RunPluginComponentSnapshot,
    agent: &Value,
) -> Result<(), String> {
    for (field, expected) in [
        ("plugin_id", plugin.plugin_id.as_str()),
        ("release_id", plugin.release_id.as_str()),
        ("version", plugin.version.as_str()),
        ("artifact_sha256", plugin.artifact_sha256.as_str()),
        ("component_key", component.component_key.as_str()),
        ("agent_name", component.component_key.as_str()),
        ("content_sha256", component.content_sha256.as_str()),
    ] {
        if agent.get(field).and_then(Value::as_str) != Some(expected) {
            return Err(format!(
                "Plugin Agent prepare response {field} does not match the immutable Run snapshot"
            ));
        }
    }
    let expected_entrypoint = component
        .runtime
        .get("entrypoint")
        .and_then(Value::as_str)
        .ok_or_else(|| "Plugin Agent Run snapshot is missing entrypoint".to_string())?;
    if agent.get("relative_source_path").and_then(Value::as_str) != Some(expected_entrypoint) {
        return Err(
            "Plugin Agent source does not match the immutable component entrypoint".to_string(),
        );
    }
    let metadata = component
        .runtime
        .get("metadata")
        .and_then(Value::as_object)
        .ok_or_else(|| "Plugin Agent Run snapshot is missing metadata".to_string())?;
    let expected_description = metadata
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let actual_description = agent
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if actual_description != expected_description {
        return Err(
            "Plugin Agent description does not match the immutable Run snapshot".to_string(),
        );
    }
    let expected_base_agent =
        agent_base_agent(metadata.get("base_agent"), "immutable Run snapshot")?;
    let actual_base_agent = agent_base_agent(agent.get("base_agent"), "prepare response")?;
    if actual_base_agent != expected_base_agent {
        return Err(
            "Plugin Agent base Agent does not match the immutable Run snapshot".to_string(),
        );
    }
    let expected_allowed_tools = component_allowed_tools(
        metadata.get("allowed_tools"),
        "immutable Run snapshot",
        "Plugin Agent",
    )?;
    let actual_allowed_tools =
        component_allowed_tools(
            Some(agent.get("allowed_tools").ok_or_else(|| {
                "Plugin Agent prepare response is missing allowed_tools".to_string()
            })?),
            "prepare response",
            "Plugin Agent",
        )?;
    if actual_allowed_tools != expected_allowed_tools {
        return Err(
            "Plugin Agent allowed tools do not match the immutable Run snapshot".to_string(),
        );
    }
    let expected_max_iterations =
        agent_max_iterations(metadata.get("max_iterations"), "immutable Run snapshot")?;
    let actual_max_iterations =
        agent_max_iterations(agent.get("max_iterations"), "prepare response")?;
    if actual_max_iterations != expected_max_iterations {
        return Err(
            "Plugin Agent max iterations do not match the immutable Run snapshot".to_string(),
        );
    }
    let prompt = required_response_text(agent, "prompt")?;
    let allowed_tools = expected_allowed_tools.into_iter().collect::<Vec<_>>();
    let expected_snapshot_sha256 = plugin_agent_snapshot_sha256(
        plugin.plugin_id.as_str(),
        plugin.release_id.as_str(),
        component.component_key.as_str(),
        expected_entrypoint,
        expected_description,
        expected_base_agent,
        allowed_tools.as_slice(),
        expected_max_iterations,
        component.content_sha256.as_str(),
        prompt.as_str(),
    )
    .map_err(|error| format!("hash Plugin Agent snapshot failed: {error}"))?;
    if agent.get("snapshot_sha256").and_then(Value::as_str)
        != Some(expected_snapshot_sha256.as_str())
    {
        return Err(
            "Plugin Agent snapshot_sha256 does not match the immutable Run snapshot".to_string(),
        );
    }
    Ok(())
}

fn merge_target_agent(
    constraints: &mut PluginCommandExecutionConstraints,
    target_agent: Option<&str>,
    component_label: &str,
) -> Result<(), String> {
    let Some(target_agent) = target_agent else {
        return Ok(());
    };
    if let Some(existing) = constraints.target_agent.as_deref() {
        if existing != target_agent {
            return Err(format!(
                "selected Plugin components require different target Agents: {existing} and {target_agent} ({component_label})"
            ));
        }
    } else {
        constraints.target_agent = Some(target_agent.to_string());
    }
    Ok(())
}

fn agent_base_agent<'a>(value: Option<&'a Value>, context: &str) -> Result<&'a str, String> {
    let raw_base_agent = value
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Plugin Agent base Agent is missing or invalid in {context}"))?;
    let base_agent = raw_base_agent.trim();
    if base_agent.is_empty() || base_agent != raw_base_agent {
        return Err(format!(
            "Plugin Agent base Agent is not normalized in {context}"
        ));
    }
    if ![
        SystemAgentKey::TaskRunnerPlanPhase.as_str(),
        SystemAgentKey::TaskRunnerLocalPlanPhase.as_str(),
        SystemAgentKey::TaskRunnerRunPhase.as_str(),
        SystemAgentKey::TaskRunnerLocalRunPhase.as_str(),
    ]
    .contains(&base_agent)
    {
        return Err(format!(
            "Plugin Agent base Agent is unsupported in {context}: {base_agent}"
        ));
    }
    Ok(base_agent)
}

fn agent_max_iterations(value: Option<&Value>, context: &str) -> Result<usize, String> {
    let value = value
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| format!("Plugin Agent max iterations is missing or invalid in {context}"))?;
    if !(1..=PLUGIN_AGENT_MAX_ITERATIONS).contains(&value) {
        return Err(format!(
            "Plugin Agent max iterations must be between 1 and {PLUGIN_AGENT_MAX_ITERATIONS} in {context}"
        ));
    }
    Ok(value)
}

fn command_target_agent<'a>(
    value: Option<&'a Value>,
    context: &str,
) -> Result<Option<&'a str>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let raw_target_agent = value
        .as_str()
        .ok_or_else(|| format!("Plugin Command target Agent is invalid in {context}"))?;
    let target_agent = raw_target_agent.trim();
    if target_agent.is_empty() || target_agent != raw_target_agent {
        return Err(format!(
            "Plugin Command target Agent is not normalized in {context}"
        ));
    }
    if ![
        SystemAgentKey::TaskRunnerPlanPhase.as_str(),
        SystemAgentKey::TaskRunnerLocalPlanPhase.as_str(),
        SystemAgentKey::TaskRunnerRunPhase.as_str(),
        SystemAgentKey::TaskRunnerLocalRunPhase.as_str(),
    ]
    .contains(&target_agent)
    {
        return Err(format!(
            "Plugin Command target Agent is unsupported in {context}: {target_agent}"
        ));
    }
    Ok(Some(target_agent))
}

fn component_allowed_tools(
    value: Option<&Value>,
    context: &str,
    component_label: &str,
) -> Result<BTreeSet<String>, String> {
    let Some(value) = value else {
        return Ok(BTreeSet::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| format!("{component_label} allowed tools must be an array in {context}"))?;
    if values.len() > PLUGIN_COMMAND_MAX_ALLOWED_TOOLS {
        return Err(format!(
            "{component_label} allowed tools exceed {PLUGIN_COMMAND_MAX_ALLOWED_TOOLS} items in {context}"
        ));
    }
    let mut allowed_tools = BTreeSet::new();
    for value in values {
        let raw_tool_name = value
            .as_str()
            .ok_or_else(|| format!("{component_label} allowed tool is invalid in {context}"))?;
        let tool_name = raw_tool_name.trim();
        if tool_name.is_empty() || tool_name != raw_tool_name {
            return Err(format!(
                "{component_label} allowed tool is not normalized in {context}"
            ));
        }
        if tool_name.len() > PLUGIN_COMMAND_MAX_TOOL_NAME_BYTES
            || !tool_name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err(format!(
                "{component_label} allowed tool is not a canonical public tool name in {context}: {tool_name}"
            ));
        }
        if !allowed_tools.insert(tool_name.to_string()) {
            return Err(format!(
                "{component_label} allowed tool is duplicated in {context}: {tool_name}"
            ));
        }
    }
    Ok(allowed_tools)
}

fn plugin_command_prompt_text(
    plugin: &RunPluginSnapshot,
    component: &RunPluginComponentSnapshot,
    command: &Value,
) -> Result<String, String> {
    let prompt = required_response_text(command, "prompt")?;
    let mut lines = vec![format!(
        "[Plugin Command: {} / {}]",
        plugin.plugin_id, component.component_key
    )];
    if let Some(description) = command
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        lines.push(format!("Description: {description}"));
    }
    if let Some(argument_hint) = command
        .get("argument_hint")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        lines.push(format!("Argument hint: {argument_hint}"));
    }
    if let Some(arguments) = component
        .runtime
        .get("arguments")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        lines.push("Arguments for this Run:".to_string());
        lines.push(arguments.to_string());
    }
    lines.push("Follow this signed Plugin Command for the current Run:".to_string());
    lines.push(prompt);
    Ok(lines.join("\n"))
}

fn plugin_agent_prompt_text(
    plugin: &RunPluginSnapshot,
    component: &RunPluginComponentSnapshot,
    agent: &Value,
) -> Result<String, String> {
    let prompt = required_response_text(agent, "prompt")?;
    let base_agent = agent_base_agent(agent.get("base_agent"), "prepare response")?;
    let max_iterations = agent_max_iterations(agent.get("max_iterations"), "prepare response")?;
    let mut lines = vec![format!(
        "[Plugin Agent Profile: {} / {}]",
        plugin.plugin_id, component.component_key
    )];
    if let Some(description) = agent
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        lines.push(format!("Description: {description}"));
    }
    lines.push(format!("Base Agent: {base_agent}"));
    lines.push(format!("Maximum iterations for this Run: {max_iterations}"));
    lines.push(
        "Apply this signed Agent profile as additional instructions for the current Run. It does not grant tools or permissions beyond the existing Task Runner policy."
            .to_string(),
    );
    lines.push(prompt);
    Ok(lines.join("\n"))
}

fn validate_native_skill_response(
    plugin: &RunPluginSnapshot,
    component: &RunPluginComponentSnapshot,
    native_skill: &Value,
) -> Result<(), String> {
    let runtime_kind = component
        .runtime
        .get("runtime_kind")
        .and_then(Value::as_str);
    if runtime_kind != Some("native_adapter") {
        return Err(
            "Local Connector published native tools for a non-native Run component snapshot"
                .to_string(),
        );
    }
    for (field, expected) in [
        ("plugin_id", plugin.plugin_id.as_str()),
        ("release_id", plugin.release_id.as_str()),
        ("plugin_version", plugin.version.as_str()),
        ("artifact_sha256", plugin.artifact_sha256.as_str()),
        ("component_key", component.component_key.as_str()),
    ] {
        if native_skill.get(field).and_then(Value::as_str) != Some(expected) {
            return Err(format!(
                "native Plugin prepare response {field} does not match the immutable Run snapshot"
            ));
        }
    }
    let metadata = component
        .runtime
        .get("metadata")
        .and_then(Value::as_object)
        .ok_or_else(|| "native Plugin Run snapshot is missing component metadata".to_string())?;
    for field in ["skill_id", "bundle_id", "bundle_hash"] {
        let expected = metadata
            .get(field)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("native Plugin Run snapshot metadata is missing {field}"))?;
        if native_skill.get(field).and_then(Value::as_str) != Some(expected) {
            return Err(format!(
                "native Plugin prepare response {field} does not match the immutable Run snapshot"
            ));
        }
    }
    if native_skill.get("bundle_hash").and_then(Value::as_str)
        != Some(component.content_sha256.as_str())
    {
        return Err(
            "native Plugin prepare response bundle_hash does not match component content_sha256"
                .to_string(),
        );
    }
    if ["snapshot_sha256", "tool_snapshot_sha256"]
        .iter()
        .any(|field| {
            native_skill
                .get(field)
                .and_then(Value::as_str)
                .map(str::len)
                != Some(64)
        })
    {
        return Err("native Plugin prepare response is missing audit snapshots".to_string());
    }
    Ok(())
}

fn required_response_text(response: &Value, field: &str) -> Result<String, String> {
    response
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("Plugin prepare response is missing {field}"))
}

fn plugin_server_name(
    plugin: &RunPluginSnapshot,
    component: &RunPluginComponentSnapshot,
) -> String {
    plugin_server_name_from_identity(plugin.plugin_id.as_str(), component.component_key.as_str())
}

fn plugin_server_name_from_identity(plugin_id: &str, component_key: &str) -> String {
    let raw = format!("plugin_{plugin_id}_{component_key}");
    let mut normalized = raw
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    while normalized.contains("__") {
        normalized = normalized.replace("__", "_");
    }
    normalized.trim_matches('_').chars().take(96).collect()
}

pub(crate) fn plugin_relay_base_url() -> Result<String, String> {
    let value = std::env::var("TASK_RUNNER_LOCAL_CONNECTOR_SERVICE_BASE_URL")
        .ok()
        .or_else(|| std::env::var("TASK_RUNNER_LOCAL_CONNECTOR_BASE_URL").ok())
        .or_else(|| std::env::var("LOCAL_CONNECTOR_CLOUD_BASE_URL").ok())
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "TASK_RUNNER_LOCAL_CONNECTOR_SERVICE_BASE_URL or LOCAL_CONNECTOR_CLOUD_BASE_URL is required for Plugin execution"
                .to_string()
        })?;
    validate_plugin_relay_base_url(value)
}

fn validate_plugin_relay_base_url(value: String) -> Result<String, String> {
    let parsed = reqwest::Url::parse(value.as_str())
        .map_err(|error| format!("Plugin relay base URL is invalid: {error}"))?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.path() != "/"
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err("Plugin relay base URL must be an HTTP(S) origin without credentials, query, or fragment".to_string());
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::{
        filter_transient_model_input_for_runtime, is_plugin_hook_dispatch,
        plugin_agent_prompt_text, plugin_command_execution_constraints, plugin_command_prompt_text,
        plugin_server_name, sanitize_runtime_error, validate_agent_response,
        validate_command_response, validate_hook_response, validate_native_skill_response,
        validate_plugin_relay_base_url, validate_prepare_response, validate_ui_response,
        PluginToolLifecycleHook, PluginToolLifecycleStage,
    };
    use chatos_mcp_runtime::{ToolLifecycleEvent, ToolLifecycleOutcome};
    use chatos_plugin_management_sdk::{
        plugin_ui_snapshot_sha256, PluginComponentKind, PluginHookEvent, PluginHookOutcome,
        PluginUiAssetSnapshot, RunPluginComponentSnapshot, RunPluginSnapshot,
        PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1, PLUGIN_UI_HOST_CSP_V1, PLUGIN_UI_IFRAME_SANDBOX_V1,
    };
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::collections::BTreeMap;

    fn plugin_snapshot() -> RunPluginSnapshot {
        RunPluginSnapshot {
            plugin_id: "plugin-browser".to_string(),
            release_id: "release-1".to_string(),
            version: "1.0.0".to_string(),
            artifact_sha256: "abc123".to_string(),
            device_id: "device-1".to_string(),
            workspace_id: Some("workspace-1".to_string()),
            component_snapshots: Vec::new(),
            permission_snapshot: Vec::new(),
            auth_connection_ids: Vec::new(),
        }
    }

    fn component_snapshot() -> RunPluginComponentSnapshot {
        RunPluginComponentSnapshot {
            component_key: "browser.tools/v1".to_string(),
            kind: PluginComponentKind::McpServer,
            content_sha256: "component-hash".to_string(),
            runtime: BTreeMap::new(),
        }
    }

    #[test]
    fn plugin_runtime_event_errors_are_bounded_and_redacted() {
        assert_eq!(
            sanitize_runtime_error(
                "request https://example.test/private failed access_token=secret"
            ),
            "request <redacted-url> failed <redacted-secret>"
        );
        assert!(sanitize_runtime_error("x".repeat(2048).as_str()).len() <= 1024);
    }

    #[test]
    fn plugin_ui_prepare_response_is_recomputed_from_the_immutable_run_snapshot() {
        let plugin = plugin_snapshot();
        let assets = vec![PluginUiAssetSnapshot {
            relative_path: "./ui/app.js".to_string(),
            media_type: "text/javascript".to_string(),
            size_bytes: 128,
            sha256: "a".repeat(64),
        }];
        let mut component = component_snapshot();
        component.component_key = "security-workbench".to_string();
        component.kind = PluginComponentKind::UiContribution;
        component.content_sha256 = "c".repeat(64);
        component.runtime = BTreeMap::from([
            ("entrypoint".to_string(), json!("./ui/index.html")),
            (
                "metadata".to_string(),
                json!({
                    "title": "Security Workbench",
                    "surface": "workbench",
                    "assets": ["./ui/app.js"],
                    "bridge_capabilities": ["artifact.read", "host.context.read"],
                    "artifact_mime_types": ["application/json"]
                }),
            ),
        ]);
        let snapshot_sha256 = plugin_ui_snapshot_sha256(
            plugin.plugin_id.as_str(),
            plugin.release_id.as_str(),
            component.component_key.as_str(),
            "Security Workbench",
            "workbench",
            "./ui/index.html",
            component.content_sha256.as_str(),
            assets.as_slice(),
            PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1,
            &["artifact.read".to_string(), "host.context.read".to_string()],
            &["application/json".to_string()],
            PLUGIN_UI_HOST_CSP_V1,
            PLUGIN_UI_IFRAME_SANDBOX_V1,
        )
        .expect("UI snapshot hash");
        let response = json!({
            "ui": [{
                "plugin_id": plugin.plugin_id.clone(),
                "release_id": plugin.release_id.clone(),
                "version": plugin.version.clone(),
                "artifact_sha256": plugin.artifact_sha256.clone(),
                "component_key": component.component_key.clone(),
                "title": "Security Workbench",
                "surface": "workbench",
                "relative_source_path": "./ui/index.html",
                "content_sha256": component.content_sha256.clone(),
                "assets": assets,
                "bridge_protocol_version": 1,
                "bridge_capabilities": ["artifact.read", "host.context.read"],
                "artifact_mime_types": ["application/json"],
                "content_security_policy": PLUGIN_UI_HOST_CSP_V1,
                "iframe_sandbox": PLUGIN_UI_IFRAME_SANDBOX_V1,
                "snapshot_sha256": snapshot_sha256
            }]
        });
        let validated = validate_ui_response(&plugin, &component, &response)
            .expect("exact signed UI snapshot should pass");
        assert_eq!(validated.snapshot_sha256, snapshot_sha256);

        let mut injected = response;
        injected["ui"][0]["html"] = json!("<script>unsafe()</script>");
        assert!(validate_ui_response(&plugin, &component, &injected)
            .expect_err("unknown UI payload fields must fail")
            .contains("unknown field"));
    }

    #[test]
    fn only_hook_dispatch_uses_the_extended_interactive_relay_window() {
        assert!(is_plugin_hook_dispatch(
            "execute",
            &json!({"operation": "dispatch_hook_event"})
        ));
        assert!(!is_plugin_hook_dispatch(
            "execute",
            &json!({"operation": "mcp_tools_call"})
        ));
        assert!(!is_plugin_hook_dispatch(
            "prepare",
            &json!({"operation": "dispatch_hook_event"})
        ));
    }

    #[test]
    fn command_response_must_match_the_immutable_run_component() {
        let plugin = plugin_snapshot();
        let mut component = component_snapshot();
        component.kind = PluginComponentKind::Command;
        component.component_key = "review".to_string();
        component.content_sha256 = "a".repeat(64);
        component
            .runtime
            .insert("entrypoint".to_string(), json!("./commands/review.md"));
        component
            .runtime
            .insert("arguments".to_string(), json!("src/lib.rs"));
        component.runtime.insert(
            "metadata".to_string(),
            json!({
                "description": "Review the current change",
                "argument_hint": "[path]",
                "requires_confirmation": false,
                "target_agent": "task_runner_run_phase",
                "allowed_tools": ["browser_tools_browser_snapshot"]
            }),
        );
        let arguments_sha256 = hex::encode(Sha256::digest(b"src/lib.rs"));
        let snapshot_sha256 = chatos_plugin_management_sdk::plugin_command_snapshot_sha256(
            plugin.plugin_id.as_str(),
            plugin.release_id.as_str(),
            component.component_key.as_str(),
            "./commands/review.md",
            Some("Review the current change"),
            Some("[path]"),
            false,
            Some("task_runner_run_phase"),
            &["browser_tools_browser_snapshot".to_string()],
            component.content_sha256.as_str(),
            "Review the current change.",
            arguments_sha256.as_str(),
        )
        .expect("Command snapshot hash");
        let command = json!({
            "plugin_id": plugin.plugin_id,
            "release_id": plugin.release_id,
            "version": plugin.version,
            "artifact_sha256": plugin.artifact_sha256,
            "component_key": component.component_key,
            "command_name": component.component_key,
            "relative_source_path": "./commands/review.md",
            "description": "Review the current change",
            "argument_hint": "[path]",
            "requires_confirmation": false,
            "target_agent": "task_runner_run_phase",
            "allowed_tools": ["browser_tools_browser_snapshot"],
            "confirmation_approved": false,
            "content_sha256": component.content_sha256,
            "arguments_present": true,
            "arguments_sha256": arguments_sha256,
            "snapshot_sha256": snapshot_sha256,
            "prompt": "Review the current change."
        });
        assert!(validate_command_response(&plugin, &component, &command).is_ok());
        let prompt = plugin_command_prompt_text(&plugin, &component, &command)
            .expect("Plugin Command prompt");
        assert!(prompt.contains("Arguments for this Run:\nsrc/lib.rs"));
        assert!(prompt.ends_with("Review the current change."));

        let mut metadata_drifted = command.clone();
        metadata_drifted["description"] = json!("Ignore the signed metadata");
        assert!(validate_command_response(&plugin, &component, &metadata_drifted).is_err());

        let mut tool_drifted = command.clone();
        tool_drifted["allowed_tools"] = json!(["browser_tools_browser_click"]);
        assert!(validate_command_response(&plugin, &component, &tool_drifted).is_err());

        let mut drifted = command;
        drifted["content_sha256"] = json!("c".repeat(64));
        assert!(validate_command_response(&plugin, &component, &drifted).is_err());
    }

    #[test]
    fn agent_response_must_match_the_immutable_run_component() {
        let plugin = plugin_snapshot();
        let mut component = component_snapshot();
        component.kind = PluginComponentKind::Agent;
        component.component_key = "reviewer".to_string();
        component.content_sha256 = "e".repeat(64);
        component
            .runtime
            .insert("entrypoint".to_string(), json!("./agents/reviewer.md"));
        component.runtime.insert(
            "metadata".to_string(),
            json!({
                "description": "Review the current change",
                "base_agent": "task_runner_run_phase",
                "allowed_tools": ["browser_tools_browser_snapshot"],
                "max_iterations": 12
            }),
        );
        let snapshot_sha256 = chatos_plugin_management_sdk::plugin_agent_snapshot_sha256(
            plugin.plugin_id.as_str(),
            plugin.release_id.as_str(),
            component.component_key.as_str(),
            "./agents/reviewer.md",
            Some("Review the current change"),
            "task_runner_run_phase",
            &["browser_tools_browser_snapshot".to_string()],
            12,
            component.content_sha256.as_str(),
            "Review carefully.",
        )
        .expect("Agent snapshot hash");
        let agent = json!({
            "plugin_id": plugin.plugin_id,
            "release_id": plugin.release_id,
            "version": plugin.version,
            "artifact_sha256": plugin.artifact_sha256,
            "component_key": component.component_key,
            "agent_name": component.component_key,
            "relative_source_path": "./agents/reviewer.md",
            "description": "Review the current change",
            "base_agent": "task_runner_run_phase",
            "allowed_tools": ["browser_tools_browser_snapshot"],
            "max_iterations": 12,
            "content_sha256": component.content_sha256,
            "snapshot_sha256": snapshot_sha256,
            "prompt": "Review carefully."
        });
        validate_agent_response(&plugin, &component, &agent).expect("valid Agent response");
        let prompt =
            plugin_agent_prompt_text(&plugin, &component, &agent).expect("Plugin Agent prompt");
        assert!(prompt.contains("Base Agent: task_runner_run_phase"));
        assert!(prompt.contains("Maximum iterations for this Run: 12"));
        assert!(prompt.ends_with("Review carefully."));

        let mut drifted = agent;
        drifted["max_iterations"] = json!(13);
        assert!(validate_agent_response(&plugin, &component, &drifted).is_err());
    }

    #[test]
    fn hook_response_must_match_the_immutable_run_component() {
        let plugin = plugin_snapshot();
        let mut component = component_snapshot();
        component.kind = PluginComponentKind::HookSet;
        component.component_key = "lifecycle-hooks".to_string();
        component.content_sha256 = "f".repeat(64);
        component
            .runtime
            .insert("entrypoint".to_string(), json!("./hooks.json"));
        let hook_set = chatos_plugin_management_sdk::parse_plugin_hook_set(
            r#"{"hooks":[{"id":"audit","events":["RunCompleted","RunFailed"],"entrypoint":{"type":"command","command":"./scripts/audit"},"failurePolicy":"continue"}]}"#,
        )
        .expect("Hook set");
        let hook_set_sha256 =
            chatos_plugin_management_sdk::normalized_plugin_hook_set_sha256(&hook_set)
                .expect("Hook set hash");
        let command_hashes = BTreeMap::from([("audit".to_string(), "a".repeat(64))]);
        let snapshot_sha256 = chatos_plugin_management_sdk::plugin_hook_snapshot_sha256(
            plugin.plugin_id.as_str(),
            plugin.release_id.as_str(),
            component.component_key.as_str(),
            "./hooks.json",
            component.content_sha256.as_str(),
            hook_set_sha256.as_str(),
            &command_hashes,
        )
        .expect("Hook snapshot hash");
        let response = json!({
            "hooks": [{
                "plugin_id": plugin.plugin_id,
                "release_id": plugin.release_id,
                "version": plugin.version,
                "artifact_sha256": plugin.artifact_sha256,
                "component_key": component.component_key,
                "relative_source_path": "./hooks.json",
                "content_sha256": component.content_sha256,
                "hook_set_sha256": hook_set_sha256,
                "command_sha256_by_hook": command_hashes,
                "hook_set": hook_set,
                "snapshot_sha256": snapshot_sha256,
            }],
            "operations": ["dispatch_hook_event"]
        });
        assert_eq!(
            validate_hook_response(&plugin, &component, &response).expect("Hook response"),
            snapshot_sha256
        );

        let mut drifted = response;
        drifted["hooks"][0]["command_sha256_by_hook"]["audit"] = json!("b".repeat(64));
        assert!(validate_hook_response(&plugin, &component, &drifted).is_err());
    }

    #[test]
    fn selected_agent_constraints_narrow_tools_and_iterations() {
        let mut agent = component_snapshot();
        agent.kind = PluginComponentKind::Agent;
        agent.component_key = "reviewer".to_string();
        agent.runtime.insert(
            "metadata".to_string(),
            json!({
                "base_agent": "task_runner_run_phase",
                "allowed_tools": ["browser_snapshot"],
                "max_iterations": 12
            }),
        );
        let mut plugin = plugin_snapshot();
        plugin.component_snapshots = vec![agent];
        let run = crate::models::TaskRunRecord::queued(
            "run-1".to_string(),
            "task-1".to_string(),
            "model-1".to_string(),
            "memory-1".to_string(),
            json!({}),
            vec![plugin],
            "2026-07-26T00:00:00Z".to_string(),
        );
        let constraints = plugin_command_execution_constraints(&run).expect("Agent constraints");
        assert_eq!(
            constraints.target_agent.as_deref(),
            Some("task_runner_run_phase")
        );
        assert_eq!(
            constraints.agent_identity.as_deref(),
            Some("plugin-browser:reviewer")
        );
        assert_eq!(constraints.max_iterations, Some(12));
        assert_eq!(constraints.tool_allowlists, vec![vec!["browser_snapshot"]]);
    }

    #[test]
    fn selected_command_constraints_preserve_agent_and_individual_tool_allowlists() {
        let mut first = component_snapshot();
        first.kind = PluginComponentKind::Command;
        first.component_key = "review".to_string();
        first.runtime.insert(
            "metadata".to_string(),
            json!({
                "target_agent": "task_runner_run_phase",
                "allowed_tools": ["browser_snapshot", "browser_click"]
            }),
        );
        let mut second = first.clone();
        second.component_key = "inspect".to_string();
        second.runtime.insert(
            "metadata".to_string(),
            json!({
                "target_agent": "task_runner_run_phase",
                "allowed_tools": ["browser_snapshot"]
            }),
        );
        let mut plugin = plugin_snapshot();
        plugin.component_snapshots = vec![first, second];
        let run = crate::models::TaskRunRecord::queued(
            "run-1".to_string(),
            "task-1".to_string(),
            "model-1".to_string(),
            "memory-1".to_string(),
            json!({}),
            vec![plugin],
            "2026-07-26T00:00:00Z".to_string(),
        );

        let constraints = plugin_command_execution_constraints(&run).expect("Command constraints");
        assert_eq!(
            constraints.target_agent.as_deref(),
            Some("task_runner_run_phase")
        );
        assert_eq!(constraints.tool_allowlists.len(), 2);
        assert_eq!(
            constraints.tool_allowlists[0],
            vec!["browser_click", "browser_snapshot"]
        );
        assert_eq!(constraints.tool_allowlists[1], vec!["browser_snapshot"]);
    }

    #[test]
    fn selected_commands_with_different_target_agents_fail_closed() {
        let mut first = component_snapshot();
        first.kind = PluginComponentKind::Command;
        first.component_key = "review".to_string();
        first.runtime.insert(
            "metadata".to_string(),
            json!({"target_agent": "task_runner_run_phase"}),
        );
        let mut second = first.clone();
        second.component_key = "plan".to_string();
        second.runtime.insert(
            "metadata".to_string(),
            json!({"target_agent": "task_runner_plan_phase"}),
        );
        let mut plugin = plugin_snapshot();
        plugin.component_snapshots = vec![first, second];
        let run = crate::models::TaskRunRecord::queued(
            "run-1".to_string(),
            "task-1".to_string(),
            "model-1".to_string(),
            "memory-1".to_string(),
            json!({}),
            vec![plugin],
            "2026-07-26T00:00:00Z".to_string(),
        );

        assert!(plugin_command_execution_constraints(&run)
            .expect_err("Agent mismatch must fail")
            .contains("different target Agents"));
    }

    #[test]
    fn relay_base_url_accepts_http_origins_only() {
        assert_eq!(
            validate_plugin_relay_base_url("http://127.0.0.1:39232".to_string())
                .expect("valid origin"),
            "http://127.0.0.1:39232"
        );
        assert!(
            validate_plugin_relay_base_url("https://connector.example.com".to_string()).is_ok()
        );
        for value in [
            "file:///tmp/connector",
            "https://user@example.com",
            "https://connector.example.com/api",
            "https://connector.example.com?token=x",
            "https://connector.example.com/#fragment",
        ] {
            assert!(
                validate_plugin_relay_base_url(value.to_string()).is_err(),
                "{value} should be rejected"
            );
        }
    }

    #[test]
    fn prepare_response_must_match_immutable_identity() {
        let plugin = plugin_snapshot();
        let component = component_snapshot();
        let response = json!({
            "plugin_id": plugin.plugin_id,
            "release_id": plugin.release_id,
            "version": plugin.version,
            "artifact_sha256": plugin.artifact_sha256,
            "component_key": component.component_key,
            "adapter_session_id": "session-1",
            "permission_snapshot": plugin.permission_snapshot,
        });
        assert!(validate_prepare_response(&plugin, &component, &response).is_ok());

        let mut drifted = response;
        drifted["release_id"] = json!("release-2");
        let error = validate_prepare_response(&plugin, &component, &drifted)
            .expect_err("release drift should fail closed");
        assert!(error.contains("release_id"));
    }

    #[test]
    fn plugin_server_names_are_normalized_and_bounded() {
        let plugin = plugin_snapshot();
        let component = component_snapshot();
        let name = plugin_server_name(&plugin, &component);
        assert_eq!(name, "plugin_plugin_browser_browser_tools_v1");
        assert!(name.len() <= 96);
        assert!(name.chars().all(|character| character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || character == '_'));
    }

    #[test]
    fn tool_lifecycle_events_map_to_bounded_plugin_hook_contexts() {
        let hook = PluginToolLifecycleHook {
            sessions: Vec::new(),
            agent_key: "task_runner_run_phase".to_string(),
            component_by_server: BTreeMap::from([(
                "plugin_plugin_browser_browser_tools".to_string(),
                "browser-tools".to_string(),
            )]),
        };
        let event = ToolLifecycleEvent {
            tool_name: "plugin_plugin_browser_browser_tools_snapshot".to_string(),
            original_name: "snapshot".to_string(),
            server_name: "plugin_plugin_browser_browser_tools".to_string(),
            server_type: "builtin".to_string(),
            arguments_sha256: "a".repeat(64),
            outcome: Some(ToolLifecycleOutcome::Succeeded),
            result_sha256: Some("b".repeat(64)),
        };

        let (pre_event, pre_context) = hook.map_event(&event, PluginToolLifecycleStage::Pre);
        assert_eq!(pre_event, PluginHookEvent::PreToolUse);
        assert_eq!(
            pre_context.agent_key.as_deref(),
            Some("task_runner_run_phase")
        );
        assert_eq!(
            pre_context.tool_name.as_deref(),
            Some("plugin_plugin_browser_browser_tools_snapshot")
        );
        assert_eq!(pre_context.tool_kind.as_deref(), Some("builtin"));
        assert_eq!(pre_context.component_key.as_deref(), Some("browser-tools"));
        assert_eq!(pre_context.outcome, None);
        assert_eq!(pre_context.summary_sha256, Some("a".repeat(64)));

        let (post_event, post_context) = hook.map_event(&event, PluginToolLifecycleStage::Post);
        assert_eq!(post_event, PluginHookEvent::PostToolUse);
        assert_eq!(post_context.outcome, Some(PluginHookOutcome::Succeeded));
        assert_eq!(post_context.summary_sha256, Some("b".repeat(64)));
    }

    #[test]
    fn native_skill_response_must_match_run_component_snapshot() {
        let mut component = component_snapshot();
        component.kind = PluginComponentKind::SkillCollection;
        component.component_key = "computer-use".to_string();
        component.content_sha256 = "a".repeat(64);
        component
            .runtime
            .insert("runtime_kind".to_string(), json!("native_adapter"));
        component.runtime.insert(
            "metadata".to_string(),
            json!({
                "skill_id": "internal_skill_computer_use",
                "bundle_id": "chatos.internal.computer-use",
                "bundle_hash": "a".repeat(64),
            }),
        );
        let plugin = plugin_snapshot();
        let native = json!({
            "plugin_id": plugin.plugin_id,
            "release_id": plugin.release_id,
            "plugin_version": plugin.version,
            "artifact_sha256": plugin.artifact_sha256,
            "component_key": component.component_key,
            "skill_id": "internal_skill_computer_use",
            "bundle_id": "chatos.internal.computer-use",
            "bundle_hash": "a".repeat(64),
            "snapshot_sha256": "b".repeat(64),
            "tool_snapshot_sha256": "c".repeat(64),
            "permissions": ["system.accessibility"],
            "tools": [{"name": "computer_list_windows", "inputSchema": {"type": "object"}}],
        });
        assert!(validate_native_skill_response(&plugin, &component, &native).is_ok());

        let mut drifted = native;
        drifted["bundle_hash"] = json!("d".repeat(64));
        assert!(
            validate_native_skill_response(&plugin, &component, &drifted)
                .expect_err("bundle hash drift must fail closed")
                .contains("bundle_hash")
        );
    }

    #[test]
    fn chrome_native_skill_tools_flow_through_the_generic_plugin_relay() {
        let mut component = component_snapshot();
        component.kind = PluginComponentKind::SkillCollection;
        component.component_key = "control-chrome".to_string();
        component.content_sha256 = "a".repeat(64);
        component
            .runtime
            .insert("runtime_kind".to_string(), json!("native_adapter"));
        component.runtime.insert(
            "metadata".to_string(),
            json!({
                "skill_id": "internal_skill_chrome",
                "bundle_id": "chatos.internal.chrome",
                "bundle_hash": "a".repeat(64),
            }),
        );
        let plugin = plugin_snapshot();
        let native = json!({
            "plugin_id": plugin.plugin_id,
            "release_id": plugin.release_id,
            "plugin_version": plugin.version,
            "artifact_sha256": plugin.artifact_sha256,
            "component_key": component.component_key,
            "skill_id": "internal_skill_chrome",
            "bundle_id": "chatos.internal.chrome",
            "bundle_hash": "a".repeat(64),
            "snapshot_sha256": "b".repeat(64),
            "tool_snapshot_sha256": "c".repeat(64),
            "permissions": ["browser.chrome.control", "workspace.read", "workspace.write"],
            "tools": [
                {"name": "chrome_status", "inputSchema": {"type": "object"}},
                {"name": "chrome_tabs", "inputSchema": {"type": "object"}},
                {"name": "chrome_tab_snapshot", "inputSchema": {"type": "object"}},
                {"name": "chrome_tab_navigate", "inputSchema": {"type": "object"}},
                {"name": "chrome_tab_click", "inputSchema": {"type": "object"}},
                {"name": "chrome_tab_type_text", "inputSchema": {"type": "object"}},
                {"name": "chrome_tab_select", "inputSchema": {"type": "object"}},
                {"name": "chrome_tab_scroll", "inputSchema": {"type": "object"}},
                {"name": "chrome_tab_history", "inputSchema": {"type": "object"}},
                {"name": "chrome_tab_activate", "inputSchema": {"type": "object"}},
                {"name": "chrome_tab_upload", "inputSchema": {"type": "object"}},
                {"name": "chrome_tab_download", "inputSchema": {"type": "object"}},
                {"name": "chrome_tab_screenshot", "inputSchema": {"type": "object"}},
                {"name": "chrome_tab_release", "inputSchema": {"type": "object"}}
            ],
        });
        assert!(validate_native_skill_response(&plugin, &component, &native).is_ok());
    }

    #[test]
    fn transient_plugin_images_require_declared_model_image_support() {
        let result = json!({
            "text": "captured",
            "_model_input": [{"image_url": "data:image/jpeg;base64,/9j/AA=="}]
        });
        let supported = filter_transient_model_input_for_runtime(result.clone(), Some(true));
        assert!(supported.get("_model_input").is_some());

        let unsupported = filter_transient_model_input_for_runtime(result, Some(false));
        assert!(unsupported.get("_model_input").is_none());
        assert_eq!(
            unsupported
                .pointer("/model_image_delivery/delivered")
                .and_then(serde_json::Value::as_bool),
            Some(false)
        );
        assert!(unsupported
            .get("text")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|text| text.contains("does not declare image input support")));
    }
}
