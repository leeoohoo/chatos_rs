// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) fn validate_cloud_component_bundle(
    immutable: &PluginToolComponentRuntimeBinding,
    bundle: &chatos_plugin_management_sdk::PluginCloudComponentBundle,
) -> Result<(), ProviderCallError> {
    if bundle.plugin_id != immutable.plugin_id
        || bundle.release_id != immutable.release_id
        || bundle.version != immutable.version
        || bundle.component_key != immutable.component.component_key
        || bundle.kind != immutable.component.kind
        || bundle.execution_host != immutable.component.execution_host
        || bundle.artifact_sha256 != immutable.artifact_sha256
        || bundle.normalized_manifest_sha256 != immutable.normalized_manifest_sha256
        || bundle.bundle_sha256 != immutable.component_content_sha256
        || plugin_cloud_bundle_sha256(bundle)
            .map_err(|error| ProviderCallError::invalid_response(error.to_string()))?
            != bundle.bundle_sha256
    {
        return Err(ProviderCallError::invalid_response(
            "Plugin cloud component Bundle does not match its immutable binding",
        ));
    }
    Ok(())
}

pub(super) fn validate_cloud_component_policy(
    immutable: &PluginToolComponentRuntimeBinding,
) -> Result<(), ProviderCallError> {
    match immutable.component.kind {
        PluginComponentKind::Command => {
            if immutable
                .component
                .metadata
                .get("requires_confirmation")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                return Err(ProviderCallError::provider_unavailable(
                    "Plugin Command requires interactive confirmation that is unavailable on the cloud component path",
                ));
            }
            Ok(())
        }
        PluginComponentKind::Agent => Ok(()),
        PluginComponentKind::SkillCollection => Err(ProviderCallError::provider_unavailable(
            "cloud Plugin Skills do not publish executable tools without a cloud adapter",
        )),
        _ => Err(ProviderCallError::provider_unavailable(
            "Plugin component kind is not an Agent tool",
        )),
    }
}

pub(super) fn validate_immutable_route(
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

pub(super) fn validate_prepare_identity(
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

pub(super) fn required_operation(
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

pub(super) fn validate_native_skill_snapshot(
    immutable: &PluginToolComponentRuntimeBinding,
    native_skill: &Value,
) -> Result<(), ProviderCallError> {
    let expected_metadata = &immutable.component.metadata;
    for (field, expected) in [
        ("plugin_id", immutable.plugin_id.as_str()),
        ("release_id", immutable.release_id.as_str()),
        ("plugin_version", immutable.version.as_str()),
        ("artifact_sha256", immutable.artifact_sha256.as_str()),
        ("component_key", immutable.component.component_key.as_str()),
        ("bundle_hash", immutable.component_content_sha256.as_str()),
    ] {
        if native_skill.get(field).and_then(Value::as_str) != Some(expected) {
            return Err(ProviderCallError::invalid_response(format!(
                "native Plugin Skill response {field} does not match its immutable binding"
            )));
        }
    }
    for field in ["skill_id", "bundle_id"] {
        let expected = expected_metadata
            .get(field)
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(format!(
                    "native Plugin Skill metadata is missing {field}"
                ))
            })?;
        if native_skill.get(field).and_then(Value::as_str) != Some(expected) {
            return Err(ProviderCallError::invalid_response(format!(
                "native Plugin Skill response {field} does not match its immutable binding"
            )));
        }
    }
    for field in [
        "skill_snapshot_sha256",
        "snapshot_sha256",
        "tool_snapshot_sha256",
    ] {
        if !native_skill
            .get(field)
            .and_then(Value::as_str)
            .is_some_and(is_lower_sha256)
        {
            return Err(ProviderCallError::invalid_response(format!(
                "native Plugin Skill response is missing valid {field}"
            )));
        }
    }
    let expected_native_snapshot = hex::encode(Sha256::digest(
        format!(
            "chatos.plugin.native-skill.snapshot.v1\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            immutable.plugin_id,
            immutable.release_id,
            immutable.version,
            immutable.artifact_sha256,
            immutable.component.component_key,
            native_skill["skill_snapshot_sha256"]
                .as_str()
                .unwrap_or_default(),
            native_skill["skill_id"].as_str().unwrap_or_default(),
            native_skill["bundle_id"].as_str().unwrap_or_default(),
            native_skill["bundle_version"].as_str().unwrap_or_default(),
            immutable.component_content_sha256,
        )
        .as_bytes(),
    ));
    if native_skill.get("snapshot_sha256").and_then(Value::as_str)
        != Some(expected_native_snapshot.as_str())
    {
        return Err(ProviderCallError::invalid_response(
            "native Plugin Skill binding snapshot hash is invalid",
        ));
    }
    Ok(())
}

pub(super) fn validate_native_tool_snapshot_hash(
    native_skill: &Value,
    tools: &[Value],
) -> Result<(), ProviderCallError> {
    let snapshot_sha256 = native_skill
        .get("snapshot_sha256")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut payload = format!("chatos.plugin.native-tools.snapshot.v1\n{snapshot_sha256}");
    for tool in tools {
        payload.push('\n');
        payload.push_str(
            serde_json::to_string(tool)
                .map_err(|error| {
                    ProviderCallError::invalid_response(format!(
                        "serialize native Plugin Skill tool snapshot failed: {error}"
                    ))
                })?
                .as_str(),
        );
    }
    let expected = hex::encode(Sha256::digest(payload.as_bytes()));
    if native_skill
        .get("tool_snapshot_sha256")
        .and_then(Value::as_str)
        != Some(expected.as_str())
    {
        return Err(ProviderCallError::invalid_response(
            "native Plugin Skill tool snapshot hash is invalid",
        ));
    }
    Ok(())
}

pub(super) fn validate_tool_snapshot(tools: &[Value]) -> Result<(), ProviderCallError> {
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

pub(super) fn command_tool_definition(binding: &PluginToolComponentRuntimeBinding) -> Value {
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

pub(super) fn agent_tool_definition(binding: &PluginToolComponentRuntimeBinding) -> Value {
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

pub(super) fn validate_command_snapshot(
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

pub(super) fn validate_agent_snapshot(
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

pub(super) fn validate_execute_identity(
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

pub(super) fn validate_local_bound_route(
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

pub(super) fn validate_cloud_bound_route(
    snapshot: &RuntimeSessionSnapshot,
    route: &ResolvedMcpRoute,
    binding: &PluginCloudToolComponentBinding,
) -> Result<(), ProviderCallError> {
    let immutable = snapshot
        .plugin_tool_component_bindings
        .get(route.resource_id.as_str())
        .ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "immutable Plugin tool component binding is missing",
            )
        })?;
    if snapshot.expires_at_unix <= chrono::Utc::now().timestamp()
        || immutable != &binding.runtime
        || route.provider_ref.as_deref() != Some(binding.runtime.provider_ref.as_str())
        || route.allow_writes != binding.runtime.allow_writes
    {
        return Err(ProviderCallError::provider_unavailable(
            "Plugin Cloud tool component route does not match its prepared session",
        ));
    }
    Ok(())
}

pub(super) fn component_metadata_text<'a>(
    binding: &'a PluginToolComponentRuntimeBinding,
    key: &str,
) -> Option<&'a str> {
    binding
        .component
        .metadata
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(super) fn component_metadata_string_array(
    binding: &PluginToolComponentRuntimeBinding,
    key: &str,
) -> Result<Vec<String>, ProviderCallError> {
    value_string_array(
        binding.component.metadata.get(key),
        format!("Plugin component metadata {key}").as_str(),
    )
}

pub(super) fn value_string_array(
    value: Option<&Value>,
    label: &str,
) -> Result<Vec<String>, ProviderCallError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| {
        ProviderCallError::provider_unavailable(format!("{label} must be an array"))
    })?;
    let mut result = values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .ok_or_else(|| {
                    ProviderCallError::provider_unavailable(format!(
                        "{label} contains an invalid item"
                    ))
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    result.sort();
    result.dedup();
    Ok(result)
}

pub(super) fn normalized_value_text(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(super) fn required_value_text<'a>(
    value: &'a Value,
    field: &str,
) -> Result<&'a str, ProviderCallError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ProviderCallError::invalid_response(format!(
                "Plugin component response is missing {field}"
            ))
        })
}

pub(super) fn sha256_text(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

pub(super) fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}
