use super::*;

pub(super) async fn execute_project_dashboard_tool(
    app: &AppHandle,
    owner_user_id: &str,
    claim: &LocalAgentToolClaim,
) -> Result<LocalAgentToolOutcome, String> {
    if !PROJECT_DASHBOARD_TOOL_NAMES.contains(&claim.invocation.tool_name.as_str()) {
        return Err("The requested project dashboard tool is unavailable.".into());
    }
    let arguments = claim
        .invocation
        .arguments
        .as_object()
        .ok_or_else(invalid_dashboard_arguments)?;
    let host = app.state::<HostManager>();
    let result = host
        .execute(HostCommand::GetRun(GetRunCommand {
            owner_user_id: owner_user_id.to_owned(),
            run_id: claim.invocation.run_id.clone(),
        }))
        .await?;
    let HostResult::Run { run } = result else {
        return Err("The project dashboard Run context is invalid.".into());
    };
    if run.owner_user_id != owner_user_id || run.run_id != claim.invocation.run_id {
        return Err("The project dashboard Run context is invalid.".into());
    }
    let store = app.state::<AgentConversationStore>().inner().clone();
    let owner = owner_user_id.to_owned();
    let run_id = run.run_id.clone();
    let (binding, planning_snapshot) = tauri::async_runtime::spawn_blocking(move || {
        store.todo_planning_tool_snapshot(&owner, &run_id, true)
    })
    .await
    .map_err(|_| "The project dashboard lookup task exited unexpectedly.".to_owned())??;
    validate_asset_run_context(&run.profile_key, &run.input, &binding)?;
    let team_reference = planning_snapshot["team"]["team_ref"]
        .as_str()
        .ok_or_else(|| "The project dashboard team context is invalid.".to_owned())?;
    if let Some(requested) = arguments.get("team_ref") {
        if requested.as_str() != Some(team_reference) {
            return Err("The project dashboard team reference is unavailable for this Run.".into());
        }
    }

    let output = match claim.invocation.tool_name.as_str() {
        "project_dashboard_get" => {
            let store = app.state::<AgentConversationStore>().inner().clone();
            let owner = owner_user_id.to_owned();
            let team_id = binding.team_id.clone();
            let dashboard = tauri::async_runtime::spawn_blocking(move || {
                store.load_project_dashboard_snapshot(&owner, &team_id)
            })
            .await
            .map_err(|_| "The project dashboard read task exited unexpectedly.".to_owned())??;
            project_dashboard_output(
                owner_user_id,
                &run.run_id,
                team_reference,
                &planning_snapshot,
                dashboard,
            )?
        }
        "project_dashboard_update" => {
            if binding.todo_id.is_some() || !binding.is_project_manager {
                return Err(
                    "Only the active project manager Agent may update the dashboard.".into(),
                );
            }
            let requested_team = required_dashboard_string(arguments, "team_ref", 600)?;
            if requested_team != team_reference {
                return Err(
                    "The project dashboard team reference is unavailable for this Run.".into(),
                );
            }
            let expected_revision = arguments
                .get("expected_revision")
                .map(|value| {
                    value
                        .as_i64()
                        .filter(|revision| *revision > 0)
                        .ok_or_else(invalid_dashboard_arguments)
                })
                .transpose()?;
            let (draft, todo_references) = parse_project_dashboard_update(arguments)?;
            let unique_references = todo_references
                .iter()
                .cloned()
                .collect::<HashSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let store = app.state::<AgentConversationStore>().inner().clone();
            let owner = owner_user_id.to_owned();
            let run_id = run.run_id.clone();
            let references_for_lookup = unique_references.clone();
            let (verified_binding, resolved_ids) =
                tauri::async_runtime::spawn_blocking(move || {
                    store.resolve_dashboard_todo_references(&owner, &run_id, &references_for_lookup)
                })
                .await
                .map_err(|_| {
                    "The project dashboard Todo lookup task exited unexpectedly.".to_owned()
                })??;
            if verified_binding != binding {
                return Err("The project dashboard binding changed during the operation.".into());
            }
            let todo_ids = unique_references
                .into_iter()
                .zip(resolved_ids)
                .collect::<HashMap<_, _>>();
            let update = materialize_project_dashboard_update(draft, &todo_ids)?;
            let store = app.state::<AgentConversationStore>().inner().clone();
            let owner = owner_user_id.to_owned();
            let team_id = binding.team_id.clone();
            let editor = binding.agent_id.clone();
            let saved = tauri::async_runtime::spawn_blocking(move || {
                store.save_project_dashboard_as_manager(
                    &owner,
                    &team_id,
                    &editor,
                    expected_revision,
                    update,
                    now_unix_ms(),
                )
            })
            .await
            .map_err(|_| "The project dashboard update task exited unexpectedly.".to_owned())??;
            dashboard_record_output(&saved, owner_user_id, &run.run_id)?
        }
        _ => return Err("The requested project dashboard tool is unavailable.".into()),
    };
    Ok(LocalAgentToolOutcome::Succeeded { output })
}

#[derive(Debug)]
struct DashboardMilestoneDraft {
    id: String,
    title: String,
    detail: String,
    status: String,
    progress_percent: i64,
    acceptance_criteria: Vec<String>,
    todo_refs: Vec<String>,
    target_at_unix_ms: Option<i64>,
}

#[derive(Debug)]
struct DashboardIssueDraft {
    id: String,
    title: String,
    detail: String,
    requested_action: String,
    severity: String,
    owner: String,
    todo_ref: Option<String>,
}

#[derive(Debug)]
struct ProjectDashboardDraft {
    phase: String,
    health: String,
    summary: String,
    next_steps: Vec<String>,
    milestones: Vec<DashboardMilestoneDraft>,
    issues: Vec<DashboardIssueDraft>,
}

fn parse_project_dashboard_update(
    arguments: &Map<String, Value>,
) -> Result<(ProjectDashboardDraft, Vec<String>), String> {
    let phase = required_dashboard_string(arguments, "phase", 240)?.to_owned();
    let health = required_dashboard_string(arguments, "health", 32)?.to_owned();
    if !matches!(
        health.as_str(),
        "on_track" | "at_risk" | "blocked" | "completed"
    ) {
        return Err(invalid_dashboard_arguments());
    }
    let summary = required_dashboard_string(arguments, "summary", 16_000)?.to_owned();
    let next_steps = dashboard_string_array(arguments.get("next_steps"), 32, 2_000)?;
    let milestone_values = dashboard_object_array(arguments.get("milestones"), 64)?;
    let issue_values = dashboard_object_array(arguments.get("issues"), 64)?;
    let mut todo_references = Vec::new();
    let mut milestones = Vec::with_capacity(milestone_values.len());
    for value in milestone_values {
        let id = required_dashboard_string(value, "id", 512)?.to_owned();
        let title = required_dashboard_string(value, "title", 240)?.to_owned();
        let detail = optional_dashboard_string(value, "detail", 8_000)?.unwrap_or_default();
        let status = required_dashboard_string(value, "status", 32)?.to_owned();
        if !matches!(
            status.as_str(),
            "pending" | "in_progress" | "blocked" | "completed"
        ) {
            return Err(invalid_dashboard_arguments());
        }
        let progress_percent = value
            .get("progress_percent")
            .and_then(Value::as_i64)
            .filter(|progress| (0..=100).contains(progress))
            .ok_or_else(invalid_dashboard_arguments)?;
        let acceptance_criteria =
            dashboard_string_array(value.get("acceptance_criteria"), 32, 2_000)?;
        let todo_refs = dashboard_unique_string_array(value.get("todo_refs"), 128, 600)?;
        todo_references.extend(todo_refs.iter().cloned());
        let target_at_unix_ms = value
            .get("target_at_unix_ms")
            .map(|value| {
                value
                    .as_i64()
                    .filter(|time| *time >= 0)
                    .ok_or_else(invalid_dashboard_arguments)
            })
            .transpose()?;
        milestones.push(DashboardMilestoneDraft {
            id,
            title,
            detail,
            status,
            progress_percent,
            acceptance_criteria,
            todo_refs,
            target_at_unix_ms,
        });
    }
    let mut issues = Vec::with_capacity(issue_values.len());
    for value in issue_values {
        let severity = required_dashboard_string(value, "severity", 32)?.to_owned();
        let owner = required_dashboard_string(value, "owner", 32)?.to_owned();
        if !matches!(severity.as_str(), "info" | "warning" | "critical")
            || !matches!(owner.as_str(), "human" | "agent" | "external")
        {
            return Err(invalid_dashboard_arguments());
        }
        let todo_ref = optional_dashboard_string(value, "todo_ref", 600)?;
        if let Some(reference) = todo_ref.as_ref() {
            todo_references.push(reference.clone());
        }
        issues.push(DashboardIssueDraft {
            id: required_dashboard_string(value, "id", 512)?.to_owned(),
            title: required_dashboard_string(value, "title", 240)?.to_owned(),
            detail: optional_dashboard_string(value, "detail", 8_000)?.unwrap_or_default(),
            requested_action: required_dashboard_string(value, "requested_action", 4_000)?
                .to_owned(),
            severity,
            owner,
            todo_ref,
        });
    }
    Ok((
        ProjectDashboardDraft {
            phase,
            health,
            summary,
            next_steps,
            milestones,
            issues,
        },
        todo_references,
    ))
}

fn materialize_project_dashboard_update(
    draft: ProjectDashboardDraft,
    todo_ids: &HashMap<String, String>,
) -> Result<AgentProjectDashboardUpdate, String> {
    Ok(AgentProjectDashboardUpdate {
        phase: draft.phase,
        health: draft.health,
        summary: draft.summary,
        next_steps: draft.next_steps,
        milestones: draft
            .milestones
            .into_iter()
            .map(|milestone| {
                Ok(AgentProjectMilestone {
                    id: milestone.id,
                    title: milestone.title,
                    detail: milestone.detail,
                    status: milestone.status,
                    progress_percent: milestone.progress_percent,
                    acceptance_criteria: milestone.acceptance_criteria,
                    linked_todo_ids: milestone
                        .todo_refs
                        .into_iter()
                        .map(|reference| {
                            todo_ids
                                .get(&reference)
                                .cloned()
                                .ok_or_else(invalid_dashboard_arguments)
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                    target_at_unix_ms: milestone.target_at_unix_ms,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
        issues: draft
            .issues
            .into_iter()
            .map(|issue| {
                Ok(AgentProjectIssue {
                    id: issue.id,
                    title: issue.title,
                    detail: issue.detail,
                    requested_action: issue.requested_action,
                    severity: issue.severity,
                    owner: issue.owner,
                    related_todo_id: issue
                        .todo_ref
                        .map(|reference| {
                            todo_ids
                                .get(&reference)
                                .cloned()
                                .ok_or_else(invalid_dashboard_arguments)
                        })
                        .transpose()?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
    })
}

fn project_dashboard_output(
    owner_user_id: &str,
    run_id: &str,
    team_reference: &str,
    planning_snapshot: &Value,
    snapshot: crate::agent_conversation::AgentProjectDashboardSnapshot,
) -> Result<Value, String> {
    let todos = planning_snapshot["todos"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let count = |status: &str| {
        todos
            .iter()
            .filter(|todo| todo["status"].as_str() == Some(status))
            .count()
    };
    Ok(json!({
        "team_ref": team_reference,
        "team_name": planning_snapshot["team"]["name"],
        "team_goal": planning_snapshot["team"]["goal"],
        "dashboard": snapshot.dashboard.as_ref()
            .map(|dashboard| dashboard_record_output(dashboard, owner_user_id, run_id))
            .transpose()?,
        "facts": {
            "total_todos": todos.len(),
            "pending_todos": count("pending"),
            "in_progress_todos": count("in_progress"),
            "blocked_todos": count("blocked"),
            "completed_todos": count("completed"),
            "pending_approvals": snapshot.pending_approval_count,
        },
        "todos": todos,
    }))
}

fn dashboard_record_output(
    dashboard: &crate::agent_conversation::AgentProjectDashboard,
    owner_user_id: &str,
    run_id: &str,
) -> Result<Value, String> {
    let encoded = serde_json::to_value(dashboard).map_err(|_| invalid_dashboard_arguments())?;
    let milestones = encoded["milestones"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|item| {
            let refs = item["linkedTodoIds"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(|id| todo_planning_reference(owner_user_id, run_id, "todo", id))
                .collect::<Vec<_>>();
            json!({
                "id": item["id"], "title": item["title"], "detail": item["detail"],
                "status": item["status"], "progress_percent": item["progressPercent"],
                "acceptance_criteria": item["acceptanceCriteria"], "todo_refs": refs,
                "target_at_unix_ms": item["targetAtUnixMs"],
            })
        })
        .collect::<Vec<_>>();
    let issues = encoded["issues"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|item| {
            let reference = item["relatedTodoId"]
                .as_str()
                .map(|id| todo_planning_reference(owner_user_id, run_id, "todo", id));
            json!({
                "id": item["id"], "title": item["title"], "detail": item["detail"],
                "requested_action": item["requestedAction"], "severity": item["severity"],
                "owner": item["owner"], "todo_ref": reference,
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "revision": encoded["revision"], "phase": encoded["phase"],
        "health": encoded["health"], "summary": encoded["summary"],
        "next_steps": encoded["nextSteps"], "milestones": milestones, "issues": issues,
        "updated_at_unix_ms": encoded["updatedAtUnixMs"],
    }))
}

fn dashboard_object_array(
    value: Option<&Value>,
    maximum: usize,
) -> Result<Vec<&Map<String, Value>>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    value
        .as_array()
        .filter(|values| values.len() <= maximum)
        .ok_or_else(invalid_dashboard_arguments)?
        .iter()
        .map(|item| item.as_object().ok_or_else(invalid_dashboard_arguments))
        .collect()
}

fn dashboard_string_array(
    value: Option<&Value>,
    maximum_items: usize,
    maximum_characters: usize,
) -> Result<Vec<String>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    value
        .as_array()
        .filter(|values| values.len() <= maximum_items)
        .ok_or_else(invalid_dashboard_arguments)?
        .iter()
        .map(|item| {
            item.as_str()
                .filter(|text| {
                    !text.is_empty()
                        && text.trim() == *text
                        && text.chars().count() <= maximum_characters
                        && !text.contains('\0')
                })
                .map(str::to_owned)
                .ok_or_else(invalid_dashboard_arguments)
        })
        .collect()
}

fn dashboard_unique_string_array(
    value: Option<&Value>,
    maximum_items: usize,
    maximum_characters: usize,
) -> Result<Vec<String>, String> {
    let values = dashboard_string_array(value, maximum_items, maximum_characters)?;
    if values.iter().collect::<HashSet<_>>().len() != values.len() {
        return Err(invalid_dashboard_arguments());
    }
    Ok(values)
}

fn required_dashboard_string<'a>(
    arguments: &'a Map<String, Value>,
    field: &str,
    maximum: usize,
) -> Result<&'a str, String> {
    arguments
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| {
            !value.is_empty()
                && value.trim() == *value
                && value.chars().count() <= maximum
                && !value.contains('\0')
        })
        .ok_or_else(invalid_dashboard_arguments)
}

fn optional_dashboard_string(
    arguments: &Map<String, Value>,
    field: &str,
    maximum: usize,
) -> Result<Option<String>, String> {
    arguments
        .get(field)
        .map(|_| required_dashboard_string(arguments, field, maximum).map(str::to_owned))
        .transpose()
}

fn invalid_dashboard_arguments() -> String {
    "The project dashboard arguments are invalid.".into()
}
