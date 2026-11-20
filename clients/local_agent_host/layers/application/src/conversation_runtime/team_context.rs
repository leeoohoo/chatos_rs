fn normalized_team_executor_context(
    metadata: &serde_json::Value,
    conversation_id: &str,
    conversation_resource: Option<&LocalConversationResourceBinding>,
) -> Result<Option<serde_json::Value>, LocalAgentRuntimeError> {
    let Some(context) = metadata.get("agent_team_execution") else {
        return Ok(None);
    };
    if metadata.get("source").and_then(serde_json::Value::as_str) != Some("native_agent_team") {
        return Err(invalid_agent_context(
            "executor context requires the native_agent_team source",
        ));
    }
    let values = context
        .as_object()
        .ok_or_else(|| invalid_agent_context("agent_team_execution must be an object"))?;
    const KEYS: [&str; 9] = [
        "schema_version",
        "delivery_lane",
        "trigger_kind",
        "delivery_id",
        "todo_id",
        "team_id",
        "agent_id",
        "project_id",
        "execution_plan",
    ];
    if values.len() != KEYS.len()
        || values.keys().any(|key| !KEYS.contains(&key.as_str()))
        || values
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            != Some(1)
        || values
            .get("delivery_lane")
            .and_then(serde_json::Value::as_str)
            != Some("executor")
        || values
            .get("trigger_kind")
            .and_then(serde_json::Value::as_str)
            != Some("todo")
    {
        return Err(invalid_agent_context(
            "executor context fields do not match the schema",
        ));
    }
    let delivery_id = agent_context_string(values, "delivery_id", 512, true)?;
    let todo_id = agent_context_string(values, "todo_id", 512, true)?;
    let team_id = agent_context_string(values, "team_id", 512, true)?;
    let agent_id = agent_context_string(values, "agent_id", 512, true)?;
    let project_id = agent_context_string(values, "project_id", 512, true)?;
    if conversation_id != format!("agent-team-executor-{delivery_id}") {
        return Err(invalid_agent_context(
            "delivery_id does not match the executor conversation identity",
        ));
    }
    if conversation_resource.is_some() {
        return Err(invalid_agent_context(
            "executor conversations must not claim a user-facing resource binding",
        ));
    }
    let plan = values
        .get("execution_plan")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| invalid_agent_context("execution_plan must be an object"))?;
    const PLAN_KEYS: [&str; 3] = ["requires_execution", "builtin_capabilities", "plugins"];
    if plan.len() != PLAN_KEYS.len() || plan.keys().any(|key| !PLAN_KEYS.contains(&key.as_str())) {
        return Err(invalid_agent_context(
            "execution_plan fields do not match the schema",
        ));
    }
    let requires_execution = plan
        .get("requires_execution")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| invalid_agent_context("requires_execution must be a boolean"))?;
    let capabilities = plan
        .get("builtin_capabilities")
        .and_then(serde_json::Value::as_array)
        .filter(|values| values.len() <= 5)
        .ok_or_else(|| invalid_agent_context("builtin_capabilities must be an array"))?;
    let mut unique = std::collections::HashSet::new();
    let mut enabled_builtin_kinds = Vec::with_capacity(capabilities.len());
    for capability in capabilities {
        let capability = capability
            .as_str()
            .ok_or_else(|| invalid_agent_context("builtin_capabilities must contain strings"))?;
        let builtin_kind = match capability {
            "project_read" => "CodeMaintainerRead",
            "project_write" if requires_execution => "CodeMaintainerWrite",
            "terminal" if requires_execution => "TerminalController",
            "requirement_survey_read" => "RequirementSurveyRead",
            "requirement_survey_write" if requires_execution => "RequirementSurveyWrite",
            _ => {
                return Err(invalid_agent_context(
                    "execution_plan contains an unauthorized capability",
                ))
            }
        };
        if !unique.insert(capability) {
            return Err(invalid_agent_context("builtin_capabilities must be unique"));
        }
        enabled_builtin_kinds.push(builtin_kind);
    }
    let plugins = plan
        .get("plugins")
        .and_then(serde_json::Value::as_array)
        .filter(|values| values.len() <= 32)
        .ok_or_else(|| invalid_agent_context("plugins must be an array"))?;
    let mut plugin_selection_ids = std::collections::HashSet::new();
    let mut normalized_plugins = Vec::with_capacity(plugins.len());
    let mut plugin_hints = Vec::with_capacity(plugins.len());
    let mut plugin_release_snapshots = Vec::with_capacity(plugins.len());
    for plugin in plugins {
        let plugin = plugin
            .as_object()
            .ok_or_else(|| invalid_agent_context("plugins must contain objects"))?;
        const PLUGIN_KEYS: [&str; 9] = [
            "installationId",
            "pluginId",
            "componentKey",
            "componentRevision",
            "releaseId",
            "releaseDigest",
            "manifestVersion",
            "displayName",
            "reason",
        ];
        if plugin.len() != PLUGIN_KEYS.len()
            || plugin
                .keys()
                .any(|key| !PLUGIN_KEYS.contains(&key.as_str()))
        {
            return Err(invalid_agent_context(
                "Plugin selection fields do not match the schema",
            ));
        }
        let installation_id = agent_context_string(plugin, "installationId", 1_024, true)?;
        let plugin_id = agent_context_string(plugin, "pluginId", 1_024, true)?;
        let component_key = agent_context_string(plugin, "componentKey", 1_024, true)?;
        let component_revision = agent_context_string(plugin, "componentRevision", 1_024, true)?;
        let release_id = agent_context_string(plugin, "releaseId", 1_024, true)?;
        let release_digest = agent_context_string(plugin, "releaseDigest", 512, true)?;
        let digest = release_digest
            .strip_prefix("sha256:")
            .unwrap_or(release_digest);
        if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(invalid_agent_context("releaseDigest is invalid"));
        }
        let manifest_version = agent_context_string(plugin, "manifestVersion", 128, false)?;
        let display_name = agent_context_string(plugin, "displayName", 240, false)?;
        let reason = agent_context_optional_text(plugin, "reason", 1_000)?;
        if !plugin_selection_ids.insert((plugin_id, component_key)) {
            return Err(invalid_agent_context("Plugin selections must be unique"));
        }
        normalized_plugins.push(json!({
            "installation_id": installation_id,
            "plugin_id": plugin_id,
            "component_key": component_key,
            "component_revision": component_revision,
            "release_id": release_id,
            "release_digest": release_digest,
            "manifest_version": manifest_version,
            "display_name": display_name,
            "reason": reason,
        }));
        plugin_hints.push(json!({
            "plugin_key": plugin_id,
            "component_key": component_key,
            "reason": reason,
        }));
        plugin_release_snapshots.push(json!({
            "plugin_key": plugin_id,
            "component_key": component_key,
            "plugin_id": plugin_id,
            "release_id": release_id,
            "version": manifest_version,
            "artifact_sha256": release_digest,
        }));
    }
    Ok(Some(json!({
        "schema_version": 1,
        "delivery_lane": "executor",
        "delivery_id": delivery_id,
        "todo_id": todo_id,
        "team_id": team_id,
        "agent_id": agent_id,
        "project_id": project_id,
        "execution_plan": {
            "requires_execution": requires_execution,
            "builtin_capabilities": capabilities,
            "plugins": normalized_plugins,
        },
        "tool_options": {
            "requires_execution": requires_execution,
            "enabled_builtin_kinds": enabled_builtin_kinds,
            "plugin_hints": plugin_hints,
            "plugin_release_snapshots": plugin_release_snapshots,
            "external_mcp_config_ids": [],
        },
    })))
}

fn agent_context_optional_text<'a>(
    values: &'a serde_json::Map<String, serde_json::Value>,
    field: &str,
    maximum: usize,
) -> Result<&'a str, LocalAgentRuntimeError> {
    let value = values
        .get(field)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid_agent_context(&format!("{field} must be a string")))?;
    if value.trim() != value || value.chars().count() > maximum || value.contains('\0') {
        return Err(invalid_agent_context(&format!("{field} is invalid")));
    }
    Ok(value)
}

fn agent_context_string<'a>(
    values: &'a serde_json::Map<String, serde_json::Value>,
    field: &str,
    maximum: usize,
    identifier: bool,
) -> Result<&'a str, LocalAgentRuntimeError> {
    let value = values
        .get(field)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid_agent_context(&format!("{field} must be a string")))?;
    let valid = !value.is_empty()
        && value.trim() == value
        && value.chars().count() <= maximum
        && !value.contains('\0')
        && (!identifier
            || value
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "-_.:|".contains(character)));
    if !valid {
        return Err(invalid_agent_context(&format!("{field} is invalid")));
    }
    Ok(value)
}

fn agent_context_identifiers<'a>(
    values: &'a serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Result<Vec<&'a str>, LocalAgentRuntimeError> {
    let items = values
        .get(field)
        .and_then(serde_json::Value::as_array)
        .filter(|items| items.len() <= 100)
        .ok_or_else(|| invalid_agent_context(&format!("{field} must be an array")))?;
    let mut unique = std::collections::HashSet::new();
    let mut result = Vec::with_capacity(items.len());
    for item in items {
        let value = item
            .as_str()
            .ok_or_else(|| invalid_agent_context(&format!("{field} must contain strings")))?;
        let valid = !value.is_empty()
            && value.trim() == value
            && value.chars().count() <= 512
            && !value.chars().any(char::is_control);
        if !valid || !unique.insert(value) {
            return Err(invalid_agent_context(&format!(
                "{field} contains an invalid value"
            )));
        }
        result.push(value);
    }
    Ok(result)
}

fn invalid_agent_context(detail: &str) -> LocalAgentRuntimeError {
    LocalAgentRuntimeError::InvalidRequest(format!("invalid Agent execution context: {detail}"))
}
