use super::*;

pub(super) fn load_todo_assignee_options(
    connection: &Connection,
    owner_user_id: &str,
    run_id: &str,
    team_id: &str,
    current_agent_id: &str,
) -> Result<Vec<Value>, String> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT profile.id, profile.name, profile.profession_key, member.role,
                   CASE WHEN room.project_manager_agent_id = profile.id THEN 1 ELSE 0 END
            FROM project_agent_room_members member
            JOIN local_agent_profiles profile
              ON profile.owner_user_id = member.owner_user_id
             AND profile.id = member.agent_id
            JOIN project_agent_rooms room
              ON room.owner_user_id = member.owner_user_id
             AND room.id = member.room_id
            WHERE member.owner_user_id = ?1 AND member.room_id = ?2
              AND member.status = 'active' AND profile.status = 'active'
            ORDER BY profile.name, profile.id
            "#,
        )
        .map_err(|_| team_todo_store_error())?;
    let rows = statement
        .query_map(params![owner_user_id, team_id], |row| {
            let agent_id = row.get::<_, String>(0)?;
            Ok(json!({
                "assignee_ref": todo_planning_reference(
                    owner_user_id,
                    run_id,
                    "assignee",
                    &agent_id,
                ),
                "name": row.get::<_, String>(1)?,
                "profession": row.get::<_, String>(2)?,
                "role": row.get::<_, String>(3)?,
                "is_project_manager": row.get::<_, i64>(4)? == 1,
                "is_current_agent": agent_id == current_agent_id,
            }))
        })
        .map_err(|_| team_todo_store_error())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| team_todo_store_error())
}

pub(super) fn todo_planning_response(
    owner_user_id: &str,
    run_id: &str,
    binding: &AgentAssetToolBinding,
    todo: &AgentTeamTodo,
    todos: &[AgentTeamTodo],
) -> Value {
    let dependencies = todo
        .dependency_ids
        .iter()
        .filter_map(|dependency_id| {
            todos
                .iter()
                .find(|candidate| candidate.id == *dependency_id)
        })
        .map(|dependency| {
            json!({
                "todo_ref": todo_planning_reference(
                    owner_user_id,
                    run_id,
                    "todo",
                    &dependency.id,
                ),
                "title": dependency.title,
                "assignee": dependency.agent_name,
                "status": dependency.status,
                "blocked_reason": dependency.blocked_reason,
                "result": dependency.result,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "todo_ref": todo_planning_reference(owner_user_id, run_id, "todo", &todo.id),
        "assignee": todo.agent_name,
        "assigned_to_current_agent": todo.agent_id == binding.agent_id,
        "title": todo.title,
        "detail": todo.detail,
        "objective": todo.execution_contract.objective,
        "scope": todo.execution_contract.scope,
        "expected_outputs": todo.execution_contract.expected_outputs,
        "acceptance_criteria": todo.execution_contract.acceptance_criteria,
        "constraints": todo.execution_contract.constraints,
        "priority": todo.priority,
        "status": todo.status,
        "blocked_reason": todo.blocked_reason,
        "result": todo.result,
        "builtin_capabilities": todo.execution_plan.builtin_capabilities,
        "plugins": todo.execution_plan.plugins.iter()
            .map(|plugin| plugin.display_name.clone())
            .collect::<Vec<_>>(),
        "dependencies": dependencies,
        "updated_at_unix_ms": todo.updated_at_unix_ms,
    })
}

pub(crate) fn todo_planning_reference(
    owner_user_id: &str,
    run_id: &str,
    kind: &str,
    identifier: &str,
) -> String {
    let digest = Sha256::digest(
        format!("chatos-todo-planning-ref-v1\n{owner_user_id}\n{run_id}\n{kind}\n{identifier}")
            .as_bytes(),
    );
    format!("{kind}_ref_{digest:x}")
}
