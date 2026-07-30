// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::{
    plugin_agent_snapshot_sha256, plugin_command_snapshot_sha256, RunPluginComponentSnapshot,
    RunPluginSnapshot,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{
    agent_base_agent, agent_max_iterations, command_target_agent, component_allowed_tools,
    required_response_text,
};

pub(super) fn validate_command_response(
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

pub(super) fn validate_agent_response(
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

pub(super) fn validate_native_skill_response(
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
