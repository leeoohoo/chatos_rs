use super::*;

pub(super) async fn agent_conversation_context(
    manager: &HostManager,
    owner_user_id: &str,
    conversation_id: &str,
) -> Result<LocalConversationDetail, String> {
    let result = manager
        .execute(HostCommand::GetConversationContext(
            GetConversationCommand {
                owner_user_id: owner_user_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
            },
        ))
        .await?;
    let HostResult::Conversation { conversation } = result else {
        return Err("Local Agent Host 返回了错误的 Agent 会话响应。".into());
    };
    validate_host_conversation(&conversation, owner_user_id, conversation_id)?;
    Ok(conversation)
}

pub(super) async fn ensure_agent_host_conversation(
    manager: &HostManager,
    owner_user_id: &str,
    conversation_id: &str,
    title: &str,
    resource: LocalConversationResourceBinding,
) -> Result<(), String> {
    let existing = manager
        .execute_detailed(HostCommand::GetConversation(GetConversationCommand {
            owner_user_id: owner_user_id.to_owned(),
            conversation_id: conversation_id.to_owned(),
        }))
        .await;
    match existing {
        Ok(HostResult::Conversation { conversation }) => {
            validate_host_conversation(&conversation, owner_user_id, conversation_id)?;
            if conversation.conversation.resource.as_ref() != Some(&resource) {
                if conversation.conversation.resource.is_some() {
                    return Err("Local Agent Host 返回了不匹配的 Agent 资源绑定。".into());
                }
                let migrated = manager
                    .execute(HostCommand::CreateConversation(CreateConversationCommand {
                        conversation_id: conversation_id.to_owned(),
                        owner_user_id: owner_user_id.to_owned(),
                        title: title.to_owned(),
                        resource: Some(resource.clone()),
                    }))
                    .await?;
                let HostResult::Conversation { conversation } = migrated else {
                    return Err("Local Agent Host 返回了错误的 Agent 会话响应。".into());
                };
                validate_host_conversation(&conversation, owner_user_id, conversation_id)?;
                if conversation.conversation.resource.as_ref() != Some(&resource) {
                    return Err("Local Agent Host 返回了不匹配的 Agent 资源绑定。".into());
                }
            }
            Ok(())
        }
        Ok(_) => Err("Local Agent Host 返回了错误的 Agent 会话响应。".into()),
        Err(error) if error.code.as_deref() == Some("not_found") => {
            let created = manager
                .execute(HostCommand::CreateConversation(CreateConversationCommand {
                    conversation_id: conversation_id.to_owned(),
                    owner_user_id: owner_user_id.to_owned(),
                    title: title.to_owned(),
                    resource: Some(resource.clone()),
                }))
                .await;
            match created {
                Ok(HostResult::Conversation { conversation }) => {
                    validate_host_conversation(&conversation, owner_user_id, conversation_id)?;
                    if conversation.conversation.resource.as_ref() != Some(&resource) {
                        return Err("Local Agent Host 返回了不匹配的 Agent 资源绑定。".into());
                    }
                    Ok(())
                }
                Ok(_) => Err("Local Agent Host 返回了错误的 Agent 会话响应。".into()),
                Err(create_error) => {
                    let retry = agent_conversation_context(manager, owner_user_id, conversation_id)
                        .await
                        .map_err(|_| create_error)?;
                    validate_host_conversation(&retry, owner_user_id, conversation_id)?;
                    if retry.conversation.resource.as_ref() != Some(&resource) {
                        return Err("Local Agent Host 返回了不匹配的 Agent 资源绑定。".into());
                    }
                    Ok(())
                }
            }
        }
        Err(error) => Err(error.message),
    }
}

pub(super) async fn ensure_team_executor_host_conversation(
    manager: &HostManager,
    owner_user_id: &str,
    conversation_id: &str,
    title: &str,
) -> Result<LocalConversationDetail, String> {
    let existing = manager
        .execute_detailed(HostCommand::GetConversationContext(
            GetConversationCommand {
                owner_user_id: owner_user_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
            },
        ))
        .await;
    match existing {
        Ok(HostResult::Conversation { conversation }) => {
            validate_team_executor_host_conversation(
                &conversation,
                owner_user_id,
                conversation_id,
            )?;
            Ok(conversation)
        }
        Ok(_) => Err("Local Agent Host 返回了错误的 executor 会话响应。".into()),
        Err(error) if error.code.as_deref() == Some("not_found") => {
            let created = manager
                .execute(HostCommand::CreateConversation(CreateConversationCommand {
                    conversation_id: conversation_id.to_owned(),
                    owner_user_id: owner_user_id.to_owned(),
                    title: title.to_owned(),
                    resource: None,
                }))
                .await;
            match created {
                Ok(HostResult::Conversation { conversation }) => {
                    validate_team_executor_host_conversation(
                        &conversation,
                        owner_user_id,
                        conversation_id,
                    )?;
                    Ok(conversation)
                }
                Ok(_) => Err("Local Agent Host 返回了错误的 executor 会话响应。".into()),
                Err(create_error) => {
                    let retry = manager
                        .execute(HostCommand::GetConversationContext(
                            GetConversationCommand {
                                owner_user_id: owner_user_id.to_owned(),
                                conversation_id: conversation_id.to_owned(),
                            },
                        ))
                        .await
                        .map_err(|_| create_error)?;
                    let HostResult::Conversation { conversation } = retry else {
                        return Err("Local Agent Host 返回了错误的 executor 会话响应。".into());
                    };
                    validate_team_executor_host_conversation(
                        &conversation,
                        owner_user_id,
                        conversation_id,
                    )?;
                    Ok(conversation)
                }
            }
        }
        Err(error) => Err(error.message),
    }
}

pub(super) async fn send_agent_guidance(
    manager: &HostManager,
    owner_user_id: &str,
    conversation_id: &str,
    expected_conversation_version: u64,
    turn_id: &str,
    content: &str,
    attachments: Vec<LocalConversationAttachmentSpec>,
) -> Result<AgentConversationSendAck, String> {
    let message_id = identifier("message");
    let result = manager
        .execute(HostCommand::GuideConversationTurn(
            GuideConversationTurnCommand {
                owner_user_id: owner_user_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
                expected_conversation_version,
                turn_id: turn_id.to_owned(),
                expected_run_version: None,
                message_id: message_id.clone(),
                message: content.to_owned(),
                message_metadata: json!({"source": "native_agent_direct_guidance"}),
                attachments,
            },
        ))
        .await?;
    let HostResult::ConversationTurnUpdated { result } = result else {
        return Err("Local Agent Host 返回了错误的 Agent 引导响应。".into());
    };
    Ok(AgentConversationSendAck {
        delivery_kind: "guidance",
        turn_id: result.turn.turn_id.clone(),
        user_message_id: result
            .message
            .as_ref()
            .map(|message| message.message_id.clone())
            .unwrap_or(message_id),
    })
}

pub(super) async fn selected_agent_model(
    manager: &HostManager,
    owner_user_id: &str,
    model_config_id: &str,
) -> Result<LocalModelConfigSnapshot, String> {
    let result = manager
        .execute(HostCommand::ListLatestModelConfigSnapshots(
            ListLatestModelConfigSnapshotsCommand {
                owner_user_id: owner_user_id.to_owned(),
            },
        ))
        .await?;
    let HostResult::ModelConfigSnapshots { snapshots } = result else {
        return Err("Local Agent Host 返回了错误的模型配置响应。".into());
    };
    snapshots
        .into_iter()
        .find(|snapshot| {
            snapshot.owner_user_id == owner_user_id && snapshot.model_config_ref == model_config_id
        })
        .ok_or_else(|| "该 Agent 绑定的模型当前不可用，请编辑 Agent 后重试。".to_owned())
}

pub(super) async fn ensure_agent_runtime_settings(
    manager: &HostManager,
    owner_user_id: &str,
    conversation_id: &str,
    model: &LocalModelConfigSnapshot,
    thinking_level: Option<&str>,
) -> Result<LocalConversationRuntimeSettings, String> {
    let current = manager
        .execute_detailed(HostCommand::GetConversationRuntimeSettings(
            GetConversationRuntimeSettingsCommand {
                owner_user_id: owner_user_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
            },
        ))
        .await;
    let expected_version = match current {
        Ok(HostResult::ConversationRuntimeSettings { settings }) => {
            let matches = settings.selected_model_config_ref == model.model_config_ref
                && settings.selected_model_config_revision == model.model_config_revision
                && settings.selected_thinking_level.as_deref() == thinking_level
                && settings.remote_connection_id.is_none()
                && settings.reasoning_enabled
                    == thinking_level.is_some_and(|level| level != "none");
            if matches {
                return Ok(settings);
            }
            let version = settings.version;
            Some(version)
        }
        Ok(_) => return Err("Local Agent Host 返回了错误的运行配置响应。".into()),
        Err(error) if error.code.as_deref() == Some("not_found") => None,
        Err(error) => return Err(error.message),
    };
    let result = manager
        .execute(HostCommand::PutConversationRuntimeSettings(
            PutConversationRuntimeSettingsCommand {
                owner_user_id: owner_user_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
                selected_model_config_ref: model.model_config_ref.clone(),
                selected_model_config_revision: model.model_config_revision.clone(),
                selected_thinking_level: thinking_level.map(str::to_owned),
                remote_connection_id: None,
                reasoning_enabled: thinking_level.is_some_and(|level| level != "none"),
                expected_version,
            },
        ))
        .await?;
    let HostResult::ConversationRuntimeSettings { settings } = result else {
        return Err("Local Agent Host 返回了错误的运行配置响应。".into());
    };
    Ok(settings)
}

pub(super) async fn latest_capability_revision(
    manager: &HostManager,
    owner_user_id: &str,
    profile_key: &str,
) -> Result<String, String> {
    let result = manager
        .execute(HostCommand::GetLatestCapabilityPolicySnapshot(
            GetLatestCapabilityPolicySnapshotCommand {
                owner_user_id: owner_user_id.to_owned(),
                profile_key: profile_key.to_owned(),
            },
        ))
        .await?;
    let HostResult::CapabilityPolicySnapshot { snapshot } = result else {
        return Err("Local Agent Host 返回了错误的能力配置响应。".into());
    };
    if snapshot.owner_user_id != owner_user_id || snapshot.profile_key != profile_key {
        return Err("Local Agent Host 返回了不匹配的能力配置。".into());
    }
    Ok(snapshot.capability_policy_revision)
}

pub(super) fn agent_execution_metadata(
    binding: &AgentExecutionBinding,
    profession: &AgentProfessionRuntimeSnapshot,
) -> serde_json::Value {
    json!({
        "source": "native_agent_direct",
        "agent_execution_context": {
            "schema_version": 1,
            "room_id": binding.room_id,
            "agent_id": binding.agent_id,
            "agent_name": binding.name,
            "role_prompt": binding.role_prompt,
            "profession_key": profession.key,
            "profession_label": profession.label,
            "profession_skill_ref": profession.skill_ref,
            "profession_skill_sha256": profession.instructions_sha256,
            "profession_skill_markdown": profession.markdown,
            "default_plugin_ids": binding.default_plugin_ids,
            "default_skill_ids": binding.default_skill_ids,
        }
    })
}

pub(super) fn team_agent_execution_metadata(
    pending: &PendingTeamDelivery,
    profession: &AgentProfessionRuntimeSnapshot,
) -> Result<serde_json::Value, String> {
    let binding = &pending.binding;
    let mut metadata = json!({
        "source": "native_agent_team",
        "agent_execution_context": {
            "schema_version": 2,
            "room_id": binding.agent.room_id,
            "agent_id": binding.agent.agent_id,
            "agent_name": binding.agent.name,
            "role_prompt": binding.agent.role_prompt,
            "profession_key": profession.key,
            "profession_label": profession.label,
            "profession_skill_ref": profession.skill_ref,
            "profession_skill_sha256": profession.instructions_sha256,
            "profession_skill_markdown": profession.markdown,
            "default_plugin_ids": binding.agent.default_plugin_ids,
            "default_skill_ids": binding.agent.default_skill_ids,
            "project_id": binding.project_id,
            "team_name": binding.team_name,
            "team_goal": binding.team_goal,
            "team_role": binding.team_role,
            "team_responsibility": binding.team_responsibility,
            "is_project_manager": binding.is_project_manager,
        }
    });
    if pending.trigger_kind != "todo" && binding.is_project_manager {
        metadata["agent_project_management"] = json!({
            "schema_version": 1,
            "instructions": "你已被明确指定为当前团队的项目经理。请先调用 project_dashboard_get 读取系统事实，再用 project_dashboard_update 建立或更新结构化项目总览、里程碑、风险、下一步和需要 Human 处理的事项。普通成员阻塞由项目经理先行调查、拆单、重派和协调；只有权限、预算、凭据、产品方向决策或外部动作确实只能由 Human 完成时，才登记 Human issue。进度和健康判断必须基于真实任务、Run、调研或 Human 消息，不得用总结文字冒充已验收交付。",
        });
    }
    if pending.trigger_kind == "todo" {
        let todo_id = pending.todo_id.as_deref().ok_or_else(team_store_error)?;
        let plan = pending
            .execution_plan
            .as_ref()
            .ok_or_else(team_store_error)?;
        metadata["agent_team_execution"] = json!({
            "schema_version": 1,
            "delivery_lane": "executor",
            "trigger_kind": "todo",
            "delivery_id": pending.delivery_id,
            "todo_id": todo_id,
            "team_id": binding.agent.room_id,
            "agent_id": binding.agent.agent_id,
            "project_id": binding.project_id,
            "execution_plan": {
                "requires_execution": plan.requires_execution,
                "builtin_capabilities": plan.builtin_capabilities,
                "plugins": plan.plugins,
            }
        });
    }
    Ok(metadata)
}

pub(super) fn agent_communication_execution_metadata(
    pending: &PendingAgentCommunicationDelivery,
    profession: &AgentProfessionRuntimeSnapshot,
) -> serde_json::Value {
    json!({
        "source": "native_agent_communication",
        "agent_execution_context": {
            "schema_version": 3,
            "conversation_kind": pending.conversation_kind,
            "room_id": pending.binding.room_id,
            "agent_id": pending.binding.agent_id,
            "agent_name": pending.binding.name,
            "role_prompt": pending.binding.role_prompt,
            "profession_key": profession.key,
            "profession_label": profession.label,
            "profession_skill_ref": profession.skill_ref,
            "profession_skill_sha256": profession.instructions_sha256,
            "profession_skill_markdown": profession.markdown,
            "default_plugin_ids": pending.binding.default_plugin_ids,
            "default_skill_ids": pending.binding.default_skill_ids,
            "delivery_id": pending.delivery_id,
            "trigger_kind": pending.trigger_kind,
        }
    })
}
