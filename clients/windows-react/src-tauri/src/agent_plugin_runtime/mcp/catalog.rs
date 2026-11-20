// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::{
    opaque_option, plugin_runtime_error, project_skill_gate, skill_option, valid_identifier,
    AgentPluginRunContext,
};
use chatos_plugin_management_sdk::SkillGateDeclaration;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

use crate::plugin_application::ResolvedAgentPluginExecution;

const MAXIMUM_TOOL_SCHEMA_BYTES: usize = 256 * 1_024;

#[derive(Clone, Debug)]
pub(in crate::agent_plugin_runtime) struct ToolDefinition {
    pub(in crate::agent_plugin_runtime) option: String,
    pub(in crate::agent_plugin_runtime) name: String,
    pub(in crate::agent_plugin_runtime) description: String,
    pub(in crate::agent_plugin_runtime) input_schema: Value,
    pub(in crate::agent_plugin_runtime) schema_sha256: String,
    pub(in crate::agent_plugin_runtime) skill_gate: Option<SkillGateDeclaration>,
    pub(in crate::agent_plugin_runtime) skill_gate_projection: Value,
    pub(in crate::agent_plugin_runtime) required_skill_refs: Vec<String>,
}
pub(super) fn decode_tool_definitions(
    context: &AgentPluginRunContext,
    resolved: &ResolvedAgentPluginExecution,
    tools: Vec<Value>,
) -> Result<Vec<ToolDefinition>, String> {
    let allowed = resolved
        .allowed_tools
        .as_ref()
        .map(|values| values.iter().map(String::as_str).collect::<HashSet<_>>());
    let mut names = HashSet::new();
    let mut definitions = Vec::new();
    for tool in tools {
        let name = tool
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| valid_identifier(name))
            .ok_or_else(plugin_runtime_error)?;
        if allowed
            .as_ref()
            .is_some_and(|allowed| !allowed.contains(name))
        {
            continue;
        }
        if !names.insert(name.to_owned()) {
            return Err(plugin_runtime_error());
        }
        let description = tool
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if description.chars().count() > 16_000 || description.contains('\0') {
            return Err(plugin_runtime_error());
        }
        let input_schema = tool
            .get("inputSchema")
            .cloned()
            .unwrap_or_else(|| json!({"type": "object", "properties": {}}));
        if !input_schema.is_object()
            || serde_json::to_vec(&input_schema)
                .map_err(|_| plugin_runtime_error())?
                .len()
                > MAXIMUM_TOOL_SCHEMA_BYTES
        {
            return Err(plugin_runtime_error());
        }
        let schema_digest = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&input_schema).map_err(|_| plugin_runtime_error())?)
        );
        let skill_gate = match tool
            .get("_meta")
            .and_then(Value::as_object)
            .and_then(|metadata| metadata.get("chatos/skillGate"))
        {
            Some(value) => {
                let gate = serde_json::from_value::<SkillGateDeclaration>(value.clone())
                    .map_err(|_| plugin_runtime_error())?;
                gate.validate().map_err(|_| plugin_runtime_error())?;
                Some(gate)
            }
            None if resolved.skill_snapshot.documents.len() == 1 => {
                let skill_name = resolved
                    .skill_snapshot
                    .documents
                    .keys()
                    .next()
                    .ok_or_else(plugin_runtime_error)?;
                Some(
                    serde_json::from_value(json!({"allOf": [skill_name]}))
                        .map_err(|_| plugin_runtime_error())?,
                )
            }
            None => None,
        };
        let required_skill_names = skill_gate
            .as_ref()
            .map(SkillGateDeclaration::catalog_skill_names)
            .transpose()
            .map_err(|_| plugin_runtime_error())?
            .unwrap_or_default();
        if required_skill_names
            .iter()
            .any(|name| !resolved.skill_snapshot.documents.contains_key(name))
        {
            return Err(plugin_runtime_error());
        }
        let required_skill_refs = required_skill_names
            .iter()
            .map(|name| skill_option(context, &resolved.identity, name))
            .collect::<Vec<_>>();
        let skill_gate_projection = skill_gate
            .as_ref()
            .map(|gate| project_skill_gate(context, &resolved.identity, gate))
            .unwrap_or_else(|| json!({"all_of": []}));
        definitions.push(ToolDefinition {
            option: opaque_option(
                "tool",
                &context.owner_user_id,
                &context.run_id,
                &format!(
                    "{}\0{}\0{}",
                    resolved.identity.frozen_identity(),
                    name,
                    schema_digest
                ),
            ),
            name: name.to_owned(),
            description: description.to_owned(),
            input_schema,
            schema_sha256: schema_digest,
            skill_gate,
            skill_gate_projection,
            required_skill_refs,
        });
    }
    if let Some(allowed) = allowed {
        if allowed.iter().any(|name| !names.contains(*name)) {
            return Err(plugin_runtime_error());
        }
    }
    definitions.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(definitions)
}
