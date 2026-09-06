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

pub(in crate::providers::plugin_components) fn skill_runtime_tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": super::super::super::SKILL_ACTIVATE_TOOL_NAME,
            "description": "Activate one immutable Plugin Skill from the current Runtime Session catalog. Use the skill_ref shown in the catalog and retain the returned activation evidence for gated Plugin tools.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "skill_ref": {
                        "type": "string",
                        "description": "Stable SK... reference from the current Plugin Skill catalog"
                    },
                    "arguments": {
                        "type": "object",
                        "description": "Optional bounded inputs used to render or specialize the Skill",
                        "additionalProperties": true
                    },
                    "parent_activation_ref": {
                        "type": "string",
                        "description": "Optional parent activation returned by an already activated router Skill"
                    }
                },
                "required": ["skill_ref"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": super::super::super::SKILL_LIST_RESOURCES_TOOL_NAME,
            "description": "List immutable resources published by an activated Plugin Skill.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "activation_ref": {"type": "string"},
                    "activation_evidence": {"type": "string"}
                },
                "required": ["activation_ref", "activation_evidence"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": super::super::super::SKILL_READ_RESOURCE_TOOL_NAME,
            "description": "Read one immutable text resource from an activated Plugin Skill.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "activation_ref": {"type": "string"},
                    "activation_evidence": {"type": "string"},
                    "relative_path": {"type": "string"},
                    "offset": {"type": "integer", "minimum": 0},
                    "max_chars": {"type": "integer", "minimum": 1, "maximum": 64000}
                },
                "required": ["activation_ref", "activation_evidence", "relative_path"],
                "additionalProperties": false
            }
        }),
    ]
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
