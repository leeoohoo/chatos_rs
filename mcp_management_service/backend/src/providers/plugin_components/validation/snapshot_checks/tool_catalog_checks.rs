// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::providers::plugin_components::{
    AGENT_TOOL_NAME, COMMAND_TOOL_NAME, MAX_COMMAND_ARGUMENT_BYTES,
};
use crate::runtime::PluginToolComponentRuntimeBinding;

use super::super::value_helpers::component_metadata_text;

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
