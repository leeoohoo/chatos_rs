use super::*;

pub(super) async fn execute_todo_execution_tool(
    app: &AppHandle,
    owner_user_id: &str,
    claim: &LocalAgentToolClaim,
) -> Result<LocalAgentToolOutcome, String> {
    if !TODO_EXECUTION_TOOL_NAMES.contains(&claim.invocation.tool_name.as_str()) {
        return Err("The requested Todo execution tool is unavailable.".into());
    }
    let arguments = claim
        .invocation
        .arguments
        .as_object()
        .ok_or_else(|| "The Todo execution arguments are invalid.".to_owned())?;
    let result = app
        .state::<HostManager>()
        .execute(HostCommand::GetRun(GetRunCommand {
            owner_user_id: owner_user_id.to_owned(),
            run_id: claim.invocation.run_id.clone(),
        }))
        .await?;
    let HostResult::Run { run } = result else {
        return Err("The Todo execution Run context is invalid.".into());
    };
    if run.owner_user_id != owner_user_id || run.run_id != claim.invocation.run_id {
        return Err("The Todo execution Run context is invalid.".into());
    }
    let store = app.state::<AgentConversationStore>().inner().clone();
    let owner = owner_user_id.to_owned();
    let run_id = run.run_id.clone();
    let now = now_unix_ms();
    let validation_store = store.clone();
    let validation_owner = owner.clone();
    let validation_run = run_id.clone();
    let (expected_binding, _) = tauri::async_runtime::spawn_blocking(move || {
        validation_store.load_agent_asset_tool_records(&validation_owner, &validation_run)
    })
    .await
    .map_err(|_| "The Todo execution binding task exited unexpectedly.".to_owned())??;
    validate_asset_run_context(&run.profile_key, &run.input, &expected_binding)?;
    if expected_binding.todo_id.is_none() {
        return Err("The Todo execution Run context is invalid.".into());
    }
    let (binding, output) = match claim.invocation.tool_name.as_str() {
        "todo_get_context" => tauri::async_runtime::spawn_blocking(move || {
            store.todo_execution_tool_context(&owner, &run_id)
        })
        .await
        .map_err(|_| "The Todo context task exited unexpectedly.".to_owned())??,
        "todo_progress_append" => {
            let stage = optional_bounded_string(arguments, "stage", 240)?.unwrap_or_default();
            let detail = required_bounded_string(arguments, "detail", 16_000)?;
            tauri::async_runtime::spawn_blocking(move || {
                store.append_todo_execution_progress(&owner, &run_id, &stage, &detail, now)
            })
            .await
            .map_err(|_| "The Todo progress task exited unexpectedly.".to_owned())??
        }
        "todo_complete" => {
            let summary = required_bounded_string(arguments, "summary", 16_000)?;
            let asset_update_suggestions = arguments
                .get("asset_update_suggestions")
                .map(|value| {
                    serde_json::from_value::<Vec<AgentTeamAssetUpdateSuggestion>>(value.clone())
                        .map_err(|_| "The Todo execution arguments are invalid.".to_owned())
                })
                .transpose()?
                .unwrap_or_default();
            let verified = app
                .state::<TerminalToolWorker>()
                .project_write_completion_verified(owner_user_id, &run.run_id);
            tauri::async_runtime::spawn_blocking(move || {
                store.finish_todo_execution(
                    &owner,
                    &run_id,
                    true,
                    &summary,
                    &asset_update_suggestions,
                    verified,
                    now,
                )
            })
            .await
            .map_err(|_| "The Todo completion task exited unexpectedly.".to_owned())??
        }
        "todo_block" => {
            let reason = required_bounded_string(arguments, "reason", 8_000)?;
            tauri::async_runtime::spawn_blocking(move || {
                store.finish_todo_execution(&owner, &run_id, false, &reason, &[], false, now)
            })
            .await
            .map_err(|_| "The Todo blocking task exited unexpectedly.".to_owned())??
        }
        _ => return Err("The requested Todo execution tool is unavailable.".into()),
    };
    if binding != expected_binding {
        return Err("The Todo execution binding changed during the operation.".into());
    }
    Ok(LocalAgentToolOutcome::Succeeded { output })
}
