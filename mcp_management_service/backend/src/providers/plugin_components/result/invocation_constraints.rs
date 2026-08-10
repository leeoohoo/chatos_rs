// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_service::MCP_ERROR_AUTH_REQUIRED;
use serde_json::Value;

use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};

use crate::providers::plugin_components::MAX_COMMAND_ARGUMENT_BYTES;
use crate::providers::ProviderCallError;

pub(in crate::providers::plugin_components) fn parse_command_arguments(
    arguments: Value,
) -> Result<Option<String>, ProviderCallError> {
    let object = arguments.as_object().ok_or_else(|| {
        ProviderCallError::invalid_response("Plugin Command arguments must be an object")
    })?;
    if object.keys().any(|key| key != "arguments") {
        return Err(ProviderCallError::invalid_response(
            "Plugin Command arguments contain an unknown field",
        ));
    }
    let Some(value) = object.get("arguments") else {
        return Ok(None);
    };
    let value = value.as_str().ok_or_else(|| {
        ProviderCallError::invalid_response("Plugin Command arguments must be a string")
    })?;
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() > MAX_COMMAND_ARGUMENT_BYTES || value.contains('\0') {
        return Err(ProviderCallError::invalid_response(
            "Plugin Command arguments are invalid or exceed their size limit",
        ));
    }
    Ok(Some(value.to_string()))
}

pub(in crate::providers::plugin_components) fn validate_empty_arguments(
    arguments: &Value,
    label: &str,
) -> Result<(), ProviderCallError> {
    if arguments
        .as_object()
        .is_some_and(|object| object.is_empty())
    {
        Ok(())
    } else {
        Err(ProviderCallError::invalid_response(format!(
            "{label} does not accept arguments"
        )))
    }
}

pub(in crate::providers::plugin_components) fn ensure_expected_tool(
    actual: &str,
    expected: &str,
) -> Result<(), ProviderCallError> {
    if actual == expected {
        Ok(())
    } else {
        Err(ProviderCallError {
            code: MCP_ERROR_AUTH_REQUIRED,
            message: format!("Plugin component publishes only the {expected} tool"),
        })
    }
}

pub(in crate::providers::plugin_components) fn make_route_unavailable(
    route: &mut ResolvedMcpRoute,
    reason: &str,
) {
    route.provider_kind = McpProviderKind::Unavailable;
    route.provider_ref = None;
    route.allow_writes = false;
    route.cancel_supported = false;
    route.reason = format!("Plugin Component Provider unavailable: {reason}");
}
