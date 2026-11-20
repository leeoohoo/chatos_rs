use super::*;

#[tauri::command]
pub(crate) async fn send_agent_conversation_draft(
    input: SendAgentConversationDraftInput,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
    skills: State<'_, AgentSkillStore>,
    general_settings: State<'_, GeneralSettingsStore>,
    manager: State<'_, HostManager>,
    attachment_manager: State<'_, AttachmentManager>,
    attachment_worker: State<'_, AttachmentToolWorker>,
    terminal_worker: State<'_, TerminalToolWorker>,
) -> Result<AgentConversationSendAck, String> {
    validate_owner(&input.owner_user_id)?;
    validate_identifier(&input.room_id)?;
    let content = input.submission.content.trim().to_owned();
    if content.is_empty() && input.submission.attachment_ids.is_empty() {
        return Err("消息或附件不能为空。".into());
    }
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = input.owner_user_id.clone();
    let room = input.room_id.clone();
    let binding =
        tauri::async_runtime::spawn_blocking(move || store.load_execution_binding(&owner, &room))
            .await
            .map_err(|_| "Agent 执行身份读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    let skill_store = skills.inner().clone();
    let settings_store = general_settings.inner().clone();
    let skill_owner = input.owner_user_id.clone();
    let profession_key = binding.profession_key.clone();
    let profession = tauri::async_runtime::spawn_blocking(move || {
        let language = settings_store.context_language(&skill_owner)?;
        skill_store.profession_runtime_snapshot(&skill_owner, &profession_key, &language)
    })
    .await
    .map_err(|_| "Agent Skill 快照任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;

    let conversation_id = agent_direct_conversation_id(&binding.room_id);
    let attachment_scope = AttachmentScope::new(&input.owner_user_id, &conversation_id);
    let attachment_ids = input.submission.attachment_ids.clone();
    let attachments =
        attachment_manager.resolve_specs(attachment_scope.clone(), &attachment_ids)?;
    let detail =
        agent_conversation_context(&manager, &input.owner_user_id, &conversation_id).await?;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;

    if let Some(turn_id) = input.guidance_turn_id.as_deref() {
        if detail.turns.iter().any(|turn| {
            turn.turn_id == turn_id && turn.status == LocalConversationTurnStatus::Running
        }) {
            let ack = send_agent_guidance(
                &manager,
                &input.owner_user_id,
                &conversation_id,
                detail.conversation.version,
                turn_id,
                &content,
                attachments,
            )
            .await?;
            authentication
                .require_session_generation(&input.owner_user_id, generation)
                .await?;
            attachment_manager.commit(attachment_scope, &attachment_ids)?;
            attachment_worker.wake();
            terminal_worker.wake();
            return Ok(ack);
        }
    }

    let model =
        selected_agent_model(&manager, &input.owner_user_id, &binding.model_config_id).await?;
    let settings = ensure_agent_runtime_settings(
        &manager,
        &input.owner_user_id,
        &conversation_id,
        &model,
        binding.thinking_level.as_deref(),
    )
    .await?;
    let capability_revision =
        latest_capability_revision(&manager, &input.owner_user_id, MAIN_CHAT_PROFILE).await?;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    let turn_id = identifier("turn");
    let message_id = identifier("message");
    let run_id = identifier("run");
    let command = StartConversationTurnCommand {
        owner_user_id: input.owner_user_id.clone(),
        conversation_id,
        conversation_if_missing: None,
        expected_conversation_version: detail.conversation.version,
        turn_id: turn_id.clone(),
        message_id: message_id.clone(),
        run_id: run_id.clone(),
        message: content.clone(),
        message_metadata: agent_execution_metadata(&binding, &profession),
        attachments,
        model_config_ref: settings.selected_model_config_ref,
        model_config_revision: settings.selected_model_config_revision,
        capability_policy_revision: capability_revision,
        max_iterations: MAX_ITERATIONS,
    };
    command
        .validate()
        .map_err(|_| "Agent 私聊消息或执行快照超过安全限制。".to_owned())?;
    let delivery_store = conversations.inner().clone();
    let delivery_owner = input.owner_user_id.clone();
    let delivery_binding = binding.clone();
    let delivery_content = content.clone();
    let stored_delivery_id = uuid::Uuid::new_v4().to_string().to_lowercase();
    let stored_message_id = message_id.clone();
    let stored_turn_id = turn_id.clone();
    let stored_run_id = run_id.clone();
    let now_unix_ms = chrono::Utc::now().timestamp_millis().max(0);
    tauri::async_runtime::spawn_blocking(move || {
        delivery_store.record_direct_delivery(
            &delivery_owner,
            &delivery_binding,
            &stored_delivery_id,
            &stored_message_id,
            &stored_turn_id,
            &stored_run_id,
            &delivery_content,
            now_unix_ms,
        )
    })
    .await
    .map_err(|_| "Agent Delivery 写入任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    let result = manager
        .execute(HostCommand::StartConversationTurn(command))
        .await?;
    let HostResult::ConversationTurnStarted { result } = result else {
        return Err("Local Agent Host 返回了错误的 Agent 发送响应。".into());
    };
    if result.turn.turn_id != turn_id
        || result.turn.run_id != run_id
        || result.turn.user_message_id != message_id
        || result.message.message_id != message_id
        || result.message.turn_id != turn_id
        || result.run.run_id != run_id
    {
        return Err("Local Agent Host 返回了不匹配的 Agent Delivery。".into());
    }
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    attachment_manager.commit(attachment_scope, &attachment_ids)?;
    attachment_worker.wake();
    terminal_worker.wake();
    Ok(AgentConversationSendAck {
        delivery_kind: "newTurn",
        turn_id: result.turn.turn_id.clone(),
        user_message_id: result.message.message_id.clone(),
    })
}
