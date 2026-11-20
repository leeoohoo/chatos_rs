use super::*;

#[tauri::command]
pub(crate) async fn queue_agent_team_message(
    input: QueueAgentTeamMessageInput,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
    attachment_manager: State<'_, AttachmentManager>,
) -> Result<QueueAgentTeamMessageResult, String> {
    validate_owner(&input.owner_user_id)?;
    validate_identifier(&input.team_id).map_err(|_| invalid_team_input())?;
    let content = input.content.trim().to_owned();
    validate_text(&content, 1_000_000, true).map_err(|_| invalid_team_message())?;
    if content.is_empty() && input.attachment_ids.is_empty() {
        return Err("消息或附件不能为空。".into());
    }
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = input.owner_user_id.clone();
    let team = input.team_id.clone();
    let mentions = input.mentioned_agent_ids.clone();
    let attachment_scope = AttachmentScope::new(
        &input.owner_user_id,
        &agent_team_attachment_conversation_id(&input.team_id),
    );
    let attachment_ids = input.attachment_ids.clone();
    let attachments =
        attachment_manager.verified_draft_payloads(attachment_scope.clone(), &attachment_ids)?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        store.queue_project_team_message_with_attachments(
            &owner,
            &team,
            &content,
            &mentions,
            &attachments,
        )
    })
    .await
    .map_err(|_| "Agent 团队消息入队任务异常退出。".to_owned())??;
    attachment_manager.discard_ids(attachment_scope, attachment_ids);
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    Ok(result)
}

#[tauri::command]
pub(crate) async fn dispatch_next_agent_team_delivery(
    owner_user_id: String,
    team_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
    skills: State<'_, AgentSkillStore>,
    general_settings: State<'_, GeneralSettingsStore>,
    manager: State<'_, HostManager>,
    attachment_manager: State<'_, AttachmentManager>,
    attachment_worker: State<'_, AttachmentToolWorker>,
    terminal_worker: State<'_, TerminalToolWorker>,
) -> Result<Option<DispatchedAgentTeamDelivery>, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&team_id).map_err(|_| invalid_team_input())?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let enqueue_store = conversations.inner().clone();
    let enqueue_owner = owner_user_id.clone();
    let enqueue_team = team_id.clone();
    let enqueue_now = chrono::Utc::now().timestamp_millis().max(0);
    tauri::async_runtime::spawn_blocking(move || {
        enqueue_store.enqueue_ready_project_team_todos(&enqueue_owner, &enqueue_team, enqueue_now)
    })
    .await
    .map_err(|_| "团队 Todo 调度任务异常退出。".to_owned())??;
    let store = conversations.inner().clone();
    let pending_owner = owner_user_id.clone();
    let pending_team = team_id.clone();
    let Some(pending) = tauri::async_runtime::spawn_blocking(move || {
        store.next_project_team_delivery(&pending_owner, &pending_team)
    })
    .await
    .map_err(|_| "Agent Delivery 读取任务异常退出。".to_owned())??
    else {
        return Ok(None);
    };
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let skill_store = skills.inner().clone();
    let settings_store = general_settings.inner().clone();
    let skill_owner = owner_user_id.clone();
    let profession_key = pending.binding.agent.profession_key.clone();
    let profession = tauri::async_runtime::spawn_blocking(move || {
        let language = settings_store.context_language(&skill_owner)?;
        skill_store.profession_runtime_snapshot(&skill_owner, &profession_key, &language)
    })
    .await
    .map_err(|_| "Agent Skill 快照任务异常退出。".to_owned())??;
    let conversation_id = if pending.trigger_kind == "todo" {
        format!("agent-team-executor-{}", pending.delivery_id)
    } else {
        agent_team_conversation_id(
            &pending.binding.agent.room_id,
            &pending.binding.agent.agent_id,
        )
    };
    let title = format!(
        "{} · {}",
        pending.binding.team_name, pending.binding.agent.name
    );
    let detail = if pending.trigger_kind == "todo" {
        ensure_team_executor_host_conversation(&manager, &owner_user_id, &conversation_id, &title)
            .await?
    } else {
        ensure_agent_host_conversation(
            &manager,
            &owner_user_id,
            &conversation_id,
            &title,
            LocalConversationResourceBinding {
                kind: LocalConversationResourceKind::Contact,
                resource_id: pending.binding.agent.agent_id.clone(),
            },
        )
        .await?;
        agent_conversation_context(&manager, &owner_user_id, &conversation_id).await?
    };
    let model = selected_agent_model(
        &manager,
        &owner_user_id,
        &pending.binding.agent.model_config_id,
    )
    .await?;
    let settings = ensure_agent_runtime_settings(
        &manager,
        &owner_user_id,
        &conversation_id,
        &model,
        pending.binding.agent.thinking_level.as_deref(),
    )
    .await?;
    let profile_key = if pending.trigger_kind == "todo" {
        "task_execution"
    } else {
        MAIN_CHAT_PROFILE
    };
    let capability_revision =
        latest_capability_revision(&manager, &owner_user_id, profile_key).await?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let turn_id = identifier("turn");
    let run_id = identifier("run");
    let host_message_id = identifier("message");
    let attachment_store = conversations.inner().clone();
    let attachment_owner = owner_user_id.clone();
    let attachment_message = pending.message_id.clone();
    let payloads = tauri::async_runtime::spawn_blocking(move || {
        attachment_store.load_message_attachment_payloads(&attachment_owner, &attachment_message)
    })
    .await
    .map_err(|_| "Agent 团队附件读取任务异常退出。".to_owned())??;
    let attachment_scope = AttachmentScope::new(&owner_user_id, &conversation_id);
    let (delivery_attachment_ids, delivery_attachments) =
        attachment_manager.authorize_verified_payloads(attachment_scope.clone(), &payloads)?;
    let command = StartConversationTurnCommand {
        owner_user_id: owner_user_id.clone(),
        conversation_id: conversation_id.clone(),
        conversation_if_missing: None,
        expected_conversation_version: detail.conversation.version,
        turn_id: turn_id.clone(),
        message_id: host_message_id.clone(),
        run_id: run_id.clone(),
        message: pending.content.clone(),
        message_metadata: team_agent_execution_metadata(&pending, &profession)?,
        attachments: delivery_attachments,
        model_config_ref: settings.selected_model_config_ref,
        model_config_revision: settings.selected_model_config_revision,
        capability_policy_revision: capability_revision,
        max_iterations: MAX_ITERATIONS,
    };
    if command.validate().is_err() {
        attachment_manager.discard_ids(attachment_scope, delivery_attachment_ids);
        return Err("Agent 团队消息或执行快照超过安全限制。".into());
    }
    let claim_store = conversations.inner().clone();
    let claim_owner = owner_user_id.clone();
    let claim_pending = pending.clone();
    let claim_turn = turn_id.clone();
    let claim_run = run_id.clone();
    let now = chrono::Utc::now().timestamp_millis().max(0);
    if let Err(error) = tauri::async_runtime::spawn_blocking(move || {
        claim_store.claim_project_team_delivery(
            &claim_owner,
            &claim_pending,
            &claim_turn,
            &claim_run,
            now,
        )
    })
    .await
    .map_err(|_| "Agent Delivery 领取任务异常退出。".to_owned())?
    {
        attachment_manager.discard_ids(attachment_scope, delivery_attachment_ids);
        return Err(error);
    }
    let started = manager
        .execute(HostCommand::StartConversationTurn(command))
        .await;
    let result = match started {
        Ok(HostResult::ConversationTurnStarted { result }) => result,
        Ok(_) => {
            attachment_manager
                .discard_ids(attachment_scope.clone(), delivery_attachment_ids.clone());
            fail_dispatched_team_delivery(
                conversations.inner().clone(),
                owner_user_id.clone(),
                pending.delivery_id.clone(),
                "Local Agent Host 返回了错误的团队 Delivery 响应。".into(),
            )
            .await;
            return Err("Local Agent Host 返回了错误的团队 Delivery 响应。".into());
        }
        Err(failure) => {
            attachment_manager
                .discard_ids(attachment_scope.clone(), delivery_attachment_ids.clone());
            fail_dispatched_team_delivery(
                conversations.inner().clone(),
                owner_user_id.clone(),
                pending.delivery_id.clone(),
                failure.clone(),
            )
            .await;
            return Err(failure);
        }
    };
    if result.turn.turn_id != turn_id
        || result.turn.run_id != run_id
        || result.turn.user_message_id != host_message_id
        || result.message.message_id != host_message_id
        || result.run.run_id != run_id
    {
        attachment_manager.discard_ids(attachment_scope.clone(), delivery_attachment_ids.clone());
        let failure = "Local Agent Host 返回了不匹配的团队 Delivery。".to_owned();
        fail_dispatched_team_delivery(
            conversations.inner().clone(),
            owner_user_id.clone(),
            pending.delivery_id.clone(),
            failure.clone(),
        )
        .await;
        return Err(failure);
    }
    attachment_manager.commit(attachment_scope, &delivery_attachment_ids)?;
    attachment_worker.wake();
    terminal_worker.wake();
    Ok(Some(DispatchedAgentTeamDelivery {
        delivery_id: pending.delivery_id,
        team_id: pending.binding.agent.room_id,
        agent_id: pending.binding.agent.agent_id,
        turn_id,
        run_id,
    }))
}

pub(super) async fn fail_dispatched_team_delivery(
    store: AgentConversationStore,
    owner_user_id: String,
    delivery_id: String,
    failure: String,
) {
    let _ = tauri::async_runtime::spawn_blocking(move || {
        store.fail_project_team_delivery(&owner_user_id, &delivery_id, &failure)
    })
    .await;
}
