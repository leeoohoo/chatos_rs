// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;
use std::path::{Component, Path};

use async_trait::async_trait;
use chatos_mcp_runtime::{BuiltinToolProvider, ToolCallContext, ToolStreamChunkCallback};
use chatos_plugin_management_sdk::{
    PluginArtifactDescriptor, PluginArtifactReadyEventPayload, PluginComponentKind,
    PluginUiReadyEventPayload, PluginUiSnapshot, PLUGIN_ARTIFACT_MAX_BYTES,
    PLUGIN_ARTIFACT_READY_EVENT_VERSION_V1, PLUGIN_UI_READY_EVENT_VERSION_V1,
};
use serde_json::{json, Map, Value};

use super::{is_lower_sha256, PluginRelayClient};
use crate::models::TaskRunEventRecord;

#[derive(Clone)]
pub(in crate::services) struct PreparedPluginSession {
    pub(super) relay: PluginRelayClient,
    pub(super) plugin_id: String,
    pub(super) release_id: String,
    pub(super) artifact_sha256: String,
    pub(super) component_key: String,
    pub(super) adapter_session_id: String,
    pub(super) component_kind: PluginComponentKind,
    pub(super) operations: BTreeSet<String>,
    pub(super) hook_snapshot_sha256: Option<String>,
    pub(super) ui_snapshot: Option<PluginUiSnapshot>,
}

impl PreparedPluginSession {
    pub(super) fn identity_body(&self) -> Map<String, Value> {
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

    pub(super) async fn execute_tool(
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

    pub(super) async fn cancel(&self) -> Result<(), String> {
        self.relay
            .request("cancel", Value::Object(self.identity_body()))
            .await
            .map(|_| ())
    }

    pub(super) fn record_ui_ready(&self) {
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

    pub(super) fn record_artifacts_ready(
        &self,
        tool_name: &str,
        value: Option<Value>,
    ) -> Result<(), String> {
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

pub(super) struct PluginRelayToolProvider {
    pub(super) server_name: String,
    pub(super) session: PreparedPluginSession,
    pub(super) operation: String,
    pub(super) tools: Vec<Value>,
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

pub(in crate::services) fn filter_transient_model_input_for_runtime(
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
