// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::WorkspaceProviderKind;
use serde_json::Value;

use super::value_helpers::is_lower_sha256;
use crate::providers::plugin_components::PluginPrepareResponse;
use crate::providers::ProviderCallError;
use crate::runtime::{
    PluginLocalToolComponentBinding, PluginToolComponentRuntimeBinding, RuntimeSessionSnapshot,
};
use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};

pub(in crate::providers::plugin_components) fn validate_immutable_route(
    immutable: &PluginToolComponentRuntimeBinding,
    route: &ResolvedMcpRoute,
    provider_kind: McpProviderKind,
) -> Result<(), ProviderCallError> {
    if route.provider_kind != provider_kind
        || route.provider_ref.as_deref() != Some(immutable.provider_ref.as_str())
        || route.resource_id != immutable.resource_id
        || route.allow_writes != immutable.allow_writes
    {
        return Err(ProviderCallError::provider_unavailable(
            "Plugin tool component route does not match its immutable binding",
        ));
    }
    Ok(())
}

pub(in crate::providers::plugin_components) fn validate_prepare_identity(
    immutable: &PluginToolComponentRuntimeBinding,
    runtime_session_id: &str,
    runtime_expires_at_unix: i64,
    prepared: &PluginPrepareResponse,
) -> Result<(), ProviderCallError> {
    if prepared.run_id != runtime_session_id
        || prepared.plugin_id != immutable.plugin_id
        || prepared.release_id != immutable.release_id
        || prepared.version != immutable.version
        || prepared.artifact_sha256 != immutable.artifact_sha256
        || prepared.component_key != immutable.component.component_key
    {
        return Err(ProviderCallError::invalid_response(
            "Plugin component prepare response does not match the immutable runtime binding",
        ));
    }
    if prepared.adapter_session_id.trim().is_empty()
        || !is_lower_sha256(prepared.session_sha256.as_str())
        || prepared.expires_at <= chrono::Utc::now().timestamp()
        || prepared.expires_at < runtime_expires_at_unix
    {
        return Err(ProviderCallError::invalid_response(
            "Plugin component prepare returned an invalid or prematurely expiring session",
        ));
    }
    Ok(())
}

pub(in crate::providers::plugin_components) fn required_operation(
    operations: &[String],
    expected: &str,
) -> Result<String, ProviderCallError> {
    if operations.iter().any(|operation| operation == expected) {
        Ok(expected.to_string())
    } else {
        Err(ProviderCallError::invalid_response(format!(
            "Plugin component prepare did not publish {expected}"
        )))
    }
}

pub(in crate::providers::plugin_components) fn validate_execute_identity(
    binding: &PluginLocalToolComponentBinding,
    response: &Value,
) -> Result<(), ProviderCallError> {
    for (field, expected) in [
        ("plugin_id", binding.runtime.plugin_id.as_str()),
        ("release_id", binding.runtime.release_id.as_str()),
        ("version", binding.runtime.version.as_str()),
        ("artifact_sha256", binding.runtime.artifact_sha256.as_str()),
        (
            "component_key",
            binding.runtime.component.component_key.as_str(),
        ),
        ("adapter_session_id", binding.adapter_session_id.as_str()),
        ("operation", binding.operation.as_str()),
    ] {
        if response.get(field).and_then(Value::as_str) != Some(expected) {
            return Err(ProviderCallError::invalid_response(format!(
                "Plugin Local component execute response {field} does not match its prepared binding"
            )));
        }
    }
    Ok(())
}

pub(in crate::providers::plugin_components) fn validate_local_bound_route(
    snapshot: &RuntimeSessionSnapshot,
    route: &ResolvedMcpRoute,
    binding: &PluginLocalToolComponentBinding,
) -> Result<(), ProviderCallError> {
    let immutable = snapshot
        .plugin_tool_component_bindings
        .get(route.resource_id.as_str())
        .ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "immutable Plugin tool component binding is missing",
            )
        })?;
    let workspace = snapshot.project_context.workspace.as_ref();
    if snapshot.project_context.workspace_provider != WorkspaceProviderKind::LocalConnector
        || snapshot.expires_at_unix.min(binding.expires_at_unix) <= chrono::Utc::now().timestamp()
        || immutable != &binding.runtime
        || route.provider_ref.as_deref() != Some(binding.runtime.provider_ref.as_str())
        || route.allow_writes != binding.runtime.allow_writes
        || workspace.and_then(|workspace| workspace.device_id.as_deref())
            != Some(binding.device_id.as_str())
        || workspace.map(|workspace| workspace.workspace_id.as_str())
            != Some(binding.workspace_id.as_str())
    {
        return Err(ProviderCallError::provider_unavailable(
            "Plugin Local tool component route does not match its prepared session",
        ));
    }
    Ok(())
}
