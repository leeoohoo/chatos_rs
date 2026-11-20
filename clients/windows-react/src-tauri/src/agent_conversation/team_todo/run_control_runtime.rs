use super::*;

pub(in crate::agent_conversation) async fn cancel_active_team_run(
    manager: &HostManager,
    owner_user_id: &str,
    conversation_id: &str,
    turn_id: &str,
    run_id: &str,
    reason: &str,
) -> Result<LocalConversationTurnRecord, String> {
    let load_conversation = || {
        manager.execute(HostCommand::GetConversationContext(
            GetConversationCommand {
                owner_user_id: owner_user_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
            },
        ))
    };
    let result = load_conversation().await?;
    let HostResult::Conversation { conversation } = result else {
        return Err("Local Agent Host 返回了错误的团队运行响应。".into());
    };
    if conversation.conversation.owner_user_id != owner_user_id
        || conversation.conversation.conversation_id != conversation_id
    {
        return Err("Local Agent Host 返回了不匹配的团队运行。".into());
    }
    let turn = conversation
        .turns
        .iter()
        .find(|turn| turn.turn_id == turn_id && turn.run_id == run_id)
        .cloned()
        .ok_or_else(|| "Local Agent Host 中找不到待停止的团队运行。".to_owned())?;
    if turn.status != LocalConversationTurnStatus::Running {
        return Ok(turn);
    }
    let run_result = manager
        .execute(HostCommand::GetRun(GetRunCommand {
            owner_user_id: owner_user_id.to_owned(),
            run_id: run_id.to_owned(),
        }))
        .await?;
    let HostResult::Run { run } = run_result else {
        return Err("Local Agent Host 返回了错误的 Run 响应。".into());
    };
    if run.owner_user_id != owner_user_id || run.run_id != run_id {
        return Err("Local Agent Host 返回了不匹配的 Run。".into());
    }
    let cancelled = manager
        .execute_detailed(HostCommand::CancelConversationTurn(
            CancelConversationTurnCommand {
                owner_user_id: owner_user_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
                expected_conversation_version: conversation.conversation.version,
                turn_id: turn_id.to_owned(),
                expected_run_version: Some(run.version),
                reason: reason.to_owned(),
            },
        ))
        .await;
    match cancelled {
        Ok(HostResult::ConversationTurnUpdated { result }) => {
            if result.conversation.owner_user_id != owner_user_id
                || result.conversation.conversation_id != conversation_id
                || result.turn.turn_id != turn_id
                || result.turn.run_id != run_id
                || result.turn.status == LocalConversationTurnStatus::Running
            {
                return Err("Local Agent Host 返回了不匹配的停止结果。".into());
            }
            Ok(result.turn)
        }
        Ok(_) => Err("Local Agent Host 返回了错误的停止响应。".into()),
        Err(original) => {
            // The cancellation may have committed before the transport failed, or the Run may
            // have reached a terminal state concurrently. Read the authoritative Host state
            // before deciding whether this journal intent still needs replay.
            let retry = load_conversation()
                .await
                .map_err(|_| original.message.clone())?;
            let HostResult::Conversation { conversation } = retry else {
                return Err(original.message);
            };
            conversation
                .turns
                .into_iter()
                .find(|turn| {
                    turn.turn_id == turn_id
                        && turn.run_id == run_id
                        && turn.status != LocalConversationTurnStatus::Running
                })
                .ok_or(original.message)
        }
    }
}

pub(super) async fn execute_team_run_cancel_intent(
    manager: &HostManager,
    intent: &TeamRunControlIntent,
) -> Result<LocalConversationTurnRecord, String> {
    cancel_active_team_run(
        manager,
        &intent.owner_user_id,
        &intent.conversation_id,
        &intent.turn_id,
        &intent.run_id,
        "用户已停止这个团队 Todo。",
    )
    .await
}

pub(super) async fn finish_team_run_cancel_intent(
    store: AgentConversationStore,
    intent: TeamRunControlIntent,
    turn: LocalConversationTurnRecord,
) -> Result<AgentTeamTodo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        store.complete_team_run_cancel_intent(&intent, &turn)
    })
    .await
    .map_err(|_| "团队任务停止对账任务异常退出。".to_owned())?
}

pub(super) struct TeamRunRetryOutcome {
    turn: LocalConversationTurnRecord,
    run_status: LocalAgentRunStatus,
}

pub(super) async fn execute_team_run_retry_intent(
    manager: &HostManager,
    intent: &TeamRunControlIntent,
    allow_already_resumed: bool,
) -> Result<TeamRunRetryOutcome, String> {
    let load_state = || async {
        let conversation_result = manager
            .execute(HostCommand::GetConversationContext(
                GetConversationCommand {
                    owner_user_id: intent.owner_user_id.clone(),
                    conversation_id: intent.conversation_id.clone(),
                },
            ))
            .await?;
        let HostResult::Conversation { conversation } = conversation_result else {
            return Err("Local Agent Host 返回了错误的团队运行响应。".to_owned());
        };
        if conversation.conversation.owner_user_id != intent.owner_user_id
            || conversation.conversation.conversation_id != intent.conversation_id
        {
            return Err("Local Agent Host 返回了不匹配的团队运行。".to_owned());
        }
        let turn = conversation
            .turns
            .iter()
            .find(|turn| turn.turn_id == intent.turn_id && turn.run_id == intent.run_id)
            .cloned()
            .ok_or_else(|| "Local Agent Host 中找不到待重试的团队运行。".to_owned())?;
        let run_result = manager
            .execute(HostCommand::GetRun(GetRunCommand {
                owner_user_id: intent.owner_user_id.clone(),
                run_id: intent.run_id.clone(),
            }))
            .await?;
        let HostResult::Run { run } = run_result else {
            return Err("Local Agent Host 返回了错误的 Run 响应。".to_owned());
        };
        if run.owner_user_id != intent.owner_user_id || run.run_id != intent.run_id {
            return Err("Local Agent Host 返回了不匹配的 Run。".to_owned());
        }
        Ok((conversation.conversation.version, turn, run))
    };
    let (conversation_version, turn, run) = load_state().await?;
    if run.status.is_terminal() {
        let mut terminal_turn = turn;
        terminal_turn.status = match run.status {
            LocalAgentRunStatus::Succeeded => LocalConversationTurnStatus::Succeeded,
            LocalAgentRunStatus::Failed => LocalConversationTurnStatus::Failed,
            LocalAgentRunStatus::Cancelled => LocalConversationTurnStatus::Cancelled,
            _ => unreachable!(),
        };
        terminal_turn.updated_at_unix_ms =
            terminal_turn.updated_at_unix_ms.max(run.updated_at_unix_ms);
        return Ok(TeamRunRetryOutcome {
            turn: terminal_turn,
            run_status: run.status,
        });
    }
    if turn.status != LocalConversationTurnStatus::Running {
        return Ok(TeamRunRetryOutcome {
            turn,
            run_status: run.status,
        });
    }
    let retryable = matches!(
        run.status,
        LocalAgentRunStatus::WaitingUser
            | LocalAgentRunStatus::Paused
            | LocalAgentRunStatus::NeedsReview
    );
    if !retryable {
        return if allow_already_resumed {
            Ok(TeamRunRetryOutcome {
                turn,
                run_status: run.status,
            })
        } else {
            Err("这个 Host Run 当前不需要 Human 重试。".into())
        };
    }
    let resumed = manager
        .execute_detailed(HostCommand::ResumeConversationTurn(
            ResumeConversationTurnCommand {
                owner_user_id: intent.owner_user_id.clone(),
                conversation_id: intent.conversation_id.clone(),
                expected_conversation_version: conversation_version,
                turn_id: intent.turn_id.clone(),
                expected_run_version: run.version,
                expected_run_status: run.status,
                message_id: intent.id.clone(),
                message: "继续执行这个团队 Todo，并先核验中断步骤是否已经产生副作用。".into(),
                message_metadata: json!({
                    "type": "human_retry_authorization",
                    "team_id": intent.team_id,
                    "todo_id": intent.todo_id,
                    "delivery_id": intent.delivery_id,
                }),
                attachments: Vec::new(),
                reason: "Human 已在运行详情中明确授权重试中断步骤。".into(),
            },
        ))
        .await;
    match resumed {
        Ok(HostResult::ConversationTurnUpdated { result }) => {
            if result.conversation.owner_user_id != intent.owner_user_id
                || result.conversation.conversation_id != intent.conversation_id
                || result.turn.turn_id != intent.turn_id
                || result.turn.run_id != intent.run_id
                || result.run.run_id != intent.run_id
            {
                return Err("Local Agent Host 返回了不匹配的重试结果。".into());
            }
            Ok(TeamRunRetryOutcome {
                turn: result.turn,
                run_status: result.run.status,
            })
        }
        Ok(_) => Err("Local Agent Host 返回了错误的重试响应。".into()),
        Err(original) => {
            // A response can be lost after Host committed the resume transaction. Re-read the
            // authoritative Run; leaving the review state proves that the stable journaled
            // message id was accepted, so local reconciliation may safely complete.
            let (_, turn, run) = load_state().await.map_err(|_| original.message.clone())?;
            if turn.status != LocalConversationTurnStatus::Running
                || !matches!(
                    run.status,
                    LocalAgentRunStatus::WaitingUser
                        | LocalAgentRunStatus::Paused
                        | LocalAgentRunStatus::NeedsReview
                )
            {
                Ok(TeamRunRetryOutcome {
                    turn,
                    run_status: run.status,
                })
            } else {
                Err(original.message)
            }
        }
    }
}

pub(super) async fn finish_team_run_retry_intent(
    store: AgentConversationStore,
    intent: TeamRunControlIntent,
    outcome: TeamRunRetryOutcome,
) -> Result<AgentTeamTodo, String> {
    let now = chrono::Utc::now().timestamp_millis().max(0);
    tauri::async_runtime::spawn_blocking(move || {
        store.complete_team_run_retry_intent(&intent, &outcome.turn, outcome.run_status, now)
    })
    .await
    .map_err(|_| "团队任务重试对账任务异常退出。".to_owned())?
}
