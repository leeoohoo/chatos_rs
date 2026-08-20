// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl PluginRuntimeHost {
    pub(super) async fn execute_prepared(
        &self,
        request: &RelayRequest,
        adapter_session_id: &str,
        operation: &str,
        session: &PreparedPluginSession,
    ) -> Result<Value, (u16, String)> {
        match operation {
            COMMAND_INVOKE_OPERATION => {
                let base = session
                    .commands
                    .get(session.component_key.as_str())
                    .ok_or_else(|| {
                        (
                            403,
                            "Plugin Command operation was not published during prepare".to_string(),
                        )
                    })?;
                let arguments = optional_body_text(&request.body, "arguments")?;
                let _invoke_guard = session.native_action_lock.lock().await;
                self.load_exact_session(request, adapter_session_id)?;
                let mut command = self
                    .command_loader
                    .load(
                        session.plugin_id.as_str(),
                        session.component_key.as_str(),
                        base.content_sha256.as_str(),
                        &session.permission_snapshot,
                        arguments.as_deref(),
                    )
                    .map_err(|error| (409, error.to_string()))?;
                validate_prepared_release(
                    command.release_id.as_str(),
                    command.artifact_sha256.as_str(),
                    session.release_id.as_str(),
                    session.artifact_sha256.as_str(),
                )?;
                validate_command_invocation_snapshot(base, &command)?;
                if command.requires_confirmation {
                    let state = self.state_snapshot().await?;
                    let mut approval_args =
                        vec![session.plugin_id.clone(), session.component_key.clone()];
                    if let Some(arguments) = command.arguments.as_ref() {
                        approval_args.push(arguments.clone());
                    }
                    let approval = self
                        .approval_service()?
                        .approve_interactive(CommandApprovalRequest {
                            request_id: request.request_id.clone(),
                            project_key: approval_project_key_for_relay_scope(&state, request),
                            command: "plugin-command".to_string(),
                            args: approval_args,
                            redact_arguments_in_history: true,
                            cwd: ".".to_string(),
                            source: "plugin_command".to_string(),
                            requested_permissions: None,
                            session_id: Some(adapter_session_id.to_string()),
                            action_audit: Some(ApprovalActionAudit {
                                kind: "plugin_command".to_string(),
                                operation: session.component_key.clone(),
                                details: vec![
                                    ApprovalActionAuditDetail {
                                        key: "plugin_id".to_string(),
                                        value: session.plugin_id.clone(),
                                    },
                                    ApprovalActionAuditDetail {
                                        key: "command_id".to_string(),
                                        value: session.component_key.clone(),
                                    },
                                    ApprovalActionAuditDetail {
                                        key: "arguments_sha256".to_string(),
                                        value: command.arguments_sha256.clone(),
                                    },
                                ],
                                privacy: Some(
                                    "Command arguments are visible only in the pending local approval and are redacted from approval history."
                                        .to_string(),
                                ),
                                safety: Some(
                                    "Approval authorizes only this exact signed Plugin Command snapshot for the current MCP invocation."
                                        .to_string(),
                                ),
                                recovery: Some(
                                    "Deny the request to prevent this Command prompt from entering the Agent tool result."
                                        .to_string(),
                                ),
                            }),
                        })
                        .await
                        .map_err(internal_error)?;
                    if let ApprovalDecision::Denied { reason, .. } = approval {
                        return Err((403, format!("Plugin Command was not approved: {reason}")));
                    }
                    self.load_exact_session(request, adapter_session_id)?;
                    let reloaded = self
                        .command_loader
                        .load(
                            session.plugin_id.as_str(),
                            session.component_key.as_str(),
                            base.content_sha256.as_str(),
                            &session.permission_snapshot,
                            arguments.as_deref(),
                        )
                        .map_err(|error| (409, error.to_string()))?;
                    validate_prepared_release(
                        reloaded.release_id.as_str(),
                        reloaded.artifact_sha256.as_str(),
                        session.release_id.as_str(),
                        session.artifact_sha256.as_str(),
                    )?;
                    validate_command_invocation_snapshot(base, &reloaded)?;
                    if reloaded != command {
                        return Err((
                            409,
                            "Plugin Command snapshot changed while awaiting approval".to_string(),
                        ));
                    }
                    command.confirmation_approved = true;
                }
                Ok(json!({
                    "plugin_id": session.plugin_id,
                    "release_id": session.release_id,
                    "version": session.version,
                    "artifact_sha256": session.artifact_sha256,
                    "component_key": session.component_key,
                    "adapter_session_id": adapter_session_id,
                    "operation": operation,
                    "result": {"command": command},
                }))
            }
            AGENT_APPLY_OPERATION => {
                let base = session
                    .agents
                    .get(session.component_key.as_str())
                    .ok_or_else(|| {
                        (
                            403,
                            "Plugin Agent operation was not published during prepare".to_string(),
                        )
                    })?;
                if request
                    .body
                    .get("arguments")
                    .is_some_and(|arguments| !arguments.is_null() && arguments != &json!({}))
                {
                    return Err((
                        400,
                        "Plugin Agent apply does not accept arguments".to_string(),
                    ));
                }
                let agent = self
                    .agent_loader
                    .load(
                        session.plugin_id.as_str(),
                        session.component_key.as_str(),
                        base.content_sha256.as_str(),
                        &session.permission_snapshot,
                    )
                    .map_err(|error| (409, error.to_string()))?;
                validate_prepared_release(
                    agent.release_id.as_str(),
                    agent.artifact_sha256.as_str(),
                    session.release_id.as_str(),
                    session.artifact_sha256.as_str(),
                )?;
                if &agent != base {
                    return Err((
                        409,
                        "Plugin Agent snapshot changed after prepare".to_string(),
                    ));
                }
                Ok(json!({
                    "plugin_id": session.plugin_id,
                    "release_id": session.release_id,
                    "version": session.version,
                    "artifact_sha256": session.artifact_sha256,
                    "component_key": session.component_key,
                    "adapter_session_id": adapter_session_id,
                    "operation": operation,
                    "result": {"agent": agent},
                }))
            }
            operation if operation == self.hook_loader.operation() => {
                let event = request
                    .body
                    .get("event")
                    .cloned()
                    .ok_or_else(|| (400, "Plugin Hook event is required".to_string()))
                    .and_then(|value| {
                        serde_json::from_value(value).map_err(|error| {
                            (400, format!("Plugin Hook event is invalid: {error}"))
                        })
                    })?;
                let context = request
                    .body
                    .get("context")
                    .cloned()
                    .map(serde_json::from_value)
                    .transpose()
                    .map_err(|error| {
                        (
                            400,
                            format!("Plugin Hook event context is invalid: {error}"),
                        )
                    })?
                    .unwrap_or_default();
                let hook_set = session
                    .hooks
                    .get(session.component_key.as_str())
                    .ok_or_else(|| {
                        (
                            403,
                            "Plugin Hook operation was not published during prepare".to_string(),
                        )
                    })?;
                let workspace_write_decisions = self
                    .approve_hook_workspace_writes(
                        request,
                        adapter_session_id,
                        session,
                        hook_set,
                        event,
                        &context,
                    )
                    .await?;
                let result = self
                    .hook_loader
                    .dispatch(
                        hook_set,
                        &session.permission_snapshot,
                        session.run_id.as_str(),
                        event,
                        &context,
                        &workspace_write_decisions,
                    )
                    .await
                    .map_err(|error| (409, error.to_string()))?;
                Ok(json!({
                    "plugin_id": session.plugin_id,
                    "release_id": session.release_id,
                    "version": session.version,
                    "artifact_sha256": session.artifact_sha256,
                    "component_key": session.component_key,
                    "adapter_session_id": adapter_session_id,
                    "operation": operation,
                    "result": result,
                }))
            }
            LOAD_SKILL_RESOURCE_OPERATION => {
                let skill_key = required_body_text(&request.body, "skill_key")?;
                let relative_path = required_body_text(&request.body, "relative_path")?;
                let skill = session.skills.get(skill_key.as_str()).ok_or_else(|| {
                    (
                        403,
                        "Plugin Skill was not selected during prepare".to_string(),
                    )
                })?;
                let resource = skill
                    .resources
                    .iter()
                    .find(|resource| resource.relative_path == relative_path)
                    .ok_or_else(|| {
                        (
                            403,
                            "Plugin Skill resource was not published during prepare".to_string(),
                        )
                    })?;
                let content = self
                    .skill_loader
                    .load_text_resource(skill, relative_path.as_str())
                    .map_err(|error| (409, error.to_string()))?;
                Ok(json!({
                    "plugin_id": session.plugin_id,
                    "release_id": session.release_id,
                    "version": session.version,
                    "artifact_sha256": session.artifact_sha256,
                    "component_key": session.component_key,
                    "skill_key": skill_key,
                    "relative_path": relative_path,
                    "content_sha256": resource.sha256,
                    "content": content,
                    "adapter_session_id": adapter_session_id,
                    "operation": operation,
                }))
            }
            operation
                if session
                    .mcp
                    .as_ref()
                    .is_some_and(|mcp| mcp.health_operation() == operation) =>
            {
                let mcp = session
                    .mcp
                    .as_ref()
                    .context("prepared Plugin MCP session is unavailable")
                    .map_err(internal_error)?;
                mcp.validate_active()
                    .map_err(|error| (409, error.to_string()))?;
                let health = mcp.check_health().await.map_err(internal_error)?;
                Ok(json!({
                    "plugin_id": session.plugin_id,
                    "release_id": session.release_id,
                    "version": session.version,
                    "artifact_sha256": session.artifact_sha256,
                    "component_key": session.component_key,
                    "mcp_health": health,
                    "adapter_session_id": adapter_session_id,
                    "operation": operation,
                }))
            }
            operation
                if session
                    .mcp
                    .as_ref()
                    .is_some_and(|mcp| mcp.operation() == operation) =>
            {
                let tool_name = required_body_text(&request.body, "tool_name")?;
                let invocation_id = required_body_text(&request.body, "invocation_id")?;
                let arguments = request
                    .body
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                if !arguments.is_object() {
                    return Err((
                        400,
                        "Plugin MCP tool arguments must be an object".to_string(),
                    ));
                }
                let mcp = session
                    .mcp
                    .as_ref()
                    .context("prepared Plugin MCP session is unavailable")
                    .map_err(internal_error)?;
                if !mcp.publishes_tool(tool_name.as_str()) {
                    return Err((
                        403,
                        format!("Plugin MCP tool was not published during prepare: {tool_name}"),
                    ));
                }
                mcp.validate_active()
                    .map_err(|error| (409, error.to_string()))?;
                let result = mcp
                    .call_tool(
                        invocation_id.as_str(),
                        tool_name.as_str(),
                        arguments,
                        request
                            .body
                            .get("tool_result_max_chars")
                            .and_then(Value::as_u64)
                            .and_then(|value| usize::try_from(value).ok()),
                    )
                    .await
                    .map_err(|error| (502, error.to_string()))?;
                let health = mcp.health_snapshot().map_err(internal_error)?;
                Ok(json!({
                    "plugin_id": session.plugin_id,
                    "release_id": session.release_id,
                    "version": session.version,
                    "artifact_sha256": session.artifact_sha256,
                    "component_key": session.component_key,
                    "tool_name": tool_name,
                    "invocation_id": invocation_id,
                    "result": result,
                    "mcp_health": health,
                    "adapter_session_id": adapter_session_id,
                    "operation": operation,
                }))
            }
            _ => Err((
                403,
                format!("Plugin operation was not published during prepare: {operation}"),
            )),
        }
    }
}

fn validate_command_invocation_snapshot(
    base: &PluginCommandSnapshot,
    invoked: &PluginCommandSnapshot,
) -> Result<(), (u16, String)> {
    if base.plugin_id != invoked.plugin_id
        || base.release_id != invoked.release_id
        || base.version != invoked.version
        || base.artifact_sha256 != invoked.artifact_sha256
        || base.component_key != invoked.component_key
        || base.command_name != invoked.command_name
        || base.relative_source_path != invoked.relative_source_path
        || base.description != invoked.description
        || base.argument_hint != invoked.argument_hint
        || base.requires_confirmation != invoked.requires_confirmation
        || base.target_agent != invoked.target_agent
        || base.allowed_tools != invoked.allowed_tools
        || base.content_sha256 != invoked.content_sha256
        || base.prompt != invoked.prompt
    {
        return Err((
            409,
            "Plugin Command immutable snapshot changed after prepare".to_string(),
        ));
    }
    Ok(())
}
