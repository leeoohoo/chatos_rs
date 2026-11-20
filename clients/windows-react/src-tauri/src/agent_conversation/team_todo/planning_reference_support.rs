use super::*;

pub(super) fn validate_reference_list(values: &[String], maximum: usize) -> Result<(), String> {
    if values.len() > maximum {
        return Err(invalid_team_todo_input());
    }
    let mut unique = HashSet::new();
    for value in values {
        validate_text(value, 600, false).map_err(|_| invalid_team_todo_input())?;
        if !unique.insert(value) {
            return Err(invalid_team_todo_input());
        }
    }
    Ok(())
}

pub(super) fn normalized_planning_capabilities(
    requested: &[String],
    requires_execution: bool,
) -> Result<Vec<String>, String> {
    if requested.len() > TODO_BUILTIN_CAPABILITIES.len() {
        return Err(invalid_team_todo_input());
    }
    let mut unique = HashSet::new();
    for capability in requested {
        if !TODO_BUILTIN_CAPABILITIES.contains(&capability.as_str())
            || !unique.insert(capability.as_str())
            || (!requires_execution && capability != "project_read")
        {
            return Err(invalid_team_todo_input());
        }
    }
    if unique.contains("project_write") {
        unique.insert("project_read");
    }
    if unique.contains("requirement_survey_write") {
        unique.insert("requirement_survey_read");
    }
    Ok(TODO_BUILTIN_CAPABILITIES
        .into_iter()
        .filter(|capability| unique.contains(capability))
        .map(str::to_owned)
        .collect())
}

pub(super) fn require_todo_planning_manager_tool_binding(
    connection: &Connection,
    owner_user_id: &str,
    run_id: &str,
) -> Result<(AgentAssetToolBinding, String), String> {
    let binding = load_agent_asset_tool_binding(connection, owner_user_id, run_id)?;
    if binding.todo_id.is_some() || !binding.is_project_manager {
        return Err("Only the active project manager Agent may plan team Todos.".into());
    }
    let source_message_id = connection
        .query_row(
            r#"
            SELECT delivery.message_id
            FROM windows_agent_delivery_runs run
            JOIN project_agent_deliveries delivery
              ON delivery.owner_user_id = run.owner_user_id
             AND delivery.id = run.delivery_id
            JOIN project_agent_messages message
              ON message.owner_user_id = delivery.owner_user_id
             AND message.id = delivery.message_id
             AND message.room_id = delivery.room_id
            WHERE run.owner_user_id = ?1 AND run.run_id = ?2
              AND run.status IN ('ready', 'running')
              AND delivery.status = 'running' AND delivery.room_id = ?3
              AND delivery.target_agent_id = ?4 AND delivery.trigger_kind != 'todo'
            "#,
            params![owner_user_id, run_id, binding.team_id, binding.agent_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|_| team_todo_store_error())?;
    Ok((binding, source_message_id))
}

pub(super) fn resolve_assignee_reference(
    connection: &Connection,
    owner_user_id: &str,
    run_id: &str,
    team_id: &str,
    reference: &str,
) -> Result<String, String> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT member.agent_id
            FROM project_agent_room_members member
            JOIN local_agent_profiles profile
              ON profile.owner_user_id = member.owner_user_id
             AND profile.id = member.agent_id
            WHERE member.owner_user_id = ?1 AND member.room_id = ?2
              AND member.status = 'active' AND profile.status = 'active'
            ORDER BY member.agent_id
            "#,
        )
        .map_err(|_| team_todo_store_error())?;
    let rows = statement
        .query_map(params![owner_user_id, team_id], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|_| team_todo_store_error())?;
    for row in rows {
        let agent_id = row.map_err(|_| team_todo_store_error())?;
        if todo_planning_reference(owner_user_id, run_id, "assignee", &agent_id) == reference {
            return Ok(agent_id);
        }
    }
    Err("The assignee reference is unavailable for this Run.".into())
}

pub(super) fn resolve_team_agent_references(
    connection: &Connection,
    owner_user_id: &str,
    run_id: &str,
    team_id: &str,
    references: &[String],
) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT member.agent_id
            FROM project_agent_room_members member
            JOIN local_agent_profiles profile
              ON profile.owner_user_id = member.owner_user_id
             AND profile.id = member.agent_id
            WHERE member.owner_user_id = ?1 AND member.room_id = ?2
              AND member.status = 'active' AND profile.status = 'active'
            ORDER BY member.agent_id
            "#,
        )
        .map_err(|_| team_todo_store_error())?;
    let agent_ids = statement
        .query_map(params![owner_user_id, team_id], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|_| team_todo_store_error())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| team_todo_store_error())?;
    references
        .iter()
        .map(|reference| {
            agent_ids
                .iter()
                .find(|agent_id| {
                    todo_planning_reference(owner_user_id, run_id, "agent", agent_id) == *reference
                })
                .cloned()
                .ok_or_else(|| "The Agent reference is unavailable for this Run.".to_owned())
        })
        .collect()
}

pub(super) fn resolve_todo_references(
    connection: &Connection,
    owner_user_id: &str,
    run_id: &str,
    team_id: &str,
    references: &[String],
    require_active: bool,
) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, status FROM local_agent_todos WHERE owner_user_id = ?1 \
             AND team_room_id = ?2 ORDER BY id",
        )
        .map_err(|_| team_todo_store_error())?;
    let rows = statement
        .query_map(params![owner_user_id, team_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|_| team_todo_store_error())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| team_todo_store_error())?;
    references
        .iter()
        .map(|reference| {
            rows.iter()
                .find(|(todo_id, status)| {
                    (!require_active || !matches!(status.as_str(), "completed" | "cancelled"))
                        && status != "cancelled"
                        && todo_planning_reference(owner_user_id, run_id, "todo", todo_id)
                            == *reference
                })
                .map(|(todo_id, _)| todo_id.clone())
                .ok_or_else(|| "The Todo reference is unavailable for this Run.".to_owned())
        })
        .collect()
}

pub(super) fn planning_snapshot_todo(
    snapshot: &Value,
    owner_user_id: &str,
    run_id: &str,
    todo_id: &str,
) -> Result<Value, String> {
    let reference = todo_planning_reference(owner_user_id, run_id, "todo", todo_id);
    snapshot["todos"]
        .as_array()
        .and_then(|todos| {
            todos
                .iter()
                .find(|todo| todo["todo_ref"].as_str() == Some(reference.as_str()))
        })
        .cloned()
        .ok_or_else(team_todo_store_error)
}
