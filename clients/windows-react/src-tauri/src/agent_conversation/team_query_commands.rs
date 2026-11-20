use super::*;

#[tauri::command]
pub(crate) async fn list_agent_teams(
    owner_user_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<Vec<AgentTeamSummary>, String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = owner_user_id.clone();
    let teams = tauri::async_runtime::spawn_blocking(move || store.list_project_teams(&owner))
        .await
        .map_err(|_| "Agent 团队读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(teams)
}

#[tauri::command]
pub(crate) async fn create_agent_team(
    app: AppHandle,
    input: CreateAgentTeamInput,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<AgentTeamSummary, String> {
    validate_owner(&input.owner_user_id)?;
    validate_identifier(&input.project_id).map_err(|_| invalid_team_input())?;
    validate_text(&input.name, 160, false).map_err(|_| invalid_team_input())?;
    validate_text(&input.goal, 8_000, true).map_err(|_| invalid_team_input())?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "项目注册表不可用。".to_owned())?;
    let project_database = data_dir.join("NativeConnector").join("Projects.sqlite3");
    let store = conversations.inner().clone();
    let owner = input.owner_user_id.clone();
    let project_id = input.project_id.clone();
    let name = input.name.clone();
    let goal = input.goal.clone();
    let team = tauri::async_runtime::spawn_blocking(move || {
        require_active_project(&project_database, &owner, &project_id)?;
        store.create_project_team(&owner, &project_id, &name, &goal)
    })
    .await
    .map_err(|_| "Agent 团队创建任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    Ok(team)
}

#[tauri::command]
pub(crate) async fn load_agent_team(
    owner_user_id: String,
    team_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<AgentTeamDetail, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&team_id).map_err(|_| invalid_team_input())?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = owner_user_id.clone();
    let selected_team = team_id.clone();
    let detail = tauri::async_runtime::spawn_blocking(move || {
        store.load_project_team(&owner, &selected_team)
    })
    .await
    .map_err(|_| "Agent 团队成员读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(detail)
}

#[tauri::command]
pub(crate) async fn load_agent_team_conversation(
    owner_user_id: String,
    team_id: String,
    before_message_id: Option<String>,
    limit: Option<u32>,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<AgentTeamConversationSnapshot, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&team_id).map_err(|_| invalid_team_input())?;
    if let Some(message_id) = before_message_id.as_deref() {
        validate_identifier(message_id).map_err(|_| invalid_team_input())?;
    }
    let limit = limit.unwrap_or(50).clamp(1, 200);
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = owner_user_id.clone();
    let team = team_id.clone();
    let before = before_message_id.clone();
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        store.load_project_team_conversation(&owner, &team, before.as_deref(), limit)
    })
    .await
    .map_err(|_| "Agent 团队消息读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(snapshot)
}

#[tauri::command]
pub(crate) async fn load_agent_team_runs(
    owner_user_id: String,
    team_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
    manager: State<'_, HostManager>,
    terminal_worker: State<'_, TerminalToolWorker>,
) -> Result<AgentTeamRunHistorySnapshot, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&team_id).map_err(|_| invalid_team_input())?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let binding_store = conversations.inner().clone();
    let binding_owner = owner_user_id.clone();
    let binding_team = team_id.clone();
    let bindings = tauri::async_runtime::spawn_blocking(move || {
        binding_store.active_project_team_run_bindings(&binding_owner, &binding_team)
    })
    .await
    .map_err(|_| "Agent 团队运行绑定读取任务异常退出。".to_owned())??;
    let mut reconciliations = Vec::with_capacity(bindings.len());
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
            Ok(_) => return Err("Local Agent Host 返回了错误的团队运行响应。".into()),
            Err(error) if error.code.as_deref() == Some("not_found") => continue,
            Err(error) => return Err(error.message),
        };
        validate_host_conversation(&detail, &owner_user_id, &binding.conversation_id)?;
        let Some(turn) = detail
            .turns
            .iter()
            .find(|turn| turn.turn_id == binding.turn_id && turn.run_id == binding.run_id)
        else {
            continue;
        };
        reconciliations.push(team_todo::TeamRunReconciliation {
            delivery_id: binding.delivery_id,
            turn_id: turn.turn_id.clone(),
            run_id: turn.run_id.clone(),
            status: turn.status,
            created_at_unix_ms: turn.created_at_unix_ms,
            updated_at_unix_ms: turn.updated_at_unix_ms,
            project_write_evidence_verified: terminal_worker
                .project_write_completion_verified(&owner_user_id, &turn.run_id),
        });
    }
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    if !reconciliations.is_empty() {
        let reconcile_store = conversations.inner().clone();
        let reconcile_owner = owner_user_id.clone();
        let reconcile_team = team_id.clone();
        tauri::async_runtime::spawn_blocking(move || {
            reconcile_store.reconcile_project_team_runs(
                &reconcile_owner,
                &reconcile_team,
                &reconciliations,
            )
        })
        .await
        .map_err(|_| "Agent 团队运行对账任务异常退出。".to_owned())??;
    }
    let store = conversations.inner().clone();
    let owner = owner_user_id.clone();
    let team = team_id.clone();
    let snapshot =
        tauri::async_runtime::spawn_blocking(move || store.load_project_team_runs(&owner, &team))
            .await
            .map_err(|_| "Agent 团队运行记录读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(snapshot)
}

#[tauri::command]
pub(crate) async fn cancel_agent_team_run(
    input: CancelAgentTeamRunInput,
    app: AppHandle,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
    manager: State<'_, HostManager>,
) -> Result<(), String> {
    validate_owner(&input.owner_user_id)?;
    validate_identifier(&input.team_id).map_err(|_| invalid_team_input())?;
    validate_identifier(&input.run_id).map_err(|_| invalid_team_input())?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let binding_store = conversations.inner().clone();
    let binding_owner = input.owner_user_id.clone();
    let binding_team = input.team_id.clone();
    let bindings = tauri::async_runtime::spawn_blocking(move || {
        binding_store.active_project_team_run_bindings(&binding_owner, &binding_team)
    })
    .await
    .map_err(|_| "Agent 团队运行绑定读取任务异常退出。".to_owned())??;
    let binding = bindings
        .into_iter()
        .find(|binding| binding.run_id == input.run_id);
    let Some(binding) = binding else {
        // A poll or another client may have observed the terminal Host state between the user's
        // click and this command. Treat that exact recorded Run as an idempotent success, but do
        // not let an unrelated or cross-team Run ID turn into a successful cancellation.
        let snapshot_store = conversations.inner().clone();
        let snapshot_owner = input.owner_user_id.clone();
        let snapshot_team = input.team_id.clone();
        let snapshot = tauri::async_runtime::spawn_blocking(move || {
            snapshot_store.load_project_team_runs(&snapshot_owner, &snapshot_team)
        })
        .await
        .map_err(|_| "Agent 团队运行记录读取任务异常退出。".to_owned())??;
        if snapshot.runs.iter().any(|run| {
            run.run_id == input.run_id && !matches!(run.run_status.as_str(), "ready" | "running")
        }) {
            authentication
                .require_session_generation(&input.owner_user_id, generation)
                .await?;
            return Ok(());
        }
        return Err("找不到可结束的团队 Run，请刷新后重试。".into());
    };
    let turn = team_todo::cancel_active_team_run(
        manager.inner(),
        &input.owner_user_id,
        &binding.conversation_id,
        &binding.turn_id,
        &binding.run_id,
        "用户已结束这个未完成的 Agent Run。",
    )
    .await?;
    let reconciliation = team_todo::TeamRunReconciliation {
        delivery_id: binding.delivery_id,
        turn_id: turn.turn_id,
        run_id: turn.run_id,
        status: turn.status,
        created_at_unix_ms: turn.created_at_unix_ms,
        updated_at_unix_ms: turn.updated_at_unix_ms,
        project_write_evidence_verified: false,
    };
    let reconcile_store = conversations.inner().clone();
    let reconcile_owner = input.owner_user_id.clone();
    let reconcile_team = input.team_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        reconcile_store.reconcile_project_team_runs(
            &reconcile_owner,
            &reconcile_team,
            &[reconciliation],
        )
    })
    .await
    .map_err(|_| "Agent 团队运行停止对账任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    crate::attachment_tool_worker::schedule_team_todo_drain(
        app,
        input.owner_user_id,
        input.team_id,
    );
    Ok(())
}

#[tauri::command]
pub(crate) async fn load_agent_team_dashboard(
    owner_user_id: String,
    team_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<AgentProjectDashboardSnapshot, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&team_id).map_err(|_| invalid_team_input())?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = owner_user_id.clone();
    let team = team_id.clone();
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        store.load_project_dashboard_snapshot(&owner, &team)
    })
    .await
    .map_err(|_| "项目总览读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(snapshot)
}
