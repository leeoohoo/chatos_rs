use super::*;

#[tauri::command]
pub(crate) async fn prepare_agent_conversation(
    owner_user_id: String,
    agent_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
    manager: State<'_, HostManager>,
) -> Result<PreparedAgentConversation, String> {
    validate_owner(&owner_user_id)?;
    validate_agent_identifier(&agent_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = owner_user_id.clone();
    let requested_agent = agent_id.clone();
    let room = tauri::async_runtime::spawn_blocking(move || {
        store.open_human_agent_direct(&owner, &requested_agent)
    })
    .await
    .map_err(|_| "Agent 私聊准备任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;

    // Bind the Host transcript to the exact Agent Contact.  The account-scoped Agent room above
    // remains the authority for the rest of the execution profile, while the Host resource
    // binding lets memory history resolve the same Agent thread without an unbound conversation.
    let conversation_id = agent_direct_conversation_id(&room.id);
    ensure_agent_host_conversation(
        &manager,
        &owner_user_id,
        &conversation_id,
        &room.name,
        LocalConversationResourceBinding {
            kind: LocalConversationResourceKind::Contact,
            resource_id: room.agent_id.clone(),
        },
    )
    .await?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(PreparedAgentConversation {
        room_id: room.id,
        agent_id: room.agent_id,
        conversation_id,
        title: room.name,
        created_at_unix_ms: room.created_at_unix_ms,
        updated_at_unix_ms: room.updated_at_unix_ms,
    })
}

#[tauri::command]
pub(crate) async fn load_agent_conversation_activity(
    owner_user_id: String,
    room_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
    manager: State<'_, HostManager>,
) -> Result<AgentConversationActivitySnapshot, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&room_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let binding_owner = owner_user_id.clone();
    let binding_room = room_id.clone();
    let binding = tauri::async_runtime::spawn_blocking(move || {
        store.load_execution_binding(&binding_owner, &binding_room)
    })
    .await
    .map_err(|_| "Agent 执行身份读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let conversation_id = agent_direct_conversation_id(&binding.room_id);
    let detail = agent_conversation_context(&manager, &owner_user_id, &conversation_id).await?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let conversation_version = detail.conversation.version;
    let activity_store = conversations.inner().clone();
    let activity_owner = owner_user_id.clone();
    let activity_binding = binding.clone();
    let items = tauri::async_runtime::spawn_blocking(move || {
        activity_store.reconcile_direct_activity(&activity_owner, &activity_binding, &detail)
    })
    .await
    .map_err(|_| "Agent 运行动态读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(AgentConversationActivitySnapshot {
        room_id: binding.room_id,
        agent_id: binding.agent_id,
        conversation_id,
        conversation_version,
        has_active_delivery: items
            .iter()
            .any(|item| matches!(item.delivery_status.as_str(), "pending" | "running")),
        items,
    })
}

#[tauri::command]
pub(crate) async fn load_agent_team_member_activity(
    owner_user_id: String,
    team_id: String,
    agent_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
    manager: State<'_, HostManager>,
) -> Result<AgentConversationActivitySnapshot, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&team_id).map_err(|_| invalid_team_input())?;
    validate_agent_identifier(&agent_id).map_err(|_| invalid_team_input())?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let binding_owner = owner_user_id.clone();
    let binding_team = team_id.clone();
    let binding_agent = agent_id.clone();
    let binding = tauri::async_runtime::spawn_blocking(move || {
        store.load_project_team_execution_binding(&binding_owner, &binding_team, &binding_agent)
    })
    .await
    .map_err(|_| "团队 Agent 执行身份读取任务异常退出。".to_owned())??;
    let conversation_id =
        agent_team_conversation_id(&binding.agent.room_id, &binding.agent.agent_id);
    let host_result = manager
        .execute_detailed(HostCommand::GetConversationContext(
            GetConversationCommand {
                owner_user_id: owner_user_id.clone(),
                conversation_id: conversation_id.clone(),
            },
        ))
        .await;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let activity_store = conversations.inner().clone();
    let activity_owner = owner_user_id.clone();
    let activity_binding = binding.agent.clone();
    let (conversation_version, items) = match host_result {
        Ok(HostResult::Conversation { conversation }) => {
            validate_host_conversation(&conversation, &owner_user_id, &conversation_id)?;
            let version = conversation.conversation.version;
            let items = tauri::async_runtime::spawn_blocking(move || {
                activity_store.reconcile_direct_activity(
                    &activity_owner,
                    &activity_binding,
                    &conversation,
                )
            })
            .await
            .map_err(|_| "团队 Agent 运行动态读取任务异常退出。".to_owned())??;
            (version, items)
        }
        Err(error) if error.code.as_deref() == Some("not_found") => {
            let items = tauri::async_runtime::spawn_blocking(move || {
                activity_store.list_direct_activity(&activity_owner, &activity_binding)
            })
            .await
            .map_err(|_| "团队 Agent 运行动态读取任务异常退出。".to_owned())??;
            (0, items)
        }
        Ok(_) => return Err("Local Agent Host 返回了错误的团队 Agent 会话响应。".into()),
        Err(error) => return Err(error.message),
    };
    Ok(AgentConversationActivitySnapshot {
        room_id: binding.agent.room_id,
        agent_id: binding.agent.agent_id,
        conversation_id,
        conversation_version,
        has_active_delivery: items
            .iter()
            .any(|item| matches!(item.delivery_status.as_str(), "pending" | "running")),
        items,
    })
}

#[tauri::command]
pub(crate) async fn load_agent_profile_activity(
    owner_user_id: String,
    agent_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
    manager: State<'_, HostManager>,
) -> Result<AgentProfileActivitySnapshot, String> {
    validate_owner(&owner_user_id)?;
    validate_agent_identifier(&agent_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let room_store = conversations.inner().clone();
    let room_owner = owner_user_id.clone();
    let room_agent = agent_id.clone();
    let room_ids = tauri::async_runtime::spawn_blocking(move || {
        room_store.activity_room_ids(&room_owner, &room_agent)
    })
    .await
    .map_err(|_| "Agent 运行 Room 查询任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;

    let mut items = Vec::new();
    for room_id in room_ids {
        let binding_store = conversations.inner().clone();
        let binding_owner = owner_user_id.clone();
        let binding_room = room_id.clone();
        let binding = tauri::async_runtime::spawn_blocking(move || {
            binding_store.load_execution_binding(&binding_owner, &binding_room)
        })
        .await
        .map_err(|_| "Agent 执行身份读取任务异常退出。".to_owned())??;
        if binding.agent_id != agent_id {
            return Err("Agent 运行动态成员身份不一致。".into());
        }
        let conversation_id = agent_direct_conversation_id(&binding.room_id);
        let result = manager
            .execute_detailed(HostCommand::GetConversationContext(
                GetConversationCommand {
                    owner_user_id: owner_user_id.clone(),
                    conversation_id: conversation_id.clone(),
                },
            ))
            .await;
        let detail = match result {
            Ok(HostResult::Conversation { conversation }) => {
                validate_host_conversation(&conversation, &owner_user_id, &conversation_id)?;
                conversation
            }
            Err(error) if error.code.as_deref() == Some("not_found") => continue,
            Ok(_) => return Err("Local Agent Host 返回了错误的 Agent 会话响应。".into()),
            Err(error) => return Err(error.message),
        };
        authentication
            .require_session_generation(&owner_user_id, generation)
            .await?;
        let activity_store = conversations.inner().clone();
        let activity_owner = owner_user_id.clone();
        let room_items = tauri::async_runtime::spawn_blocking(move || {
            activity_store.reconcile_direct_activity(&activity_owner, &binding, &detail)
        })
        .await
        .map_err(|_| "Agent 运行动态读取任务异常退出。".to_owned())??;
        items.extend(room_items);
    }
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    items.sort_by(|left, right| {
        right
            .updated_at_unix_ms
            .cmp(&left.updated_at_unix_ms)
            .then_with(|| right.run_id.cmp(&left.run_id))
    });
    items.truncate(100);
    Ok(AgentProfileActivitySnapshot {
        agent_id,
        has_active_delivery: items
            .iter()
            .any(|item| matches!(item.delivery_status.as_str(), "pending" | "running")),
        items,
    })
}
