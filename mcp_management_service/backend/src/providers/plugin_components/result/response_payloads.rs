// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::PluginComponentKind;
use serde_json::{json, Value};

use crate::providers::plugin_components::validation::{
    component_metadata_text, required_value_text,
};
use crate::providers::plugin_components::THIRD_PARTY_PLUGIN_ENVELOPE;
use crate::providers::ProviderCallError;
use crate::runtime::PluginToolComponentRuntimeBinding;

pub(in crate::providers::plugin_components) fn plugin_command_result(
    immutable: &PluginToolComponentRuntimeBinding,
    command: &Value,
    arguments: Option<&str>,
) -> Result<Value, ProviderCallError> {
    let prompt = required_value_text(command, "prompt")?;
    Ok(plugin_instruction_result(
        immutable,
        "Plugin Command",
        prompt,
        arguments,
        Some(command),
    ))
}

pub(in crate::providers::plugin_components) fn plugin_command_result_from_bundle(
    immutable: &PluginToolComponentRuntimeBinding,
    bundle: &chatos_plugin_management_sdk::PluginCloudComponentBundle,
    arguments: Option<&str>,
) -> Result<Value, ProviderCallError> {
    Ok(plugin_instruction_result(
        immutable,
        "Plugin Command",
        bundle.primary_text.as_str(),
        arguments,
        None,
    ))
}

pub(in crate::providers::plugin_components) fn plugin_skill_result_from_bundle(
    immutable: &PluginToolComponentRuntimeBinding,
    bundle: &chatos_plugin_management_sdk::PluginCloudComponentBundle,
) -> Result<Value, ProviderCallError> {
    Ok(plugin_instruction_result(
        immutable,
        "Plugin Skill",
        bundle.primary_text.as_str(),
        None,
        None,
    ))
}

pub(in crate::providers::plugin_components) fn plugin_skill_result_from_local_snapshot(
    immutable: &PluginToolComponentRuntimeBinding,
    skill: &Value,
) -> Result<Value, ProviderCallError> {
    let instructions = required_value_text(skill, "instructions")?;
    Ok(plugin_instruction_result(
        immutable,
        "Plugin Skill",
        instructions,
        None,
        Some(skill),
    ))
}

pub(in crate::providers::plugin_components) fn plugin_agent_result(
    immutable: &PluginToolComponentRuntimeBinding,
    agent: &Value,
) -> Result<Value, ProviderCallError> {
    let prompt = required_value_text(agent, "prompt")?;
    Ok(plugin_instruction_result(
        immutable,
        "Plugin Agent Profile",
        prompt,
        None,
        Some(agent),
    ))
}

pub(in crate::providers::plugin_components) fn plugin_agent_result_from_bundle(
    immutable: &PluginToolComponentRuntimeBinding,
    bundle: &chatos_plugin_management_sdk::PluginCloudComponentBundle,
) -> Result<Value, ProviderCallError> {
    Ok(plugin_instruction_result(
        immutable,
        "Plugin Agent Profile",
        bundle.primary_text.as_str(),
        None,
        None,
    ))
}

fn plugin_instruction_result(
    immutable: &PluginToolComponentRuntimeBinding,
    label: &str,
    prompt: &str,
    arguments: Option<&str>,
    runtime_snapshot: Option<&Value>,
) -> Value {
    let mut lines = vec![
        THIRD_PARTY_PLUGIN_ENVELOPE.to_string(),
        String::new(),
        format!(
            "[{label}: {} / {}]",
            immutable.plugin_id, immutable.component.component_key
        ),
    ];
    if let Some(description) = component_metadata_text(immutable, "description") {
        lines.push(format!("Description: {description}"));
    }
    if let Some(argument_hint) = component_metadata_text(immutable, "argument_hint") {
        lines.push(format!("Argument hint: {argument_hint}"));
    }
    if let Some(arguments) = arguments {
        lines.push("Arguments for this invocation:".to_string());
        lines.push(arguments.to_string());
    }
    if immutable.component.kind == PluginComponentKind::Agent {
        if let Some(base_agent) = component_metadata_text(immutable, "base_agent") {
            lines.push(format!("Base Agent: {base_agent}"));
        }
        if let Some(max_iterations) = immutable
            .component
            .metadata
            .get("max_iterations")
            .and_then(Value::as_u64)
        {
            lines.push(format!("Maximum iterations: {max_iterations}"));
        }
        lines.push("Apply this profile only as additional instructions for the current Agent invocation. It grants no tools or permissions beyond the active MCP Runtime Session.".to_string());
    } else if immutable.component.kind == PluginComponentKind::Command {
        lines.push(
            "Follow this signed Plugin Command for the current Agent invocation:".to_string(),
        );
    } else {
        lines.push(
            "Apply this signed Plugin Skill as additional guidance for the current Agent invocation. It grants no tools or permissions beyond the active MCP Runtime Session."
                .to_string(),
        );
    }
    lines.push(prompt.to_string());
    json!({
        "content": [{"type": "text", "text": lines.join("\n")}],
        "structuredContent": {
            "plugin_id": immutable.plugin_id,
            "release_id": immutable.release_id,
            "component_key": immutable.component.component_key,
            "component_kind": immutable.component.kind,
            "content_sha256": immutable.component_content_sha256,
            "allowed_tools": immutable.component.metadata.get("allowed_tools").cloned().unwrap_or_else(|| json!([])),
            "base_agent": immutable.component.metadata.get("base_agent").cloned().unwrap_or(Value::Null),
            "max_iterations": immutable.component.metadata.get("max_iterations").cloned().unwrap_or(Value::Null),
            "runtime_snapshot_sha256": runtime_snapshot.and_then(|value| value.get("snapshot_sha256")).cloned().unwrap_or(Value::Null)
        }
    })
}
