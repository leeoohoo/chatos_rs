// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use serde_json::{json, Value};

use crate::providers::plugin_components::{
    AGENT_TOOL_NAME, COMMAND_TOOL_NAME, MAX_COMMAND_ARGUMENT_BYTES, MAX_PLUGIN_TOOLS,
    MAX_PLUGIN_TOOL_SNAPSHOT_BYTES,
};
use crate::providers::ProviderCallError;
use crate::runtime::PluginToolComponentRuntimeBinding;

use super::super::value_helpers::component_metadata_text;

pub(in crate::providers::plugin_components) fn validate_tool_snapshot(
    tools: &[Value],
) -> Result<(), ProviderCallError> {
    if tools.is_empty() || tools.len() > MAX_PLUGIN_TOOLS {
        return Err(ProviderCallError::invalid_response(
            "Plugin component tool snapshot must contain between 1 and 200 tools",
        ));
    }
    let encoded = serde_json::to_vec(tools).map_err(|error| {
        ProviderCallError::invalid_response(format!(
            "serialize Plugin component tool snapshot failed: {error}"
        ))
    })?;
    if encoded.len() > MAX_PLUGIN_TOOL_SNAPSHOT_BYTES {
        return Err(ProviderCallError::invalid_response(
            "Plugin component tool snapshot exceeds its size limit",
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
                    "Plugin component tool snapshot contains an unnamed tool",
                )
            })?;
        if !names.insert(name) {
            return Err(ProviderCallError::invalid_response(
                "Plugin component tool snapshot contains duplicate tool names",
            ));
        }
    }
    Ok(())
}

pub(in crate::providers::plugin_components) fn command_tool_definition(
    binding: &PluginToolComponentRuntimeBinding,
) -> Value {
    let description = component_metadata_text(binding, "description")
        .unwrap_or("Invoke the signed Plugin Command and return its immutable instructions");
    let argument_hint = component_metadata_text(binding, "argument_hint");
    json!({
        "name": COMMAND_TOOL_NAME,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": {
                "arguments": {
                    "type": "string",
                    "maxLength": MAX_COMMAND_ARGUMENT_BYTES,
                    "description": argument_hint.unwrap_or("Optional arguments for this Plugin Command")
                }
            },
            "additionalProperties": false
        }
    })
}

pub(in crate::providers::plugin_components) fn skill_tool_definition(
    binding: &PluginToolComponentRuntimeBinding,
) -> Value {
    let description = component_metadata_text(binding, "description")
        .unwrap_or("Apply the signed Plugin Skill to the current task");
    json!({
        "name": "apply",
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }
    })
}

pub(in crate::providers::plugin_components) fn agent_tool_definition(
    binding: &PluginToolComponentRuntimeBinding,
) -> Value {
    let description = component_metadata_text(binding, "description")
        .unwrap_or("Apply the signed Plugin Agent profile to the current task");
    json!({
        "name": AGENT_TOOL_NAME,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }
    })
}
