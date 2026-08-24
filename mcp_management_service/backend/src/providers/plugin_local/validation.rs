// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use chatos_mcp_management_sdk::{ResolvedMcpRoute, WorkspaceProviderKind};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{PluginLocalProviderBinding, PluginPrepareResponse};
use crate::providers::ProviderCallError;
use crate::runtime::{PluginMcpRuntimeBinding, RuntimeSessionSnapshot};

pub(super) fn validate_prepare_response(
    immutable: &PluginMcpRuntimeBinding,
    runtime_session_id: &str,
    runtime_expires_at_unix: i64,
    prepared: &PluginPrepareResponse,
) -> Result<(), ProviderCallError> {
    if prepared.run_id != runtime_session_id
        || prepared.plugin_id != immutable.plugin_id
        || prepared.release_id != immutable.release_id
        || prepared.version != immutable.version
        || prepared.artifact_sha256 != immutable.artifact_sha256
        || prepared.component_key != immutable.component_key
        || prepared.mcp.plugin_id != immutable.plugin_id
        || prepared.mcp.release_id != immutable.release_id
        || prepared.mcp.version != immutable.version
        || prepared.mcp.artifact_sha256 != immutable.artifact_sha256
        || prepared.mcp.component_key != immutable.component_key
    {
        return Err(ProviderCallError::invalid_response(
            "Plugin Local prepare response does not match the immutable runtime binding",
        ));
    }
    if prepared.adapter_session_id.trim().is_empty()
        || !is_lower_sha256(prepared.session_sha256.as_str())
        || !is_lower_sha256(prepared.mcp.snapshot_sha256.as_str())
        || !is_lower_sha256(prepared.mcp.tool_snapshot_sha256.as_str())
        || !is_lower_sha256(prepared.mcp.server_instructions_sha256.as_str())
        || prepared.expires_at <= chrono::Utc::now().timestamp()
        || prepared.expires_at < runtime_expires_at_unix
    {
        return Err(ProviderCallError::invalid_response(
            "Plugin Local prepare returned an invalid or prematurely expiring session snapshot",
        ));
    }
    validate_tool_snapshot(
        prepared.mcp.tools.as_slice(),
        prepared.mcp.tool_snapshot_sha256.as_str(),
    )?;
    validate_server_instructions(
        prepared.mcp.server_instructions.as_deref(),
        prepared.mcp.server_instructions_sha256.as_str(),
    )?;
    if prepared.mcp.oauth_connection_id.as_ref().is_some_and(|id| {
        !immutable
            .auth_connection_ids
            .iter()
            .any(|allowed| allowed == id)
    }) {
        return Err(ProviderCallError::invalid_response(
            "Plugin Local prepare selected an OAuth connection outside the immutable snapshot",
        ));
    }
    Ok(())
}

fn validate_server_instructions(
    instructions: Option<&str>,
    expected_sha256: &str,
) -> Result<(), ProviderCallError> {
    if instructions.is_some_and(|value| {
        value.is_empty()
            || value.trim() != value
            || value.len() > super::MAX_PLUGIN_SERVER_INSTRUCTIONS_BYTES
            || value.contains('\0')
            || value.contains('\r')
    }) {
        return Err(ProviderCallError::invalid_response(
            "Plugin MCP server instructions are not normalized or exceed the byte limit",
        ));
    }
    let encoded = serde_json::to_vec(&instructions).map_err(|error| {
        ProviderCallError::invalid_response(format!(
            "serialize Plugin MCP server instructions failed: {error}"
        ))
    })?;
    if hex::encode(Sha256::digest(encoded)) != expected_sha256 {
        return Err(ProviderCallError::invalid_response(
            "Plugin MCP server instructions hash is invalid",
        ));
    }
    Ok(())
}

pub(super) fn validate_bound_route(
    snapshot: &RuntimeSessionSnapshot,
    route: &ResolvedMcpRoute,
    binding: &PluginLocalProviderBinding,
) -> Result<(), ProviderCallError> {
    let immutable = snapshot
        .plugin_mcp_bindings
        .get(route.resource_id.as_str())
        .ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "immutable Plugin MCP runtime binding is missing",
            )
        })?;
    let workspace = snapshot.project_context.workspace.as_ref();
    if !matches!(
        snapshot.project_context.workspace_provider,
        WorkspaceProviderKind::LocalConnector
    ) || !snapshot
        .expires_at_unix
        .min(binding.expires_at_unix)
        .gt(&chrono::Utc::now().timestamp())
        || immutable != &binding.runtime
        || route.provider_ref.as_deref() != Some(binding.runtime.provider_ref.as_str())
        || route.allow_writes != binding.runtime.allow_writes
        || workspace.and_then(|workspace| workspace.device_id.as_deref())
            != Some(binding.device_id.as_str())
        || workspace.map(|workspace| workspace.workspace_id.as_str())
            != Some(binding.workspace_id.as_str())
    {
        return Err(ProviderCallError::provider_unavailable(
            "Plugin Local route does not match its prepared runtime session",
        ));
    }
    Ok(())
}

fn validate_tool_snapshot(tools: &[Value], expected_sha256: &str) -> Result<(), ProviderCallError> {
    if tools.is_empty() || tools.len() > super::MAX_PLUGIN_TOOLS {
        return Err(ProviderCallError::invalid_response(
            "Plugin MCP tool snapshot must contain between 1 and 200 tools",
        ));
    }
    let encoded = serde_json::to_vec(tools).map_err(|error| {
        ProviderCallError::invalid_response(format!(
            "serialize Plugin MCP tool snapshot failed: {error}"
        ))
    })?;
    if encoded.len() > super::MAX_PLUGIN_TOOL_SNAPSHOT_BYTES
        || hex::encode(Sha256::digest(encoded)) != expected_sha256
    {
        return Err(ProviderCallError::invalid_response(
            "Plugin MCP tool snapshot hash or size is invalid",
        ));
    }
    let mut names = HashSet::new();
    for tool in tools {
        let name = tool
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .ok_or_else(|| {
                ProviderCallError::invalid_response(
                    "Plugin MCP tool snapshot contains an unnamed tool",
                )
            })?;
        if !names.insert(name) {
            return Err(ProviderCallError::invalid_response(
                "Plugin MCP tool snapshot contains duplicate tool names",
            ));
        }
    }
    Ok(())
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}
