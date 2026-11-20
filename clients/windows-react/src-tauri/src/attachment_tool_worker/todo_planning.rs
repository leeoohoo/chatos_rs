use super::*;

pub(super) async fn execute_todo_planning_read_tool(
    app: &AppHandle,
    owner_user_id: &str,
    claim: &LocalAgentToolClaim,
) -> Result<LocalAgentToolOutcome, String> {
    if !TODO_PLANNING_READ_TOOL_NAMES.contains(&claim.invocation.tool_name.as_str()) {
        return Err("The requested Todo planning tool is unavailable.".into());
    }
    let arguments = claim
        .invocation
        .arguments
        .as_object()
        .ok_or_else(|| "The Todo planning arguments are invalid.".to_owned())?;
    let (run, expected_binding) = load_todo_planning_run_context(app, owner_user_id, claim).await?;
    let store = app.state::<AgentConversationStore>().inner().clone();
    let owner = owner_user_id.to_owned();
    let run_id = run.run_id.clone();
    if expected_binding.todo_id.is_some() {
        return Err("Todo planning tools are unavailable in an executor Run.".into());
    }
    let output = if claim.invocation.tool_name == "todo_read_progress" {
        let todo_reference = required_bounded_string(arguments, "todo_ref", 600)?;
        let limit = arguments
            .get("limit")
            .map(|value| {
                value
                    .as_u64()
                    .filter(|value| (1..=500).contains(value))
                    .map(|value| value as usize)
                    .ok_or_else(|| "The Todo planning arguments are invalid.".to_owned())
            })
            .transpose()?
            .unwrap_or(100);
        let (binding, output) = tauri::async_runtime::spawn_blocking(move || {
            store.todo_planning_progress(&owner, &run_id, &todo_reference, limit)
        })
        .await
        .map_err(|_| "The Todo progress lookup task exited unexpectedly.".to_owned())??;
        if binding != expected_binding {
            return Err("The Todo planning binding changed during the operation.".into());
        }
        output
    } else {
        let include_terminal = arguments
            .get("include_terminal")
            .map(|value| {
                value
                    .as_bool()
                    .ok_or_else(|| "The Todo planning arguments are invalid.".to_owned())
            })
            .transpose()?
            .unwrap_or(claim.invocation.tool_name != "todo_list");
        let (binding, snapshot) = tauri::async_runtime::spawn_blocking(move || {
            store.todo_planning_tool_snapshot(&owner, &run_id, include_terminal)
        })
        .await
        .map_err(|_| "The Todo planning lookup task exited unexpectedly.".to_owned())??;
        if binding != expected_binding {
            return Err("The Todo planning binding changed during the operation.".into());
        }
        match claim.invocation.tool_name.as_str() {
            "todo_execution_options" => {
                require_todo_planning_manager(&binding)?;
                let plugin_manager = app.state::<PluginApplicationManager>();
                let host = app.state::<HostManager>();
                let plugins = plugin_manager
                    .list_agent_plugins(host.inner(), owner_user_id)
                    .await?
                    .into_iter()
                    .map(|option| {
                        json!({
                            "plugin_ref": todo_planning_reference(
                                owner_user_id,
                                &run.run_id,
                                "plugin",
                                &option.frozen_identity(),
                            ),
                            "name": option.display_name,
                            "description": option.description,
                        })
                    })
                    .collect::<Vec<_>>();
                json!({
                    "teams": [snapshot["team"].clone()],
                    "builtin_capabilities": snapshot["builtin_capabilities"].clone(),
                    "plugins": plugins,
                })
            }
            "todo_dependency_options" => {
                require_todo_planning_manager(&binding)?;
                let team_reference = required_bounded_string(arguments, "team_ref", 600)?;
                if snapshot["team"]["team_ref"].as_str() != Some(team_reference.as_str()) {
                    return Err("The team reference is unavailable for this Run.".into());
                }
                let todos = snapshot["todos"]
                    .as_array()
                    .ok_or_else(|| "The Todo planning snapshot is invalid.".to_owned())?
                    .iter()
                    .filter(|todo| todo["status"].as_str() != Some("cancelled"))
                    .cloned()
                    .collect::<Vec<_>>();
                json!({"todos": todos})
            }
            "todo_list" => json!({"todos": snapshot["todos"].clone()}),
            _ => return Err("The requested Todo planning tool is unavailable.".into()),
        }
    };
    Ok(LocalAgentToolOutcome::Succeeded { output })
}

fn require_todo_planning_manager(binding: &AgentAssetToolBinding) -> Result<(), String> {
    if binding.todo_id.is_none() && binding.is_project_manager {
        Ok(())
    } else {
        Err("Only the active project manager Agent may plan team Todos.".into())
    }
}

pub(super) async fn execute_todo_planning_write_tool(
    app: &AppHandle,
    owner_user_id: &str,
    claim: &LocalAgentToolClaim,
) -> Result<LocalAgentToolOutcome, String> {
    if !TODO_PLANNING_WRITE_TOOL_NAMES.contains(&claim.invocation.tool_name.as_str()) {
        return Err("The requested Todo planning mutation is unavailable.".into());
    }
    let arguments = claim
        .invocation
        .arguments
        .as_object()
        .ok_or_else(|| "The Todo planning arguments are invalid.".to_owned())?;
    let (run, expected_binding) = load_todo_planning_run_context(app, owner_user_id, claim).await?;
    let store = app.state::<AgentConversationStore>().inner().clone();
    let owner = owner_user_id.to_owned();
    let run_id = run.run_id.clone();
    require_todo_planning_manager(&expected_binding)?;
    let now = now_unix_ms();
    let (binding, output) = match claim.invocation.tool_name.as_str() {
        "todo_add" => {
            let plugins =
                resolve_todo_plugin_hints(app, owner_user_id, &run.run_id, arguments).await?;
            let input = TodoPlanningCreateInput {
                request_key: claim.invocation.invocation_id.clone(),
                team_reference: required_bounded_string(arguments, "team_ref", 600)?,
                assignee_reference: required_bounded_string(arguments, "assignee_ref", 600)?,
                title: required_bounded_string(arguments, "title", 500)?,
                detail: optional_planning_text(arguments, "detail", 16_000, true)?
                    .unwrap_or_default(),
                priority: optional_planning_integer(arguments, "priority", 0, 100)?.unwrap_or(50),
                dependency_references: planning_string_array(
                    arguments,
                    "depends_on_todo_refs",
                    0,
                    64,
                    600,
                )?
                .unwrap_or_default(),
                execution_contract: AgentTodoExecutionContract {
                    objective: required_bounded_string(arguments, "objective", 8_000)?,
                    scope: required_bounded_string(arguments, "scope", 16_000)?,
                    expected_outputs: planning_string_array(
                        arguments,
                        "expected_outputs",
                        1,
                        64,
                        4_000,
                    )?
                    .ok_or_else(invalid_todo_planning_arguments)?,
                    acceptance_criteria: planning_string_array(
                        arguments,
                        "acceptance_criteria",
                        1,
                        64,
                        4_000,
                    )?
                    .ok_or_else(invalid_todo_planning_arguments)?,
                    constraints: planning_string_array(arguments, "constraints", 0, 64, 4_000)?
                        .unwrap_or_default(),
                },
                requires_execution: arguments
                    .get("requires_execution")
                    .and_then(Value::as_bool)
                    .ok_or_else(invalid_todo_planning_arguments)?,
                builtin_capabilities: planning_string_array(
                    arguments,
                    "builtin_capabilities",
                    0,
                    3,
                    64,
                )?
                .ok_or_else(invalid_todo_planning_arguments)?,
                plugins,
            };
            tauri::async_runtime::spawn_blocking(move || {
                store.create_todo_from_planning_tool(&owner, &run_id, input, now)
            })
            .await
            .map_err(|_| "The Todo creation task exited unexpectedly.".to_owned())??
        }
        "todo_update" => {
            let input = TodoPlanningUpdateInput {
                todo_reference: required_bounded_string(arguments, "todo_ref", 600)?,
                title: optional_planning_text(arguments, "title", 500, false)?,
                detail: optional_planning_text(arguments, "detail", 16_000, true)?,
                priority: optional_planning_integer(arguments, "priority", 0, 100)?,
                status: optional_planning_text(arguments, "status", 32, false)?,
                dependency_references: planning_string_array(
                    arguments,
                    "depends_on_todo_refs",
                    0,
                    64,
                    600,
                )?,
                objective: optional_planning_text(arguments, "objective", 8_000, false)?,
                scope: optional_planning_text(arguments, "scope", 16_000, false)?,
                expected_outputs: planning_string_array(
                    arguments,
                    "expected_outputs",
                    1,
                    64,
                    4_000,
                )?,
                acceptance_criteria: planning_string_array(
                    arguments,
                    "acceptance_criteria",
                    1,
                    64,
                    4_000,
                )?,
                constraints: planning_string_array(arguments, "constraints", 0, 64, 4_000)?,
            };
            tauri::async_runtime::spawn_blocking(move || {
                store.update_todo_from_planning_tool(&owner, &run_id, input, now)
            })
            .await
            .map_err(|_| "The Todo update task exited unexpectedly.".to_owned())??
        }
        "todo_reorder" => {
            let references = planning_string_array(arguments, "todo_refs", 1, 500, 600)?
                .ok_or_else(invalid_todo_planning_arguments)?;
            tauri::async_runtime::spawn_blocking(move || {
                store.reorder_todos_from_planning_tool(&owner, &run_id, &references, now)
            })
            .await
            .map_err(|_| "The Todo reorder task exited unexpectedly.".to_owned())??
        }
        _ => return Err("The requested Todo planning mutation is unavailable.".into()),
    };
    if binding != expected_binding {
        return Err("The Todo planning binding changed during the operation.".into());
    }
    Ok(LocalAgentToolOutcome::Succeeded { output })
}

async fn load_todo_planning_run_context(
    app: &AppHandle,
    owner_user_id: &str,
    claim: &LocalAgentToolClaim,
) -> Result<(LocalAgentRunRecord, AgentAssetToolBinding), String> {
    let result = app
        .state::<HostManager>()
        .execute(HostCommand::GetRun(GetRunCommand {
            owner_user_id: owner_user_id.to_owned(),
            run_id: claim.invocation.run_id.clone(),
        }))
        .await?;
    let HostResult::Run { run } = result else {
        return Err("The Todo planning Run context is invalid.".into());
    };
    if run.owner_user_id != owner_user_id || run.run_id != claim.invocation.run_id {
        return Err("The Todo planning Run context is invalid.".into());
    }
    let store = app.state::<AgentConversationStore>().inner().clone();
    let owner = owner_user_id.to_owned();
    let run_id = run.run_id.clone();
    let (binding, _) = tauri::async_runtime::spawn_blocking(move || {
        store.load_agent_asset_tool_records(&owner, &run_id)
    })
    .await
    .map_err(|_| "The Todo planning binding task exited unexpectedly.".to_owned())??;
    validate_asset_run_context(&run.profile_key, &run.input, &binding)?;
    Ok((run, binding))
}

async fn resolve_todo_plugin_hints(
    app: &AppHandle,
    owner_user_id: &str,
    run_id: &str,
    arguments: &Map<String, Value>,
) -> Result<Vec<AgentTodoPluginSelection>, String> {
    let Some(raw_hints) = arguments.get("plugin_hints") else {
        return Ok(Vec::new());
    };
    let hints = raw_hints
        .as_array()
        .filter(|hints| hints.len() <= 32)
        .ok_or_else(invalid_todo_planning_arguments)?;
    let plugin_manager = app.state::<PluginApplicationManager>();
    let host = app.state::<HostManager>();
    let options = plugin_manager
        .list_agent_plugins(host.inner(), owner_user_id)
        .await?;
    let by_reference = options
        .into_iter()
        .map(|option| {
            (
                todo_planning_reference(owner_user_id, run_id, "plugin", &option.frozen_identity()),
                option,
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    let mut selected_components = HashSet::new();
    hints
        .iter()
        .map(|hint| {
            let hint = hint
                .as_object()
                .filter(|hint| {
                    hint.len() <= 2
                        && hint
                            .keys()
                            .all(|key| matches!(key.as_str(), "plugin_ref" | "reason"))
                })
                .ok_or_else(invalid_todo_planning_arguments)?;
            let plugin_reference = required_bounded_string(hint, "plugin_ref", 600)?;
            let reason = optional_planning_text(hint, "reason", 1_000, true)?.unwrap_or_default();
            let option = by_reference.get(&plugin_reference).ok_or_else(|| {
                "Plugin option is unavailable or expired; call todo_execution_options again."
                    .to_owned()
            })?;
            if !selected_components.insert(option.selection_id()) {
                return Err("The same Plugin component cannot be selected twice.".into());
            }
            Ok(todo_plugin_selection(option, reason))
        })
        .collect()
}

fn todo_plugin_selection(
    option: &TrustedAgentPluginOption,
    reason: String,
) -> AgentTodoPluginSelection {
    AgentTodoPluginSelection {
        installation_id: option.installation_id.clone(),
        plugin_id: option.plugin_id.clone(),
        component_key: option.component_key.clone(),
        component_revision: option.component_revision.clone(),
        release_id: option.release_id.clone(),
        release_digest: option.release_digest.clone(),
        manifest_version: option.manifest_version.clone(),
        display_name: option.display_name.clone(),
        reason,
    }
}
