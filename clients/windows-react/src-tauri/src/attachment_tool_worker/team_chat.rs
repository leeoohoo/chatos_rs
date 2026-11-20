use super::*;

static ACTIVE_TEAM_TODO_DRAINS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

pub(super) async fn execute_team_chat_read_tool(
    app: &AppHandle,
    owner_user_id: &str,
    claim: &LocalAgentToolClaim,
) -> Result<LocalAgentToolOutcome, String> {
    let arguments = claim
        .invocation
        .arguments
        .as_object()
        .ok_or_else(|| "The team communication arguments are invalid.".to_owned())?;
    let result = app
        .state::<HostManager>()
        .execute(HostCommand::GetRun(GetRunCommand {
            owner_user_id: owner_user_id.to_owned(),
            run_id: claim.invocation.run_id.clone(),
        }))
        .await?;
    let HostResult::Run { run } = result else {
        return Err("The team communication Run context is invalid.".into());
    };
    if run.owner_user_id != owner_user_id || run.run_id != claim.invocation.run_id {
        return Err("The team communication Run context is invalid.".into());
    }
    let store = app.state::<AgentConversationStore>().inner().clone();
    let owner = owner_user_id.to_owned();
    let run_id = run.run_id.clone();
    let request_key = claim.invocation.invocation_id.clone();
    let tool_name = claim.invocation.tool_name.clone();
    let limit = match tool_name.as_str() {
        "agent_workspace_snapshot" | "chat_list_members" if arguments.is_empty() => None,
        "chat_read_all_unread" => {
            Some(optional_planning_integer(arguments, "limit", 1, 500)?.unwrap_or(200))
        }
        "chat_read_attachment" => {
            Some(optional_planning_integer(arguments, "limit", 1, 12_000)?.unwrap_or(12_000))
        }
        _ => return Err("The team communication arguments are invalid.".into()),
    };
    let message_reference = if tool_name == "chat_read_attachment" {
        Some(required_bounded_string(arguments, "message_ref", 600)?)
    } else {
        None
    };
    let attachment_reference = if tool_name == "chat_read_attachment" {
        Some(required_bounded_string(arguments, "attachment_ref", 600)?)
    } else {
        None
    };
    let offset = if tool_name == "chat_read_attachment" {
        optional_planning_integer(arguments, "offset", 0, i64::MAX)?.unwrap_or(0) as usize
    } else {
        0
    };
    let (binding, output) =
        tauri::async_runtime::spawn_blocking(move || match tool_name.as_str() {
            "agent_workspace_snapshot" => store.agent_workspace_snapshot(&owner, &run_id),
            "chat_list_members" => store.team_chat_member_snapshot(&owner, &run_id),
            "chat_read_all_unread" => store.read_all_unread_team_chat(
                &owner,
                &run_id,
                &request_key,
                limit.ok_or_else(|| "The unread limit is unavailable.".to_owned())? as usize,
                now_unix_ms(),
            ),
            "chat_read_attachment" => store.read_agent_chat_attachment(
                &owner,
                &run_id,
                message_reference
                    .as_deref()
                    .ok_or_else(|| "The message reference is unavailable.".to_owned())?,
                attachment_reference
                    .as_deref()
                    .ok_or_else(|| "The attachment reference is unavailable.".to_owned())?,
                offset,
                limit.ok_or_else(|| "The attachment read limit is unavailable.".to_owned())?
                    as usize,
            ),
            _ => Err("The team communication tool is unavailable.".into()),
        })
        .await
        .map_err(|_| "The team member lookup task exited unexpectedly.".to_owned())??;
    validate_communication_run_context(&run.profile_key, &run.input, &binding)?;
    Ok(LocalAgentToolOutcome::Succeeded { output })
}

pub(super) async fn execute_team_chat_write_tool(
    app: &AppHandle,
    owner_user_id: &str,
    claim: &LocalAgentToolClaim,
) -> Result<LocalAgentToolOutcome, String> {
    let arguments = claim
        .invocation
        .arguments
        .as_object()
        .ok_or_else(|| "The team communication arguments are invalid.".to_owned())?;
    let result = app
        .state::<HostManager>()
        .execute(HostCommand::GetRun(GetRunCommand {
            owner_user_id: owner_user_id.to_owned(),
            run_id: claim.invocation.run_id.clone(),
        }))
        .await?;
    let HostResult::Run { run } = result else {
        return Err("The team communication Run context is invalid.".into());
    };
    if run.owner_user_id != owner_user_id || run.run_id != claim.invocation.run_id {
        return Err("The team communication Run context is invalid.".into());
    }
    if claim.invocation.tool_name == "chat_document_create" {
        let name = required_bounded_string(arguments, "name", 240)?;
        let title = required_bounded_string(arguments, "title", 512)?;
        let markdown = required_bounded_string(arguments, "markdown", 2 * 1_024 * 1_024)?;
        let store = app.state::<AgentConversationStore>().inner().clone();
        let owner = owner_user_id.to_owned();
        let run_id = run.run_id.clone();
        let request_key = claim.invocation.invocation_id.clone();
        let (binding, output) = tauri::async_runtime::spawn_blocking(move || {
            store.create_agent_chat_document(
                &owner,
                &run_id,
                &request_key,
                &name,
                &title,
                &markdown,
                now_unix_ms(),
            )
        })
        .await
        .map_err(|_| "The Agent document task exited unexpectedly.".to_owned())??;
        validate_communication_run_context(&run.profile_key, &run.input, &binding)?;
        return Ok(LocalAgentToolOutcome::Succeeded { output });
    }
    if matches!(
        claim.invocation.tool_name.as_str(),
        "agent_propose_member" | "agent_propose_existing_member" | "agent_propose_member_removal"
    ) {
        let store = app.state::<AgentConversationStore>().inner().clone();
        let owner = owner_user_id.to_owned();
        let run_id = run.run_id.clone();
        let request_key = claim.invocation.invocation_id.clone();
        let tool_name = claim.invocation.tool_name.clone();
        let arguments = arguments.clone();
        let (binding, output) =
            tauri::async_runtime::spawn_blocking(move || match tool_name.as_str() {
                "agent_propose_member" => store.create_agent_creation_proposal(
                    &owner,
                    &run_id,
                    &request_key,
                    &required_bounded_string(&arguments, "name", 120)?,
                    &required_bounded_string(&arguments, "role", 160)?,
                    optional_bounded_string(&arguments, "responsibility", 8_000)?
                        .as_deref()
                        .unwrap_or_default(),
                    &required_bounded_string(&arguments, "role_prompt", 32_000)?,
                    optional_bounded_string(&arguments, "thinking_level", 16)?.as_deref(),
                    &required_bounded_string(&arguments, "profession_key", 120)?,
                    optional_bounded_string(&arguments, "rationale", 4_000)?
                        .as_deref()
                        .unwrap_or_default(),
                    now_unix_ms(),
                ),
                "agent_propose_existing_member" => store.create_agent_membership_proposal(
                    &owner,
                    &run_id,
                    &request_key,
                    &required_bounded_string(&arguments, "team_ref", 600)?,
                    &required_bounded_string(&arguments, "target_agent_ref", 600)?,
                    &required_bounded_string(&arguments, "role", 160)?,
                    optional_bounded_string(&arguments, "responsibility", 8_000)?
                        .as_deref()
                        .unwrap_or_default(),
                    now_unix_ms(),
                ),
                "agent_propose_member_removal" => store.create_agent_removal_proposal(
                    &owner,
                    &run_id,
                    &request_key,
                    &required_bounded_string(&arguments, "target_agent_ref", 600)?,
                    &required_bounded_string(&arguments, "reason", 4_000)?,
                    optional_bounded_string(&arguments, "handoff_plan", 8_000)?
                        .as_deref()
                        .unwrap_or_default(),
                    now_unix_ms(),
                ),
                _ => Err("The Agent staffing proposal tool is unavailable.".into()),
            })
            .await
            .map_err(|_| "The Agent staffing proposal task exited unexpectedly.".to_owned())??;
        validate_communication_run_context(&run.profile_key, &run.input, &binding)?;
        return Ok(LocalAgentToolOutcome::Succeeded { output });
    }
    if matches!(
        claim.invocation.tool_name.as_str(),
        "chat_heartbeat_complete" | "agent_cycle_complete"
    ) {
        if !arguments.is_empty() {
            return Err("The Agent cycle completion arguments are invalid.".into());
        }
        let heartbeat_only = claim.invocation.tool_name == "chat_heartbeat_complete";
        let store = app.state::<AgentConversationStore>().inner().clone();
        let owner = owner_user_id.to_owned();
        let run_id = run.run_id.clone();
        let (binding, output) = tauri::async_runtime::spawn_blocking(move || {
            store.validate_agent_communication_cycle_completion(&owner, &run_id, heartbeat_only)
        })
        .await
        .map_err(|_| "The Agent cycle completion task exited unexpectedly.".to_owned())??;
        validate_communication_run_context(&run.profile_key, &run.input, &binding)?;
        return Ok(LocalAgentToolOutcome::Succeeded { output });
    }
    if claim.invocation.tool_name != "chat_send_message" {
        return Err("The team communication mutation is unavailable.".into());
    }
    let content = required_bounded_string(arguments, "content", 2_000)?;
    let reply_reference = optional_bounded_string(arguments, "reply_to_message_ref", 600)?;
    let target_agent_reference = optional_bounded_string(arguments, "target_agent_ref", 600)?;
    let team_reference = optional_bounded_string(arguments, "team_ref", 600)?;
    if [
        reply_reference.is_some(),
        target_agent_reference.is_some(),
        team_reference.is_some(),
    ]
    .into_iter()
    .filter(|selected| *selected)
    .count()
        > 1
    {
        return Err("The team communication target is ambiguous.".into());
    }
    let mention_references =
        planning_string_array(arguments, "mention_agent_refs", 0, 64, 600)?.unwrap_or_default();
    let document_references =
        planning_string_array(arguments, "document_refs", 0, 5, 600)?.unwrap_or_default();
    let store = app.state::<AgentConversationStore>().inner().clone();
    let owner = owner_user_id.to_owned();
    let run_id = run.run_id.clone();
    let request_key = claim.invocation.invocation_id.clone();
    let (binding, output) = tauri::async_runtime::spawn_blocking(move || {
        store.send_team_chat_message(
            &owner,
            &run_id,
            &request_key,
            &content,
            reply_reference.as_deref(),
            target_agent_reference.as_deref(),
            team_reference.as_deref(),
            &document_references,
            &mention_references,
            now_unix_ms(),
        )
    })
    .await
    .map_err(|_| "The team message task exited unexpectedly.".to_owned())??;
    validate_communication_run_context(&run.profile_key, &run.input, &binding)?;
    Ok(LocalAgentToolOutcome::Succeeded { output })
}

pub(crate) fn schedule_team_todo_drain(app: AppHandle, owner_user_id: String, team_id: String) {
    tauri::async_runtime::spawn(async move {
        let key = format!("{owner_user_id}\0{team_id}");
        let active = ACTIVE_TEAM_TODO_DRAINS.get_or_init(|| Mutex::new(HashSet::new()));
        if !active.lock().await.insert(key.clone()) {
            return;
        }
        let mut consecutive_failures = 0_u8;
        loop {
            let reconciled = load_agent_team_runs(
                owner_user_id.clone(),
                team_id.clone(),
                app.state::<AuthenticationManager>(),
                app.state::<AgentConversationStore>(),
                app.state::<HostManager>(),
                app.state::<TerminalToolWorker>(),
            )
            .await
            .is_ok();
            let dispatched = dispatch_next_agent_team_delivery(
                owner_user_id.clone(),
                team_id.clone(),
                app.state::<AuthenticationManager>(),
                app.state::<AgentConversationStore>(),
                app.state::<AgentSkillStore>(),
                app.state::<crate::general_settings::GeneralSettingsStore>(),
                app.state::<HostManager>(),
                app.state::<AttachmentManager>(),
                app.state::<AttachmentToolWorker>(),
                app.state::<TerminalToolWorker>(),
            )
            .await;
            let store = app.state::<AgentConversationStore>().inner().clone();
            let owner = owner_user_id.clone();
            let team = team_id.clone();
            let has_work = tauri::async_runtime::spawn_blocking(move || {
                store.team_todo_scheduler_has_work(&owner, &team)
            })
            .await
            .ok()
            .and_then(Result::ok);
            if has_work == Some(false) {
                break;
            }
            if reconciled && dispatched.is_ok() && has_work.is_some() {
                consecutive_failures = 0;
            } else {
                consecutive_failures = consecutive_failures.saturating_add(1);
                if consecutive_failures >= 10 {
                    break;
                }
            }
            sleep(Duration::from_secs(2)).await;
        }
        active.lock().await.remove(&key);
    });
}

pub(crate) fn schedule_agent_communication_drain(
    app: AppHandle,
    owner_user_id: String,
    room_id: String,
) {
    tauri::async_runtime::spawn(async move {
        let key = format!("communication\0{owner_user_id}\0{room_id}");
        let active = ACTIVE_TEAM_TODO_DRAINS.get_or_init(|| Mutex::new(HashSet::new()));
        if !active.lock().await.insert(key.clone()) {
            return;
        }
        let mut consecutive_failures = 0_u8;
        loop {
            let reconciled = reconcile_agent_communication_runs(
                owner_user_id.clone(),
                room_id.clone(),
                app.state::<AuthenticationManager>(),
                app.state::<AgentConversationStore>(),
                app.state::<HostManager>(),
            )
            .await
            .is_ok();
            let dispatched = dispatch_next_agent_communication_delivery(
                owner_user_id.clone(),
                room_id.clone(),
                app.state::<AuthenticationManager>(),
                app.state::<AgentConversationStore>(),
                app.state::<AgentSkillStore>(),
                app.state::<crate::general_settings::GeneralSettingsStore>(),
                app.state::<HostManager>(),
                app.state::<AttachmentToolWorker>(),
                app.state::<TerminalToolWorker>(),
            )
            .await;
            let store = app.state::<AgentConversationStore>().inner().clone();
            let owner = owner_user_id.clone();
            let room = room_id.clone();
            let has_work = tauri::async_runtime::spawn_blocking(move || {
                store.communication_scheduler_has_work(&owner, &room)
            })
            .await
            .ok()
            .and_then(Result::ok);
            if has_work == Some(false) {
                break;
            }
            if reconciled && dispatched.is_ok() && has_work.is_some() {
                consecutive_failures = 0;
            } else {
                consecutive_failures = consecutive_failures.saturating_add(1);
                if consecutive_failures >= 10 {
                    break;
                }
            }
            sleep(Duration::from_secs(2)).await;
        }
        active.lock().await.remove(&key);
    });
}
