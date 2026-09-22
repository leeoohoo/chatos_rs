fn runtime_grant_claims(snapshot: &RuntimeSessionSnapshot) -> RuntimeGrantClaims {
    RuntimeGrantClaims {
        iss: String::new(),
        sub: snapshot.caller_service.clone(),
        aud: String::new(),
        session_id: snapshot.session_id.clone(),
        trace_id: snapshot.trace_id.clone(),
        tenant_id: snapshot.tenant_id.clone(),
        owner_user_id: snapshot.owner_user_id.clone(),
        agent_key: snapshot.agent_key.clone(),
        task_profile: snapshot.task_profile.clone(),
        project_id: snapshot.project_id.clone(),
        device_id: snapshot.device_id.clone(),
        run_id: snapshot.run_id.clone(),
        turn_id: snapshot.turn_id.clone(),
        task_id: snapshot.task_id.clone(),
        source_session_id: snapshot.source_session_id.clone(),
        source_user_message_id: snapshot.source_user_message_id.clone(),
        contact_agent_id: snapshot.contact_agent_id.clone(),
        default_model_config_id: snapshot.default_model_config_id.clone(),
        default_remote_connection_id: snapshot.default_remote_connection_id.clone(),
        policy_revision: snapshot.policy_revision.clone(),
        route_revision: snapshot.route_revision.clone(),
        allowed_resource_ids: snapshot
            .routes
            .iter()
            .map(|route| route.resource_id.clone())
            .collect(),
        iat: 0,
        exp: 0,
    }
}

pub(super) async fn close_runtime_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<CloseRuntimeSessionResponse>, ApiError> {
    let identity =
        require_internal_request_identity(&state.config, &headers, "runtime.sessions.close")?;
    let trace_id = identity.require_signed_trace_id()?.to_string();
    let terminal_status = headers
        .get("x-mcp-management-terminal-status")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| {
            matches!(
                *value,
                "succeeded" | "failed" | "cancelled" | "blocked" | "waived" | "closed"
            )
        })
        .unwrap_or("closed");
    let session_id = session_id.trim();
    if let Some(response) = state
        .runtime_session_closes
        .get(session_id, identity.caller.as_str())
        .await
        .map_err(ApiError::internal)?
    {
        return Ok(Json(response));
    }
    let snapshot = state
        .runtime_sessions
        .get(session_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("runtime session was not found or has expired"))?;
    if snapshot.caller_service != identity.caller {
        return Err(ApiError::forbidden(
            "runtime session belongs to another caller service",
        ));
    }
    let reclaimed_invocations = state
        .runtime_invocations
        .close_session(snapshot.session_id.as_str())
        .await;
    let reclaimed_invocations = reclaimed_invocations.map_err(|error| {
        tracing::error!(
            session_id = snapshot.session_id.as_str(),
            error = error.as_str(),
            "close active Runtime Invocations for Runtime Session failed"
        );
        ApiError::internal(error)
    })?;
    let execution_scope_released = if let Some(run_id) = snapshot.run_id.as_deref() {
        let provider = snapshot.execution_scope_provider();
        state
            .runtime_execution_scopes
            .detach_session(
                snapshot.owner_user_id.as_str(),
                snapshot.project_id.as_deref(),
                run_id,
                provider,
                snapshot.session_id.as_str(),
            )
            .await
            .map_err(ApiError::internal)?
    } else {
        true
    };
    let provider_finalization = match state
        .providers
        .close_session(&snapshot, execution_scope_released, terminal_status)
        .await
    {
        Ok(result) => result,
        Err(error) => {
            if execution_scope_released {
                if let Some(run_id) = snapshot.run_id.as_deref() {
                    let provider = snapshot.execution_scope_provider();
                    if let Err(reattach_error) = state
                        .runtime_execution_scopes
                        .attach_session(
                            snapshot.owner_user_id.as_str(),
                            snapshot.project_id.as_deref(),
                            run_id,
                            provider,
                            snapshot.session_id.as_str(),
                            snapshot.expires_at_unix,
                        )
                        .await
                    {
                        tracing::error!(
                            session_id = snapshot.session_id.as_str(),
                            error = %reattach_error,
                            "failed to restore Runtime Session execution scope after provider finalization error"
                        );
                    }
                }
            }
            return Err(ApiError::bad_gateway(error.message));
        }
    };
    tracing::info!(
        session_id = snapshot.session_id.as_str(),
        reclaimed_invocations,
        "closed Runtime Session active invocations"
    );
    let integration_conflict = provider_finalization
        .as_ref()
        .is_some_and(|result| result.status == RuntimeProviderFinalizationStatus::Conflict);
    if integration_conflict {
        if execution_scope_released {
            if let Some(run_id) = snapshot.run_id.as_deref() {
                let provider = snapshot.execution_scope_provider();
                state
                    .runtime_execution_scopes
                    .attach_session(
                        snapshot.owner_user_id.as_str(),
                        snapshot.project_id.as_deref(),
                        run_id,
                        provider,
                        snapshot.session_id.as_str(),
                        snapshot.expires_at_unix,
                    )
                    .await
                    .map_err(|error| ApiError::internal(error.to_string()))?;
            }
        }
        record_runtime_session_audit(&identity.caller, trace_id, &snapshot, "close", "conflict");
        return Ok(Json(CloseRuntimeSessionResponse {
            session_id: snapshot.session_id.clone(),
            closed: false,
            provider_finalization,
        }));
    }
    let close_response = CloseRuntimeSessionResponse {
        session_id: snapshot.session_id.clone(),
        closed: true,
        provider_finalization,
    };
    state
        .runtime_session_closes
        .save(
            identity.caller.as_str(),
            close_response.clone(),
            chrono::Utc::now()
                .timestamp()
                .saturating_add(7 * 24 * 60 * 60),
        )
        .await
        .map_err(ApiError::internal)?;
    let _removed = state
        .runtime_sessions
        .remove(session_id)
        .await
        .map_err(ApiError::internal)?;
    record_runtime_session_audit(&identity.caller, trace_id, &snapshot, "close", "succeeded");
    Ok(Json(close_response))
}

fn record_runtime_session_audit(
    caller_service: &str,
    trace_id: String,
    snapshot: &RuntimeSessionSnapshot,
    action: &str,
    outcome: &str,
) {
    let event = chatos_service_runtime::InternalResourceAccessAudit {
        caller_service: caller_service.to_string(),
        audience_service: "mcp-management-service".to_string(),
        scope: format!("runtime.sessions.{action}"),
        trace_id,
        represented_user_id: Some(snapshot.owner_user_id.clone()),
        tenant_id: Some(snapshot.tenant_id.clone()),
        project_id: snapshot.project_id.clone(),
        resource_type: "mcp_runtime_session".to_string(),
        resource_id: snapshot.session_id.clone(),
        resource_name: None,
        action: action.to_string(),
        outcome: outcome.to_string(),
    };
    if let Err(error) = chatos_service_runtime::record_internal_resource_access(&event) {
        tracing::error!(
            session_id = snapshot.session_id.as_str(),
            error = error.as_str(),
            "record MCP Runtime Session audit failed"
        );
    }
}

fn apply_live_tool_snapshots(
    capabilities: &mut chatos_plugin_management_sdk::ResolvedAgentCapabilities,
    mut snapshots: HashMap<String, Vec<serde_json::Value>>,
) {
    for resolved in &mut capabilities.mcps {
        if let Some(tools) = snapshots.remove(resolved.resource.id.as_str()) {
            resolved.tool_snapshot = tools;
            resolved.available = true;
            resolved.status = "ready".to_string();
            resolved.reason = None;
        }
    }
}

fn apply_requested_mcp_scope(
    capabilities: &mut chatos_plugin_management_sdk::ResolvedAgentCapabilities,
    requested_mcp_ids: Option<&[String]>,
) -> Result<(), ApiError> {
    let Some(requested_mcp_ids) = requested_mcp_ids else {
        return Ok(());
    };
    let requested = requested_mcp_ids.iter().cloned().collect::<HashSet<_>>();
    let available = capabilities
        .mcps
        .iter()
        .map(|resolved| resolved.resource.id.clone())
        .collect::<HashSet<_>>();
    let mut unknown = requested
        .difference(&available)
        .cloned()
        .collect::<Vec<_>>();
    unknown.sort();
    if !unknown.is_empty() {
        return Err(ApiError::conflict(format!(
            "requested MCP resources are not present in the configured Agent policy: {}",
            unknown.join(", ")
        )));
    }
    capabilities.mcps.retain(|resolved| {
        resolved.binding.required || requested.contains(resolved.resource.id.as_str())
    });
    for resolved in &mut capabilities.mcps {
        if requested.contains(resolved.resource.id.as_str()) {
            resolved.binding.required = true;
        }
    }
    Ok(())
}

fn apply_selected_plugin_scope(
    capabilities: &mut ResolvedAgentCapabilities,
    selected_plugins: &[SelectedPluginRef],
) -> Result<(), ApiError> {
    let mut selected_by_id = HashMap::new();
    for selected in selected_plugins {
        let plugin_id = selected.plugin_id.trim();
        if plugin_id.is_empty() {
            return Err(ApiError::bad_request("selected Plugin id is required"));
        }
        if !selected.selected_agent_ids.is_empty() {
            return Err(ApiError::bad_request(
                "Plugin Agent selection is not supported for runtime sessions",
            ));
        }
        if selected_by_id.insert(plugin_id, selected).is_some() {
            return Err(ApiError::bad_request(format!(
                "Plugin is selected more than once: {plugin_id}"
            )));
        }
    }
    let known = capabilities
        .plugins
        .iter()
        .map(|plugin| plugin.catalog.id.as_str())
        .collect::<HashSet<_>>();
    let mut unknown = selected_by_id
        .keys()
        .filter(|plugin_id| !known.contains(**plugin_id))
        .copied()
        .collect::<Vec<_>>();
    unknown.sort();
    if !unknown.is_empty() {
        return Err(ApiError::conflict(format!(
            "selected Plugins are not present in the configured Agent policy: {}",
            unknown.join(", ")
        )));
    }
    capabilities.plugins.retain_mut(|plugin| {
        let selected = selected_by_id.get(plugin.catalog.id.as_str()).copied();
        if selected.is_none() && !plugin.binding.required {
            return false;
        }
        if let Some(selected) = selected {
            // A Plugin explicitly selected for this runtime is part of the requested
            // capability contract. Preparation failures must not be silently reduced
            // to a runtime that only exposes unrelated builtin tools.
            plugin.binding.required = true;
            let selected_skills = normalized_plugin_component_ids(
                selected.selected_skill_ids.as_slice(),
                "selected_skill_ids",
            );
            let selected_commands = normalized_plugin_component_ids(
                selected.selected_command_ids.as_slice(),
                "selected_command_ids",
            );
            let selected_skills = match selected_skills {
                Ok(value) => value,
                Err(error) => {
                    plugin.available = false;
                    plugin.reason = Some(error);
                    return true;
                }
            };
            let selected_commands = match selected_commands {
                Ok(value) => value,
                Err(error) => {
                    plugin.available = false;
                    plugin.reason = Some(error);
                    return true;
                }
            };
            plugin
                .components
                .retain(|component| match component.component.kind {
                    PluginComponentKind::SkillCollection => {
                        selected_skills.is_empty()
                            || selected_skills.contains(component.component.component_key.as_str())
                            || component.component.required
                    }
                    PluginComponentKind::Command => {
                        selected_commands.contains(component.component.component_key.as_str())
                            || component.component.required
                    }
                    PluginComponentKind::Agent => false,
                    _ => true,
                });
        }
        true
    });
    Ok(())
}

fn validate_plugin_command_invocations(
    selected_plugins: &[SelectedPluginRef],
    invocations: &[chatos_plugin_management_sdk::PluginCommandInvocation],
) -> Result<HashMap<(String, String), Option<String>>, ApiError> {
    const MAX_INVOCATIONS: usize = 64;
    const MAX_ARGUMENT_BYTES: usize = 16 * 1024;
    if invocations.len() > MAX_INVOCATIONS {
        return Err(ApiError::bad_request(format!(
            "plugin_command_invocations must contain at most {MAX_INVOCATIONS} items"
        )));
    }
    let selected_commands = selected_plugins
        .iter()
        .flat_map(|selected| {
            selected.selected_command_ids.iter().map(move |command_id| {
                (
                    selected.plugin_id.trim().to_string(),
                    command_id.trim().to_string(),
                )
            })
        })
        .collect::<HashSet<_>>();
    let mut normalized = HashMap::new();
    for invocation in invocations {
        let plugin_id = invocation.plugin_id.trim();
        let command_id = invocation.command_id.trim();
        if plugin_id.is_empty() || command_id.is_empty() {
            return Err(ApiError::bad_request(
                "Plugin Command invocation identity is required",
            ));
        }
        let key = (plugin_id.to_string(), command_id.to_string());
        if !selected_commands.contains(&key) {
            return Err(ApiError::bad_request(format!(
                "Plugin Command invocation is not selected: {plugin_id}:{command_id}"
            )));
        }
        if normalized.contains_key(&key) {
            return Err(ApiError::bad_request(format!(
                "Plugin Command invocation is duplicated: {plugin_id}:{command_id}"
            )));
        }
        let arguments = invocation
            .arguments
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if arguments.is_some_and(|value| value.len() > MAX_ARGUMENT_BYTES || value.contains('\0')) {
            return Err(ApiError::bad_request(format!(
                "Plugin Command arguments are invalid: {plugin_id}:{command_id}"
            )));
        }
        normalized.insert(key, arguments.map(ToOwned::to_owned));
    }
    Ok(normalized)
}

fn apply_plugin_command_arguments(
    bindings: &mut HashMap<String, crate::runtime::PluginToolComponentRuntimeBinding>,
    command_arguments: &HashMap<(String, String), Option<String>>,
) {
    for binding in bindings.values_mut() {
        if binding.component.kind != PluginComponentKind::Command {
            continue;
        }
        binding.command_arguments = command_arguments
            .get(&(
                binding.plugin_id.clone(),
                binding.component.component_key.clone(),
            ))
            .cloned()
            .flatten();
    }
}

fn normalized_plugin_component_ids(
    values: &[String],
    field: &str,
) -> Result<HashSet<String>, String> {
    let mut normalized = HashSet::new();
    for value in values {
        let value = value.trim();
        if value.is_empty() {
            return Err(format!("{field} contains an empty component id"));
        }
        if !normalized.insert(value.to_string()) {
            return Err(format!(
                "{field} contains a duplicate component id: {value}"
            ));
        }
    }
    Ok(normalized)
}

fn capability_runtime_provider(
    context: &chatos_mcp_management_sdk::ProjectExecutionContext,
) -> &'static str {
    match context.workspace_provider {
        WorkspaceProviderKind::LocalConnector => WorkspaceProviderKind::LocalConnector.as_str(),
        WorkspaceProviderKind::None => "server",
    }
}

fn validate_session_request(request: &CreateRuntimeSessionRequest) -> Result<(), ApiError> {
    for (field, value) in [
        ("tenant_id", request.tenant_id.as_str()),
        ("owner_user_id", request.owner_user_id.as_str()),
        ("agent_key", request.agent_key.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(ApiError::bad_request(format!("{field} is required")));
        }
    }
    if request
        .tool_result_max_chars
        .is_some_and(|value| !(1..=10_000_000).contains(&value))
    {
        return Err(ApiError::bad_request(
            "tool_result_max_chars must be between 1 and 10000000",
        ));
    }
    if request
        .task_title
        .as_deref()
        .is_some_and(|value| value.trim().chars().count() > 256)
    {
        return Err(ApiError::bad_request(
            "task_title must not exceed 256 characters",
        ));
    }
    Ok(())
}

fn normalized_unique_items(
    values: Vec<String>,
    field: &str,
    max_items: usize,
) -> Result<Vec<String>, ApiError> {
    let mut values = values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    if values.len() > max_items {
        return Err(ApiError::bad_request(format!(
            "{field} exceeds {max_items} items"
        )));
    }
    if values.iter().any(|value| value.len() > 256) {
        return Err(ApiError::bad_request(format!(
            "{field} contains an item longer than 256 bytes"
        )));
    }
    let encoded_bytes = values
        .iter()
        .map(String::len)
        .sum::<usize>()
        .saturating_add(values.len().saturating_sub(1));
    if encoded_bytes > 12 * 1024 {
        return Err(ApiError::bad_request(format!(
            "{field} exceeds 12288 encoded bytes"
        )));
    }
    Ok(values)
}

fn validate_task_runner_provider_context(
    agent_key: SystemAgentKey,
    request: &CreateRuntimeSessionRequest,
    routes: &[ResolvedMcpRoute],
) -> Result<(), ApiError> {
    let has_route = |system_key| {
        let resource_id = chatos_mcp::system_mcp_descriptor(system_key).resource_id;
        routes.iter().any(|route| route.resource_id == resource_id)
    };
    let has_task_runner_ask_user_route = routes.iter().any(|route| {
        route.resource_id == chatos_mcp::system_mcp_descriptor(SystemMcpKey::AskUser).resource_id
            && route.provider_kind == McpProviderKind::InternalService
            && route.provider_ref.as_deref() == Some("task-runner")
    });
    let has_chatos_ask_user_route = routes.iter().any(|route| {
        route.resource_id == chatos_mcp::system_mcp_descriptor(SystemMcpKey::AskUser).resource_id
            && route.provider_kind == McpProviderKind::InternalService
            && route.provider_ref.as_deref() == Some("chatos")
    });
    if has_route(SystemMcpKey::TaskRunnerService) {
        if !is_chatos_callback_agent(agent_key) {
            return Err(ApiError::conflict(
                "Task Runner Service MCP is only valid for ChatOS task planning Agents",
            ));
        }
        for (field, value) in [
            ("source_session_id", request.source_session_id.as_deref()),
            (
                "source_user_message_id",
                request.source_user_message_id.as_deref(),
            ),
        ] {
            if value.map(str::trim).is_none_or(|value| value.is_empty()) {
                return Err(ApiError::conflict(format!(
                    "Task Runner Service MCP requires {field}"
                )));
            }
        }
    }
    if has_route(SystemMcpKey::TaskProcessLog) {
        if !is_task_runner_phase_agent(agent_key) {
            return Err(ApiError::conflict(
                "Task Process Log MCP is only valid for Task Runner phase Agents",
            ));
        }
        for (field, value) in [
            ("run_id", request.run_id.as_deref()),
            ("task_id", request.task_id.as_deref()),
        ] {
            if value.map(str::trim).is_none_or(|value| value.is_empty()) {
                return Err(ApiError::conflict(format!(
                    "Task Process Log MCP requires {field}"
                )));
            }
        }
    }
    if has_task_runner_ask_user_route {
        if !is_task_runner_phase_agent(agent_key) {
            return Err(ApiError::conflict(
                "Task Runner Ask User MCP is only valid for Task Runner phase Agents",
            ));
        }
        for (field, value) in [
            ("run_id", request.run_id.as_deref()),
            ("task_id", request.task_id.as_deref()),
        ] {
            if value.map(str::trim).is_none_or(|value| value.is_empty()) {
                return Err(ApiError::conflict(format!(
                    "Task Runner Ask User MCP requires {field}"
                )));
            }
        }
    }
    if has_chatos_ask_user_route {
        if !is_chatos_callback_agent(agent_key) {
            return Err(ApiError::conflict(
                "ChatOS Ask User MCP is only valid for ChatOS conversation Agents",
            ));
        }
        for (field, value) in [
            ("turn_id", request.turn_id.as_deref()),
            ("source_session_id", request.source_session_id.as_deref()),
            (
                "source_user_message_id",
                request.source_user_message_id.as_deref(),
            ),
        ] {
            if value.map(str::trim).is_none_or(|value| value.is_empty()) {
                return Err(ApiError::conflict(format!(
                    "ChatOS Ask User MCP requires {field}"
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "runtime_sessions/tests.rs"]
mod tests;
