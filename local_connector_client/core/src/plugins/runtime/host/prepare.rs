// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl PluginRuntimeHost {
    pub(super) async fn prepare(&self, request: &RelayRequest) -> Result<Value, (u16, String)> {
        let run_id = required_body_text(&request.body, "run_id")?;
        let owner_user_id =
            required_envelope_text(request.owner_user_id.as_deref(), "owner_user_id")?;
        let device_id = required_envelope_text(request.device_id.as_deref(), "device_id")?;
        let plugin_id = required_body_text(&request.body, "plugin_id")?;
        if self
            .disabled_plugins
            .lock()
            .map_err(|_| {
                (
                    500,
                    "Plugin disabled-state store is unavailable".to_string(),
                )
            })?
            .contains(plugin_id.as_str())
        {
            return Err((409, "Plugin is disabled by the user".to_string()));
        }
        let release_id = required_body_text(&request.body, "release_id")?;
        let artifact_sha256 = required_sha256(&request.body, "artifact_sha256")?;
        let component_key = required_body_text(&request.body, "component_key")?;
        let permission_snapshot =
            optional_body_text_set(&request.body, "permission_snapshot", 256)?;
        let component_kind = self
            .skill_loader
            .active_component_kind(plugin_id.as_str(), component_key.as_str())
            .map_err(|error| (409, error.to_string()))?;
        let catalog_only = optional_body_bool(&request.body, "catalog_only")?.unwrap_or(false);
        if catalog_only
            && !matches!(
                component_kind,
                PluginComponentKind::Command | PluginComponentKind::Agent
            )
        {
            return Err((
                400,
                "catalog_only is supported only for Plugin Command and Agent components"
                    .to_string(),
            ));
        }
        let adapter_session_id = Uuid::new_v4().to_string();
        let (skills, agents, commands, hooks, ui, mcp, version, operations) = match component_kind {
            PluginComponentKind::SkillCollection => {
                let skill_keys = required_body_text_array(&request.body, "skill_keys", 64)?;
                let available = self
                    .skill_loader
                    .load_component(plugin_id.as_str(), component_key.as_str())
                    .map_err(|error| (409, error.to_string()))?;
                let mut by_key = available
                    .into_iter()
                    .map(|skill| (skill.skill_key.clone(), skill))
                    .collect::<BTreeMap<_, _>>();
                let mut selected = BTreeMap::new();
                for skill_key in skill_keys {
                    let skill = by_key.remove(skill_key.as_str()).ok_or_else(|| {
                        (
                            404,
                            format!(
                                "Plugin Skill is not available in the selected component: {skill_key}"
                            ),
                        )
                    })?;
                    validate_prepared_release(
                        skill.release_id.as_str(),
                        skill.artifact_sha256.as_str(),
                        release_id.as_str(),
                        artifact_sha256.as_str(),
                    )?;
                    selected.insert(skill_key, skill);
                }
                let version = selected
                    .values()
                    .next()
                    .context("Plugin prepare selected no Skills")
                    .map_err(internal_error)?
                    .version
                    .clone();
                let content_sha256 = optional_body_text(&request.body, "content_sha256")?;
                self.skill_loader
                    .validate_component_content_snapshot(
                        plugin_id.as_str(),
                        component_key.as_str(),
                        content_sha256.as_deref(),
                    )
                    .map_err(|error| (409, error.to_string()))?;
                (
                    selected,
                    BTreeMap::new(),
                    BTreeMap::new(),
                    BTreeMap::new(),
                    None,
                    None,
                    version,
                    vec![LOAD_SKILL_RESOURCE_OPERATION],
                )
            }
            PluginComponentKind::McpServer => {
                if permission_snapshot.contains("workspace.write") {
                    return Err((
                        409,
                        "Plugin MCP workspace.write is unavailable until per-call workspace approval is enforced"
                            .to_string(),
                    ));
                }
                let workspace_root = if permission_snapshot.contains("workspace.read") {
                    let workspace_id = request.workspace_id.trim();
                    if workspace_id.is_empty() {
                        return Err((
                            409,
                            "Plugin MCP workspace.read requires a bound Local Connector workspace"
                                .to_string(),
                        ));
                    }
                    let state = self.state_snapshot().await?;
                    let workspace = state.workspace_by_id(workspace_id).ok_or_else(|| {
                        (
                            409,
                            "Plugin MCP workspace is not registered locally".to_string(),
                        )
                    })?;
                    Some(approved_workspace_root(workspace.absolute_root.as_path())?)
                } else {
                    None
                };
                let server_key = optional_body_text(&request.body, "server_key")?;
                let tool_allowlist = optional_body_text_set(&request.body, "tool_allowlist", 200)?;
                let tool_blocklist = optional_body_text_set(&request.body, "tool_blocklist", 200)?;
                let mcp = self
                    .mcp_adapter
                    .prepare(
                        plugin_id.as_str(),
                        component_key.as_str(),
                        server_key.as_deref(),
                        adapter_session_id.as_str(),
                        owner_user_id.as_str(),
                        device_id.as_str(),
                        workspace_root.as_deref(),
                        &permission_snapshot,
                        &tool_allowlist,
                        &tool_blocklist,
                    )
                    .await
                    .map_err(|error| (409, error.to_string()))?;
                validate_prepared_release(
                    mcp.snapshot().release_id.as_str(),
                    mcp.snapshot().artifact_sha256.as_str(),
                    release_id.as_str(),
                    artifact_sha256.as_str(),
                )?;
                let version = mcp.snapshot().version.clone();
                let operation = mcp.operation();
                let health_operation = mcp.health_operation();
                (
                    BTreeMap::new(),
                    BTreeMap::new(),
                    BTreeMap::new(),
                    BTreeMap::new(),
                    None,
                    Some(mcp),
                    version,
                    vec![operation, health_operation],
                )
            }
            PluginComponentKind::Command => {
                let content_sha256 = required_sha256(&request.body, "content_sha256")?;
                let arguments = optional_body_text(&request.body, "arguments")?;
                let mut command = self
                    .command_loader
                    .load(
                        plugin_id.as_str(),
                        component_key.as_str(),
                        content_sha256.as_str(),
                        &permission_snapshot,
                        arguments.as_deref(),
                    )
                    .map_err(|error| (409, error.to_string()))?;
                validate_prepared_release(
                    command.release_id.as_str(),
                    command.artifact_sha256.as_str(),
                    release_id.as_str(),
                    artifact_sha256.as_str(),
                )?;
                if command.requires_confirmation && !catalog_only {
                    let state = self.state_snapshot().await?;
                    let mut approval_args = vec![plugin_id.clone(), component_key.clone()];
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
                            session_id: Some(adapter_session_id.clone()),
                            action_audit: Some(ApprovalActionAudit {
                                kind: "plugin_command".to_string(),
                                operation: component_key.clone(),
                                details: vec![
                                    ApprovalActionAuditDetail {
                                        key: "plugin_id".to_string(),
                                        value: plugin_id.clone(),
                                    },
                                    ApprovalActionAuditDetail {
                                        key: "command_id".to_string(),
                                        value: component_key.clone(),
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
                                    "Approval authorizes only this exact signed Plugin Command snapshot for the current Run."
                                        .to_string(),
                                ),
                                recovery: Some(
                                    "Deny the request to prevent this Command prompt from entering the Run."
                                        .to_string(),
                                ),
                            }),
                        })
                        .await
                        .map_err(internal_error)?;
                    if let ApprovalDecision::Denied { reason, .. } = approval {
                        return Err((403, format!("Plugin Command was not approved: {reason}")));
                    }
                    let reloaded = self
                        .command_loader
                        .load(
                            plugin_id.as_str(),
                            component_key.as_str(),
                            content_sha256.as_str(),
                            &permission_snapshot,
                            arguments.as_deref(),
                        )
                        .map_err(|error| (409, error.to_string()))?;
                    validate_prepared_release(
                        reloaded.release_id.as_str(),
                        reloaded.artifact_sha256.as_str(),
                        release_id.as_str(),
                        artifact_sha256.as_str(),
                    )?;
                    if reloaded != command {
                        return Err((
                            409,
                            "Plugin Command snapshot changed while awaiting approval".to_string(),
                        ));
                    }
                    command.confirmation_approved = true;
                }
                let version = command.version.clone();
                (
                    BTreeMap::new(),
                    BTreeMap::new(),
                    BTreeMap::from([(component_key.clone(), command)]),
                    BTreeMap::new(),
                    None,
                    None,
                    version,
                    if catalog_only {
                        vec![COMMAND_INVOKE_OPERATION]
                    } else {
                        Vec::new()
                    },
                )
            }
            PluginComponentKind::Agent => {
                let content_sha256 = required_sha256(&request.body, "content_sha256")?;
                let agent = self
                    .agent_loader
                    .load(
                        plugin_id.as_str(),
                        component_key.as_str(),
                        content_sha256.as_str(),
                        &permission_snapshot,
                    )
                    .map_err(|error| (409, error.to_string()))?;
                validate_prepared_release(
                    agent.release_id.as_str(),
                    agent.artifact_sha256.as_str(),
                    release_id.as_str(),
                    artifact_sha256.as_str(),
                )?;
                let version = agent.version.clone();
                (
                    BTreeMap::new(),
                    BTreeMap::from([(component_key.clone(), agent)]),
                    BTreeMap::new(),
                    BTreeMap::new(),
                    None,
                    None,
                    version,
                    if catalog_only {
                        vec![AGENT_APPLY_OPERATION]
                    } else {
                        Vec::new()
                    },
                )
            }
            PluginComponentKind::HookSet => {
                let content_sha256 = required_sha256(&request.body, "content_sha256")?;
                let hook_set = self
                    .hook_loader
                    .load(
                        plugin_id.as_str(),
                        component_key.as_str(),
                        content_sha256.as_str(),
                        &permission_snapshot,
                    )
                    .map_err(|error| (409, error.to_string()))?;
                validate_prepared_release(
                    hook_set.release_id.as_str(),
                    hook_set.artifact_sha256.as_str(),
                    release_id.as_str(),
                    artifact_sha256.as_str(),
                )?;
                let version = hook_set.version.clone();
                (
                    BTreeMap::new(),
                    BTreeMap::new(),
                    BTreeMap::new(),
                    BTreeMap::from([(component_key.clone(), hook_set)]),
                    None,
                    None,
                    version,
                    vec![self.hook_loader.operation()],
                )
            }
            PluginComponentKind::UiContribution => {
                let content_sha256 = required_sha256(&request.body, "content_sha256")?;
                let ui = self
                    .ui_loader
                    .load(
                        plugin_id.as_str(),
                        component_key.as_str(),
                        content_sha256.as_str(),
                        &permission_snapshot,
                    )
                    .map_err(|error| (409, error.to_string()))?;
                validate_prepared_release(
                    ui.release_id.as_str(),
                    ui.artifact_sha256.as_str(),
                    release_id.as_str(),
                    artifact_sha256.as_str(),
                )?;
                let version = ui.version.clone();
                (
                    BTreeMap::new(),
                    BTreeMap::new(),
                    BTreeMap::new(),
                    BTreeMap::new(),
                    Some(ui),
                    None,
                    version,
                    Vec::new(),
                )
            }
            _ => {
                return Err((
                    409,
                    "Plugin component runtime is not implemented by this Host".to_string(),
                ));
            }
        };
        let expires_at = Utc::now().timestamp() + PLUGIN_SESSION_TTL_SECONDS;
        let session = PreparedPluginSession {
            run_id: run_id.clone(),
            owner_user_id,
            device_id,
            workspace_id: request.workspace_id.trim().to_string(),
            plugin_id: plugin_id.clone(),
            release_id: release_id.clone(),
            version: version.clone(),
            artifact_sha256: artifact_sha256.clone(),
            component_key: component_key.clone(),
            permission_snapshot: permission_snapshot.clone(),
            skills: skills.clone(),
            agents: agents.clone(),
            commands: commands.clone(),
            hooks: hooks.clone(),
            ui: ui.clone(),
            native_action_lock: Arc::new(AsyncMutex::new(())),
            hook_dispatch_lock: Arc::new(AsyncMutex::new(())),
            native_action_cancelled: Arc::new(AtomicBool::new(false)),
            mcp,
            expires_at,
        };
        if let Some(ui) = session.ui.clone() {
            self.artifact_store
                .register_ui_grant(PluginUiArtifactGrant {
                    owner_user_id: session.owner_user_id.clone(),
                    device_id: session.device_id.clone(),
                    workspace_id: session.workspace_id.clone(),
                    run_id: session.run_id.clone(),
                    plugin_id: session.plugin_id.clone(),
                    release_id: session.release_id.clone(),
                    artifact_sha256: session.artifact_sha256.clone(),
                    component_key: session.component_key.clone(),
                    adapter_session_id: adapter_session_id.clone(),
                    ui,
                    permission_snapshot: session.permission_snapshot.clone(),
                    expires_at,
                })
                .map_err(internal_error)?;
        }
        let mcp_snapshot = session.mcp.as_ref().map(|mcp| mcp.snapshot().clone());
        let mcp_health = session
            .mcp
            .as_ref()
            .map(PreparedPluginMcp::health_snapshot)
            .transpose()
            .map_err(internal_error)?;
        let session_sha256 = session_audit_hash(&session);
        self.prune_expired_sessions();
        let mut sessions = self.sessions()?;
        sessions.insert(adapter_session_id.clone(), session);
        let skills = skills.into_values().collect::<Vec<_>>();
        let agents = agents.into_values().collect::<Vec<_>>();
        let commands = commands.into_values().collect::<Vec<_>>();
        let hooks = hooks.into_values().collect::<Vec<_>>();
        let ui = ui.into_iter().collect::<Vec<_>>();
        Ok(json!({
            "run_id": run_id,
            "plugin_id": plugin_id,
            "release_id": release_id,
            "version": version,
            "artifact_sha256": artifact_sha256,
            "component_key": component_key,
            "skills": skills,
            "agents": agents,
            "commands": commands,
            "hooks": hooks,
            "ui": ui,
            "mcp": mcp_snapshot,
            "mcp_health": mcp_health,
            "operations": operations,
            "permission_snapshot": permission_snapshot,
            "adapter_session_id": adapter_session_id,
            "session_sha256": session_sha256,
            "expires_at": expires_at,
        }))
    }
}
