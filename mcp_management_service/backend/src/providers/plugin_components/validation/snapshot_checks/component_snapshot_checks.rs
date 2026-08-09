// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use chatos_plugin_management_sdk::{plugin_agent_snapshot_sha256, plugin_command_snapshot_sha256};

use crate::providers::ProviderCallError;
use crate::runtime::PluginToolComponentRuntimeBinding;

use super::super::value_helpers::{
    component_metadata_string_array, component_metadata_text, normalized_value_text,
    required_value_text, sha256_text, value_string_array,
};

pub(in crate::providers::plugin_components) fn validate_command_snapshot(
    immutable: &PluginToolComponentRuntimeBinding,
    command: &Value,
    expected_arguments: Option<&str>,
    confirmation_approved: bool,
) -> Result<(), ProviderCallError> {
    for (field, expected) in [
        ("plugin_id", immutable.plugin_id.as_str()),
        ("release_id", immutable.release_id.as_str()),
        ("version", immutable.version.as_str()),
        ("artifact_sha256", immutable.artifact_sha256.as_str()),
        ("component_key", immutable.component.component_key.as_str()),
        ("command_name", immutable.component.component_key.as_str()),
        (
            "content_sha256",
            immutable.component_content_sha256.as_str(),
        ),
    ] {
        if command.get(field).and_then(Value::as_str) != Some(expected) {
            return Err(ProviderCallError::invalid_response(format!(
                "Plugin Command response {field} does not match its immutable binding"
            )));
        }
    }
    let entrypoint = immutable
        .component
        .entrypoint
        .as_ref()
        .map(|entrypoint| entrypoint.path.as_str())
        .ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Plugin Command immutable binding is missing its entrypoint",
            )
        })?;
    if command.get("relative_source_path").and_then(Value::as_str) != Some(entrypoint) {
        return Err(ProviderCallError::invalid_response(
            "Plugin Command response entrypoint does not match its immutable binding",
        ));
    }
    for field in ["description", "argument_hint"] {
        if normalized_value_text(command.get(field))
            != immutable
                .component
                .metadata
                .get(field)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
        {
            return Err(ProviderCallError::invalid_response(format!(
                "Plugin Command response {field} does not match its immutable binding"
            )));
        }
    }
    let requires_confirmation = immutable
        .component
        .metadata
        .get("requires_confirmation")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let expected_arguments_sha256 = sha256_text(expected_arguments.unwrap_or_default());
    if command
        .get("requires_confirmation")
        .and_then(Value::as_bool)
        != Some(requires_confirmation)
        || command
            .get("confirmation_approved")
            .and_then(Value::as_bool)
            != Some(confirmation_approved && requires_confirmation)
        || command.get("arguments_sha256").and_then(Value::as_str)
            != Some(expected_arguments_sha256.as_str())
        || command.get("arguments_present").and_then(Value::as_bool)
            != Some(expected_arguments.is_some())
        || command.get("arguments").is_some()
    {
        return Err(ProviderCallError::invalid_response(
            "Plugin Command response confirmation or arguments snapshot is invalid",
        ));
    }
    let expected_target_agent = component_metadata_text(immutable, "target_agent");
    if normalized_value_text(command.get("target_agent")) != expected_target_agent {
        return Err(ProviderCallError::invalid_response(
            "Plugin Command target Agent does not match its immutable binding",
        ));
    }
    let expected_allowed_tools = component_metadata_string_array(immutable, "allowed_tools")?;
    if value_string_array(command.get("allowed_tools"), "Plugin Command allowed_tools")?
        != expected_allowed_tools
    {
        return Err(ProviderCallError::invalid_response(
            "Plugin Command allowed tools do not match its immutable binding",
        ));
    }
    let prompt = required_value_text(command, "prompt")?;
    let expected_snapshot_sha256 = plugin_command_snapshot_sha256(
        immutable.plugin_id.as_str(),
        immutable.release_id.as_str(),
        immutable.component.component_key.as_str(),
        immutable.component.execution_host,
        entrypoint,
        component_metadata_text(immutable, "description"),
        component_metadata_text(immutable, "argument_hint"),
        requires_confirmation,
        expected_target_agent,
        expected_allowed_tools.as_slice(),
        immutable.component_content_sha256.as_str(),
        prompt,
        expected_arguments_sha256.as_str(),
    )
    .map_err(|error| ProviderCallError::invalid_response(error.to_string()))?;
    if command.get("snapshot_sha256").and_then(Value::as_str)
        != Some(expected_snapshot_sha256.as_str())
    {
        return Err(ProviderCallError::invalid_response(
            "Plugin Command response snapshot hash is invalid",
        ));
    }
    Ok(())
}

pub(in crate::providers::plugin_components) fn validate_agent_snapshot(
    immutable: &PluginToolComponentRuntimeBinding,
    agent: &Value,
) -> Result<(), ProviderCallError> {
    for (field, expected) in [
        ("plugin_id", immutable.plugin_id.as_str()),
        ("release_id", immutable.release_id.as_str()),
        ("version", immutable.version.as_str()),
        ("artifact_sha256", immutable.artifact_sha256.as_str()),
        ("component_key", immutable.component.component_key.as_str()),
        ("agent_name", immutable.component.component_key.as_str()),
        (
            "content_sha256",
            immutable.component_content_sha256.as_str(),
        ),
    ] {
        if agent.get(field).and_then(Value::as_str) != Some(expected) {
            return Err(ProviderCallError::invalid_response(format!(
                "Plugin Agent response {field} does not match its immutable binding"
            )));
        }
    }
    let entrypoint = immutable
        .component
        .entrypoint
        .as_ref()
        .map(|entrypoint| entrypoint.path.as_str())
        .ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Plugin Agent immutable binding is missing its entrypoint",
            )
        })?;
    if agent.get("relative_source_path").and_then(Value::as_str) != Some(entrypoint)
        || normalized_value_text(agent.get("description"))
            != component_metadata_text(immutable, "description")
    {
        return Err(ProviderCallError::invalid_response(
            "Plugin Agent response metadata does not match its immutable binding",
        ));
    }
    let base_agent = component_metadata_text(immutable, "base_agent").ok_or_else(|| {
        ProviderCallError::provider_unavailable(
            "Plugin Agent immutable binding is missing base_agent",
        )
    })?;
    let allowed_tools = component_metadata_string_array(immutable, "allowed_tools")?;
    let max_iterations = immutable
        .component
        .metadata
        .get("max_iterations")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Plugin Agent immutable binding is missing max_iterations",
            )
        })?;
    if agent.get("base_agent").and_then(Value::as_str) != Some(base_agent)
        || value_string_array(agent.get("allowed_tools"), "Plugin Agent allowed_tools")?
            != allowed_tools
        || agent.get("max_iterations").and_then(Value::as_u64) != Some(max_iterations)
    {
        return Err(ProviderCallError::invalid_response(
            "Plugin Agent execution constraints do not match its immutable binding",
        ));
    }
    let prompt = required_value_text(agent, "prompt")?;
    let expected_snapshot_sha256 = plugin_agent_snapshot_sha256(
        immutable.plugin_id.as_str(),
        immutable.release_id.as_str(),
        immutable.component.component_key.as_str(),
        immutable.component.execution_host,
        entrypoint,
        component_metadata_text(immutable, "description"),
        base_agent,
        allowed_tools.as_slice(),
        usize::try_from(max_iterations).map_err(|_| {
            ProviderCallError::provider_unavailable("Plugin Agent max_iterations is invalid")
        })?,
        immutable.component_content_sha256.as_str(),
        prompt,
    )
    .map_err(|error| ProviderCallError::invalid_response(error.to_string()))?;
    if agent.get("snapshot_sha256").and_then(Value::as_str)
        != Some(expected_snapshot_sha256.as_str())
    {
        return Err(ProviderCallError::invalid_response(
            "Plugin Agent response snapshot hash is invalid",
        ));
    }
    Ok(())
}
