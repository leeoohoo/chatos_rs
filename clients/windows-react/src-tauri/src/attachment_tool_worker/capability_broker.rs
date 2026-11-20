use super::*;

pub(super) async fn execute_capability_broker_tool(
    app: &AppHandle,
    owner_user_id: &str,
    claim: &LocalAgentToolClaim,
) -> Result<LocalAgentToolOutcome, String> {
    let arguments = claim
        .invocation
        .arguments
        .as_object()
        .ok_or_else(|| "The Plugin capability arguments are invalid.".to_owned())?;
    let host = app.state::<HostManager>();
    let result = host
        .execute(HostCommand::GetRun(GetRunCommand {
            owner_user_id: owner_user_id.to_owned(),
            run_id: claim.invocation.run_id.clone(),
        }))
        .await?;
    let HostResult::Run { run } = result else {
        return Err("The Plugin capability Run context is invalid.".into());
    };
    if run.owner_user_id != owner_user_id
        || run.run_id != claim.invocation.run_id
        || run.profile_key != "task_execution"
    {
        return Err("The Plugin capability Run context is invalid.".into());
    }
    let store = app.state::<AgentConversationStore>().inner().clone();
    let owner = owner_user_id.to_owned();
    let run_id = run.run_id.clone();
    let (binding, _) = tauri::async_runtime::spawn_blocking(move || {
        store.load_agent_asset_tool_records(&owner, &run_id)
    })
    .await
    .map_err(|_| "The Plugin capability binding task exited unexpectedly.".to_owned())??;
    validate_asset_run_context(&run.profile_key, &run.input, &binding)?;
    if binding.todo_id.is_none() {
        return Err("Plugin capabilities are available only in a Todo executor Run.".into());
    }
    if matches!(
        claim.invocation.tool_name.as_str(),
        "agent_skill_activate" | "agent_skill_read_resource"
    ) {
        if let Some(outcome) = app
            .state::<AttachmentToolWorker>()
            .product_skills
            .execute_if_product(
                host.inner(),
                owner_user_id,
                &run.run_id,
                &claim.invocation.tool_name,
                arguments,
            )
            .await?
        {
            return Ok(outcome);
        }
    }
    let platform =
        resolve_platform_execution_context(app, host.inner(), owner_user_id, &run.run_id).await?;
    let plugin_manager = app.state::<PluginApplicationManager>();
    plugin_manager
        .agent_runtime
        .execute(
            plugin_manager.inner(),
            host.inner(),
            AgentPluginRunContext {
                owner_user_id: owner_user_id.to_owned(),
                run_id: run.run_id,
                execution_root: platform.execution_root,
                workspace_scope_id: platform.workspace_scope_id,
                input: run.input,
            },
            &claim.invocation.invocation_id,
            &claim.invocation.tool_name,
            arguments,
        )
        .await
}
