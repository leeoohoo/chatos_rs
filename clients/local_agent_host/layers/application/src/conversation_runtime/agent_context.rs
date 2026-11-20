fn normalized_conversation_agent_identity(
    metadata: &serde_json::Value,
    conversation_resource: Option<&LocalConversationResourceBinding>,
) -> Result<(String, Option<String>), LocalAgentRuntimeError> {
    let values = metadata
        .as_object()
        .ok_or_else(|| invalid_agent_context("message metadata must be an object"))?;
    let agent_id = agent_context_string(values, "agent_id", 512, true)?;
    let conversation_resource = conversation_resource.ok_or_else(|| {
        invalid_agent_context("Ordinary Agent conversations require an Agent-bound resource")
    })?;
    match conversation_resource {
        LocalConversationResourceBinding {
            kind: LocalConversationResourceKind::Project,
            ..
        } if agent_id != JIGULI_AGENT_ID => {
            return Err(invalid_agent_context(
                "Project conversations must use the jiguli Agent identity",
            ));
        }
        LocalConversationResourceBinding {
            kind: LocalConversationResourceKind::Contact,
            resource_id,
        } if agent_id != resource_id => {
            return Err(invalid_agent_context(
                "Contact resource does not match the Agent identity",
            ));
        }
        _ => {}
    }
    let agent_name = values
        .get("agent_name")
        .map(|_| agent_context_string(values, "agent_name", 120, false))
        .transpose()?
        .map(ToOwned::to_owned);
    Ok((agent_id.to_string(), agent_name))
}

fn normalized_agent_execution_context(
    metadata: &serde_json::Value,
    conversation_id: &str,
    conversation_resource: Option<&LocalConversationResourceBinding>,
    team_executor_context: Option<&serde_json::Value>,
) -> Result<Option<serde_json::Value>, LocalAgentRuntimeError> {
    let Some(context) = metadata.get("agent_execution_context") else {
        return Ok(None);
    };
    let source = metadata
        .get("source")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid_agent_context("source is required"))?;
    if !matches!(source, "native_agent_direct" | "native_agent_team") {
        return Err(invalid_agent_context(
            "source is not an Agent execution surface",
        ));
    }
    let values = context
        .as_object()
        .ok_or_else(|| invalid_agent_context("context must be an object"))?;
    const DIRECT_KEYS: [&str; 12] = [
        "schema_version",
        "room_id",
        "agent_id",
        "agent_name",
        "role_prompt",
        "profession_key",
        "profession_label",
        "profession_skill_ref",
        "profession_skill_sha256",
        "profession_skill_markdown",
        "default_plugin_ids",
        "default_skill_ids",
    ];
    const TEAM_KEYS: [&str; 18] = [
        "schema_version",
        "room_id",
        "agent_id",
        "agent_name",
        "role_prompt",
        "profession_key",
        "profession_label",
        "profession_skill_ref",
        "profession_skill_sha256",
        "profession_skill_markdown",
        "default_plugin_ids",
        "default_skill_ids",
        "project_id",
        "team_name",
        "team_goal",
        "team_role",
        "team_responsibility",
        "is_project_manager",
    ];
    let schema_version = values
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| invalid_agent_context("schema_version is required"))?;
    let expected_keys: &[&str] = match (source, schema_version) {
        ("native_agent_direct", 1) => &DIRECT_KEYS,
        ("native_agent_team", 2) => &TEAM_KEYS,
        _ => {
            return Err(invalid_agent_context(
                "source and schema_version do not match",
            ))
        }
    };
    if values.len() != expected_keys.len()
        || values
            .keys()
            .any(|key| !expected_keys.contains(&key.as_str()))
    {
        return Err(invalid_agent_context(
            "context fields do not match the schema",
        ));
    }
    let room_id = agent_context_string(values, "room_id", 512, true)?;
    let agent_id = agent_context_string(values, "agent_id", 512, true)?;
    if team_executor_context.is_none() {
        match conversation_resource {
            Some(LocalConversationResourceBinding {
                kind: LocalConversationResourceKind::Contact,
                resource_id,
            }) if resource_id == agent_id => {}
            Some(LocalConversationResourceBinding {
                kind: LocalConversationResourceKind::Contact,
                ..
            }) => {
                return Err(invalid_agent_context(
                    "Contact resource does not match the Agent identity",
                ))
            }
            Some(LocalConversationResourceBinding {
                kind: LocalConversationResourceKind::Project,
                ..
            }) => {
                return Err(invalid_agent_context(
                    "Agent conversations must use a Contact resource",
                ))
            }
            None => {
                return Err(invalid_agent_context(
                    "Agent conversations require a Contact resource",
                ))
            }
        }
    }
    let expected_conversation_id = if let Some(executor) = team_executor_context {
        for (field, expected) in [("team_id", room_id), ("agent_id", agent_id)] {
            if executor.get(field).and_then(serde_json::Value::as_str) != Some(expected) {
                return Err(invalid_agent_context(&format!(
                    "executor {field} does not match the Agent identity"
                )));
            }
        }
        conversation_id.to_string()
    } else if source == "native_agent_team" {
        format!("agent-team-v2-{room_id}-{agent_id}")
    } else {
        format!("agent-direct-v2-{room_id}")
    };
    if conversation_id != expected_conversation_id {
        return Err(invalid_agent_context(
            "room_id does not match the Host conversation identity",
        ));
    }
    let agent_name = agent_context_string(values, "agent_name", 120, false)?;
    let role_prompt = agent_context_string(values, "role_prompt", 32_000, false)?;
    let profession_key = agent_context_string(values, "profession_key", 512, true)?;
    let profession_label = agent_context_string(values, "profession_label", 200, false)?;
    let profession_skill_ref = agent_context_string(values, "profession_skill_ref", 700, true)?;
    let profession_skill_sha256 =
        agent_context_string(values, "profession_skill_sha256", 64, true)?;
    if profession_skill_sha256.len() != 64
        || !profession_skill_sha256
            .bytes()
            .all(|value| value.is_ascii_digit() || (b'a'..=b'f').contains(&value))
    {
        return Err(invalid_agent_context(
            "profession_skill_sha256 must be lowercase SHA-256",
        ));
    }
    let profession_skill_markdown =
        agent_context_string(values, "profession_skill_markdown", 500_000, false)?;
    let default_plugin_ids = agent_context_identifiers(values, "default_plugin_ids")?;
    let default_skill_ids = agent_context_identifiers(values, "default_skill_ids")?;
    let mut normalized = json!({
        "schema_version": 1,
        "room_id": room_id,
        "agent_id": agent_id,
        "agent_name": agent_name,
        "role_prompt": role_prompt,
        "profession_key": profession_key,
        "profession_label": profession_label,
        "profession_skill_ref": profession_skill_ref,
        "profession_skill_sha256": profession_skill_sha256,
        "profession_skill_markdown": profession_skill_markdown,
        "default_plugin_ids": default_plugin_ids,
        "default_skill_ids": default_skill_ids,
    });
    if source == "native_agent_team" {
        let project_id = agent_context_string(values, "project_id", 512, true)?;
        if let Some(executor) = team_executor_context {
            if executor
                .get("project_id")
                .and_then(serde_json::Value::as_str)
                != Some(project_id)
            {
                return Err(invalid_agent_context(
                    "executor project_id does not match the Agent identity",
                ));
            }
        }
        let team_name = agent_context_string(values, "team_name", 160, false)?;
        let team_goal = agent_context_optional_text(values, "team_goal", 8_000)?;
        let team_role = agent_context_string(values, "team_role", 160, false)?;
        let team_responsibility =
            agent_context_optional_text(values, "team_responsibility", 8_000)?;
        let is_project_manager = values
            .get("is_project_manager")
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(|| invalid_agent_context("is_project_manager must be a boolean"))?;
        let object = normalized
            .as_object_mut()
            .expect("normalized Agent context is an object");
        object.insert("schema_version".into(), json!(2));
        object.insert("project_id".into(), json!(project_id));
        object.insert("team_name".into(), json!(team_name));
        object.insert("team_goal".into(), json!(team_goal));
        object.insert("team_role".into(), json!(team_role));
        object.insert("team_responsibility".into(), json!(team_responsibility));
        object.insert("is_project_manager".into(), json!(is_project_manager));
    }
    Ok(Some(normalized))
}
