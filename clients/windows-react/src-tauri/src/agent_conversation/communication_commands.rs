use super::*;

pub(crate) async fn reconcile_agent_communication_runs(
    owner_user_id: String,
    room_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
    manager: State<'_, HostManager>,
) -> Result<(), String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&room_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let binding_store = conversations.inner().clone();
    let binding_owner = owner_user_id.clone();
    let binding_room = room_id.clone();
    let bindings = tauri::async_runtime::spawn_blocking(move || {
        binding_store.active_agent_communication_run_bindings(&binding_owner, &binding_room)
    })
    .await
    .map_err(|_| "Agent 通讯运行绑定读取任务异常退出。".to_owned())??;
    let mut reconciliations = Vec::with_capacity(bindings.len());
    let mut orphaned_deliveries = Vec::new();
    for binding in bindings {
        let result = manager
            .execute_detailed(HostCommand::GetConversationContext(
                GetConversationCommand {
                    owner_user_id: owner_user_id.clone(),
                    conversation_id: binding.conversation_id.clone(),
                },
            ))
            .await;
        let detail = match result {
            Ok(HostResult::Conversation { conversation }) => conversation,
            Ok(_) => return Err("Local Agent Host 返回了错误的 Agent 通讯运行响应。".into()),
            Err(error) if error.code.as_deref() == Some("not_found") => {
                orphaned_deliveries.push(binding.delivery_id);
                continue;
            }
            Err(error) => return Err(error.message),
        };
        validate_host_conversation(&detail, &owner_user_id, &binding.conversation_id)?;
        let Some(turn) = detail
            .turns
            .iter()
            .find(|turn| turn.turn_id == binding.turn_id && turn.run_id == binding.run_id)
        else {
            orphaned_deliveries.push(binding.delivery_id);
            continue;
        };
        reconciliations.push(AgentCommunicationRunReconciliation {
            delivery_id: binding.delivery_id,
            turn_id: turn.turn_id.clone(),
            run_id: turn.run_id.clone(),
            status: turn.status,
            created_at_unix_ms: turn.created_at_unix_ms,
            updated_at_unix_ms: turn.updated_at_unix_ms,
        });
    }
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    if !reconciliations.is_empty() {
        let reconcile_store = conversations.inner().clone();
        let reconcile_owner = owner_user_id.clone();
        tauri::async_runtime::spawn_blocking(move || {
            reconcile_store.reconcile_agent_communication_runs(
                &reconcile_owner,
                &room_id,
                &reconciliations,
            )
        })
        .await
        .map_err(|_| "Agent 通讯运行对账任务异常退出。".to_owned())??;
    }
    for delivery_id in orphaned_deliveries {
        let failure_store = conversations.inner().clone();
        let failure_owner = owner_user_id.clone();
        tauri::async_runtime::spawn_blocking(move || {
            failure_store.fail_project_team_delivery(
                &failure_owner,
                &delivery_id,
                "Agent 通讯 Delivery 的 Host Turn 不存在，恢复时已终止该运行。",
            )
        })
        .await
        .map_err(|_| "Agent 通讯孤立 Delivery 清理任务异常退出。".to_owned())??;
    }
    Ok(())
}

pub(crate) async fn dispatch_next_agent_communication_delivery(
    owner_user_id: String,
    room_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
    skills: State<'_, AgentSkillStore>,
    general_settings: State<'_, GeneralSettingsStore>,
    manager: State<'_, HostManager>,
    attachment_worker: State<'_, AttachmentToolWorker>,
    terminal_worker: State<'_, TerminalToolWorker>,
) -> Result<bool, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&room_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let pending_owner = owner_user_id.clone();
    let pending_room = room_id.clone();
    let Some(pending) = tauri::async_runtime::spawn_blocking(move || {
        store.next_agent_communication_delivery(&pending_owner, &pending_room)
    })
    .await
    .map_err(|_| "Agent 通讯 Delivery 读取任务异常退出。".to_owned())??
    else {
        return Ok(false);
    };
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let skill_store = skills.inner().clone();
    let settings_store = general_settings.inner().clone();
    let skill_owner = owner_user_id.clone();
    let profession_key = pending.binding.profession_key.clone();
    let profession = tauri::async_runtime::spawn_blocking(move || {
        let language = settings_store.context_language(&skill_owner)?;
        skill_store.profession_runtime_snapshot(&skill_owner, &profession_key, &language)
    })
    .await
    .map_err(|_| "Agent Skill 快照任务异常退出。".to_owned())??;
    let conversation_id =
        agent_communication_conversation_id(&pending.binding.room_id, &pending.binding.agent_id);
    let title = format!("{} · {}", pending.room_name, pending.binding.name);
    ensure_agent_host_conversation(
        &manager,
        &owner_user_id,
        &conversation_id,
        &title,
        LocalConversationResourceBinding {
            kind: LocalConversationResourceKind::Contact,
            resource_id: pending.binding.agent_id.clone(),
        },
    )
    .await?;
    let detail = agent_conversation_context(&manager, &owner_user_id, &conversation_id).await?;
    let model =
        selected_agent_model(&manager, &owner_user_id, &pending.binding.model_config_id).await?;
    let settings = ensure_agent_runtime_settings(
        &manager,
        &owner_user_id,
        &conversation_id,
        &model,
        pending.binding.thinking_level.as_deref(),
    )
    .await?;
    let capability_revision =
        latest_capability_revision(&manager, &owner_user_id, MAIN_CHAT_PROFILE).await?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let turn_id = identifier("turn");
    let run_id = identifier("run");
    let host_message_id = identifier("message");
    let command = StartConversationTurnCommand {
        owner_user_id: owner_user_id.clone(),
        conversation_id,
        conversation_if_missing: None,
        expected_conversation_version: detail.conversation.version,
        turn_id: turn_id.clone(),
        message_id: host_message_id.clone(),
        run_id: run_id.clone(),
        message: pending.content.clone(),
        message_metadata: agent_communication_execution_metadata(&pending, &profession),
        attachments: vec![],
        model_config_ref: settings.selected_model_config_ref,
        model_config_revision: settings.selected_model_config_revision,
        capability_policy_revision: capability_revision,
        max_iterations: MAX_ITERATIONS,
    };
    command
        .validate()
        .map_err(|_| "Agent 通讯消息或执行快照超过安全限制。".to_owned())?;
    let claim_store = conversations.inner().clone();
    let claim_owner = owner_user_id.clone();
    let claim_pending = pending.clone();
    let claim_turn = turn_id.clone();
    let claim_run = run_id.clone();
    let now = chrono::Utc::now().timestamp_millis().max(0);
    tauri::async_runtime::spawn_blocking(move || {
        claim_store.claim_agent_communication_delivery(
            &claim_owner,
            &claim_pending,
            &claim_turn,
            &claim_run,
            now,
        )
    })
    .await
    .map_err(|_| "Agent 通讯 Delivery 领取任务异常退出。".to_owned())??;
    let started = manager
        .execute(HostCommand::StartConversationTurn(command))
        .await;
    let result = match started {
        Ok(HostResult::ConversationTurnStarted { result }) => result,
        Ok(_) => {
            fail_dispatched_team_delivery(
                conversations.inner().clone(),
                owner_user_id.clone(),
                pending.delivery_id.clone(),
                "Local Agent Host 返回了错误的 Agent 通讯 Delivery 响应。".into(),
            )
            .await;
            return Err("Local Agent Host 返回了错误的 Agent 通讯 Delivery 响应。".into());
        }
        Err(failure) => {
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
        let failure = "Local Agent Host 返回了不匹配的 Agent 通讯 Delivery。".to_owned();
        fail_dispatched_team_delivery(
            conversations.inner().clone(),
            owner_user_id.clone(),
            pending.delivery_id.clone(),
            failure.clone(),
        )
        .await;
        return Err(failure);
    }
    attachment_worker.wake();
    terminal_worker.wake();
    Ok(true)
}
