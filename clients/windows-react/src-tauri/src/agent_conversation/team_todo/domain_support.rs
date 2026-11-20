use super::*;

pub(super) fn validate_team_todo(todo: &AgentTeamTodo) -> Result<(), String> {
    validate_identifier(&todo.id).map_err(|_| team_todo_store_error())?;
    validate_agent_identifier(&todo.agent_id).map_err(|_| team_todo_store_error())?;
    validate_text(&todo.agent_name, 120, false).map_err(|_| team_todo_store_error())?;
    validate_identifier(&todo.team_id).map_err(|_| team_todo_store_error())?;
    if let Some(room_id) = todo.source_room_id.as_deref() {
        validate_identifier(room_id).map_err(|_| team_todo_store_error())?;
    }
    if let Some(message_id) = todo.source_message_id.as_deref() {
        validate_identifier(message_id).map_err(|_| team_todo_store_error())?;
    }
    validate_text(&todo.title, 500, false).map_err(|_| team_todo_store_error())?;
    validate_text(&todo.detail, 16_000, true).map_err(|_| team_todo_store_error())?;
    validate_text(&todo.blocked_reason, 8_000, true).map_err(|_| team_todo_store_error())?;
    validate_text(&todo.result, 16_000, true).map_err(|_| team_todo_store_error())?;
    if !(0..=100).contains(&todo.priority)
        || todo.sort_order < 0
        || !matches!(
            todo.status.as_str(),
            "pending" | "in_progress" | "blocked" | "completed" | "cancelled"
        )
        || todo.created_at_unix_ms < 0
        || todo.updated_at_unix_ms < todo.created_at_unix_ms
        || todo.dependency_ids.len() > 64
    {
        return Err(team_todo_store_error());
    }
    let mut dependencies = HashSet::new();
    for dependency_id in &todo.dependency_ids {
        validate_identifier(dependency_id).map_err(|_| team_todo_store_error())?;
        if dependency_id == &todo.id || !dependencies.insert(dependency_id) {
            return Err(team_todo_store_error());
        }
    }
    validate_execution_contract(&todo.execution_contract)?;
    validate_execution_plan(&todo.execution_plan)
}

pub(super) fn validate_execution_contract(
    contract: &AgentTodoExecutionContract,
) -> Result<(), String> {
    validate_text(&contract.objective, 8_000, false).map_err(|_| team_todo_store_error())?;
    validate_text(&contract.scope, 16_000, true).map_err(|_| team_todo_store_error())?;
    validate_contract_list(&contract.expected_outputs, false)?;
    validate_contract_list(&contract.acceptance_criteria, false)?;
    validate_contract_list(&contract.constraints, true)
}

pub(super) fn validate_contract_list(values: &[String], allow_empty: bool) -> Result<(), String> {
    if values.len() > 64 || (!allow_empty && values.is_empty()) {
        return Err(team_todo_store_error());
    }
    for value in values {
        validate_text(value, 4_000, false).map_err(|_| team_todo_store_error())?;
    }
    Ok(())
}

pub(super) fn validate_execution_plan(plan: &AgentTodoExecutionPlan) -> Result<(), String> {
    if plan.selected_at_unix_ms < 0
        || plan.builtin_capabilities.len() > TODO_BUILTIN_CAPABILITIES.len()
        || plan.plugins.len() > 32
    {
        return Err(team_todo_store_error());
    }
    validate_identifier(&plan.selection_revision).map_err(|_| team_todo_store_error())?;
    let mut capabilities = HashSet::new();
    for capability in &plan.builtin_capabilities {
        if !TODO_BUILTIN_CAPABILITIES.contains(&capability.as_str())
            || !capabilities.insert(capability.as_str())
            || (!plan.requires_execution && capability != "project_read")
        {
            return Err(team_todo_store_error());
        }
    }
    if capabilities.contains("project_write") && !capabilities.contains("project_read")
        || capabilities.contains("requirement_survey_write")
            && !capabilities.contains("requirement_survey_read")
    {
        return Err(team_todo_store_error());
    }
    let mut plugins = HashSet::new();
    for plugin in &plan.plugins {
        validate_identifier(&plugin.installation_id).map_err(|_| team_todo_store_error())?;
        validate_identifier(&plugin.plugin_id).map_err(|_| team_todo_store_error())?;
        validate_identifier(&plugin.component_key).map_err(|_| team_todo_store_error())?;
        validate_identifier(&plugin.component_revision).map_err(|_| team_todo_store_error())?;
        validate_identifier(&plugin.release_id).map_err(|_| team_todo_store_error())?;
        if !valid_release_digest(&plugin.release_digest) {
            return Err(team_todo_store_error());
        }
        validate_text(&plugin.manifest_version, 128, false).map_err(|_| team_todo_store_error())?;
        validate_text(&plugin.display_name, 240, false).map_err(|_| team_todo_store_error())?;
        validate_text(&plugin.reason, 1_000, true).map_err(|_| team_todo_store_error())?;
        if !plugins.insert((&plugin.plugin_id, &plugin.component_key)) {
            return Err(team_todo_store_error());
        }
    }
    Ok(())
}

pub(super) fn valid_release_digest(value: &str) -> bool {
    let digest = value.strip_prefix("sha256:").unwrap_or(value);
    digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(super) fn validate_todo_mutation(
    team_id: &str,
    agent_id: Option<&str>,
    title: &str,
    detail: &str,
    priority: i64,
    dependency_ids: &[String],
    now_unix_ms: i64,
) -> Result<(), String> {
    validate_identifier(team_id).map_err(|_| invalid_team_todo_input())?;
    if let Some(agent_id) = agent_id {
        validate_agent_identifier(agent_id).map_err(|_| invalid_team_todo_input())?;
    }
    validate_text(title, 500, false).map_err(|_| invalid_team_todo_input())?;
    validate_text(detail, 16_000, true).map_err(|_| invalid_team_todo_input())?;
    if !(0..=100).contains(&priority) || now_unix_ms < 0 {
        return Err(invalid_team_todo_input());
    }
    validate_identifier_list(dependency_ids, 64)
}

pub(super) fn validate_identifier_list(values: &[String], maximum: usize) -> Result<(), String> {
    if values.len() > maximum {
        return Err(invalid_team_todo_input());
    }
    let mut unique = HashSet::new();
    for value in values {
        validate_identifier(value).map_err(|_| invalid_team_todo_input())?;
        if !unique.insert(value) {
            return Err(invalid_team_todo_input());
        }
    }
    Ok(())
}

pub(super) fn normalized_contract(
    contract: Option<AgentTodoExecutionContract>,
    title: &str,
    detail: &str,
) -> AgentTodoExecutionContract {
    let mut contract = contract.unwrap_or(AgentTodoExecutionContract {
        objective: String::new(),
        scope: String::new(),
        expected_outputs: Vec::new(),
        acceptance_criteria: Vec::new(),
        constraints: Vec::new(),
    });
    if contract.objective.is_empty() {
        contract.objective = title.to_owned();
    }
    if contract.scope.is_empty() {
        contract.scope = detail.to_owned();
    }
    if contract.expected_outputs.is_empty() {
        contract.expected_outputs = vec![title.to_owned()];
    }
    if contract.acceptance_criteria.is_empty() {
        contract.acceptance_criteria = vec!["完成任务目标并输出可核验的结果总结。".into()];
    }
    contract
}

pub(super) fn default_execution_plan(now_unix_ms: i64) -> AgentTodoExecutionPlan {
    AgentTodoExecutionPlan {
        requires_execution: true,
        builtin_capabilities: vec!["project_read".into()],
        plugins: Vec::new(),
        selection_revision: "local-v1".into(),
        selected_at_unix_ms: now_unix_ms,
    }
}

pub(super) fn require_active_team_member(
    connection: &Connection,
    owner_user_id: &str,
    team_id: &str,
    agent_id: &str,
) -> Result<(), String> {
    let exists = connection
        .query_row(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM project_agent_room_members member
                JOIN local_agent_profiles profile
                  ON profile.owner_user_id = member.owner_user_id
                 AND profile.id = member.agent_id
                WHERE member.owner_user_id = ?1 AND member.room_id = ?2
                  AND member.agent_id = ?3 AND member.status = 'active'
                  AND profile.status = 'active'
            )
            "#,
            params![owner_user_id, team_id, agent_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|_| team_todo_store_error())?;
    if !exists {
        return Err("任务执行 Agent 不是当前团队的活跃成员。".into());
    }
    Ok(())
}

pub(super) fn validate_dependencies(
    connection: &Connection,
    owner_user_id: &str,
    team_id: &str,
    todo_id: Option<&str>,
    dependency_ids: &[String],
) -> Result<(), String> {
    for dependency_id in dependency_ids {
        if todo_id == Some(dependency_id.as_str()) {
            return Err("团队任务不能依赖自身。".into());
        }
        let exists = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM local_agent_todos WHERE owner_user_id = ?1 \
                 AND team_room_id = ?2 AND id = ?3)",
                params![owner_user_id, team_id, dependency_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|_| team_todo_store_error())?;
        if !exists {
            return Err("任务依赖项不属于当前团队。".into());
        }
    }
    if let Some(todo_id) = todo_id {
        for dependency_id in dependency_ids {
            if dependency_reaches(connection, owner_user_id, dependency_id, todo_id)? {
                return Err("任务依赖关系会形成循环。".into());
            }
        }
    }
    Ok(())
}

pub(super) fn dependency_reaches(
    connection: &Connection,
    owner_user_id: &str,
    start_id: &str,
    target_id: &str,
) -> Result<bool, String> {
    connection
        .query_row(
            r#"
            WITH RECURSIVE dependency_path(todo_id) AS (
                SELECT prerequisite_todo_id
                FROM local_agent_todo_dependencies
                WHERE owner_user_id = ?1 AND todo_id = ?2
                UNION
                SELECT dependency.prerequisite_todo_id
                FROM local_agent_todo_dependencies dependency
                JOIN dependency_path path ON path.todo_id = dependency.todo_id
                WHERE dependency.owner_user_id = ?1
            )
            SELECT EXISTS(SELECT 1 FROM dependency_path WHERE todo_id = ?3)
            "#,
            params![owner_user_id, start_id, target_id],
            |row| row.get(0),
        )
        .map_err(|_| team_todo_store_error())
}

pub(super) fn replace_dependencies(
    connection: &Connection,
    owner_user_id: &str,
    todo_id: &str,
    dependency_ids: &[String],
    now_unix_ms: i64,
) -> Result<(), String> {
    for dependency_id in dependency_ids {
        connection
            .execute(
                "INSERT INTO local_agent_todo_dependencies (owner_user_id, todo_id, \
                 prerequisite_todo_id, created_at_unix_ms) VALUES (?1, ?2, ?3, ?4)",
                params![owner_user_id, todo_id, dependency_id, now_unix_ms],
            )
            .map_err(|_| team_todo_store_error())?;
    }
    Ok(())
}

pub(super) fn todo_has_active_host_run(
    connection: &Connection,
    owner_user_id: &str,
    todo_id: &str,
) -> Result<bool, String> {
    connection
        .query_row(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM project_agent_deliveries delivery
                LEFT JOIN windows_agent_delivery_runs run
                  ON run.owner_user_id = delivery.owner_user_id
                 AND run.delivery_id = delivery.id
                WHERE delivery.owner_user_id = ?1 AND delivery.trigger_kind = 'todo'
                  AND (delivery.deduplication_key = ?2
                       OR delivery.deduplication_key LIKE ?3)
                  AND (delivery.status = 'running' OR run.status IN ('ready', 'running'))
            )
            "#,
            params![
                owner_user_id,
                format!("todo:{todo_id}"),
                format!("todo:{todo_id}:attempt:%"),
            ],
            |row| row.get(0),
        )
        .map_err(|_| team_todo_store_error())
}

pub(super) fn load_pending_run_control_intent(
    connection: &Connection,
    owner_user_id: &str,
    todo_id: &str,
    action: &str,
) -> Result<TeamRunControlIntent, String> {
    let intent = connection
        .query_row(
            r#"
            SELECT id, team_room_id, delivery_id, conversation_id, turn_id, run_id
            FROM windows_agent_run_control_journal
            WHERE owner_user_id = ?1 AND todo_id = ?2 AND action = ?3 AND status = 'pending'
            "#,
            params![owner_user_id, todo_id, action],
            |row| {
                Ok(TeamRunControlIntent {
                    id: row.get(0)?,
                    owner_user_id: owner_user_id.to_owned(),
                    action: action.to_owned(),
                    team_id: row.get(1)?,
                    todo_id: todo_id.to_owned(),
                    delivery_id: row.get(2)?,
                    conversation_id: row.get(3)?,
                    turn_id: row.get(4)?,
                    run_id: row.get(5)?,
                })
            },
        )
        .map_err(|_| team_todo_store_error())?;
    for value in [
        &intent.id,
        &intent.owner_user_id,
        &intent.action,
        &intent.team_id,
        &intent.todo_id,
        &intent.delivery_id,
        &intent.conversation_id,
        &intent.turn_id,
        &intent.run_id,
    ] {
        validate_identifier(value).map_err(|_| team_todo_store_error())?;
    }
    Ok(intent)
}

pub(super) fn team_todo_store_error() -> String {
    "团队任务数据不可用，请检查本机存储后重试。".into()
}

pub(super) fn invalid_team_todo_input() -> String {
    "团队任务内容无效，请检查后重试。".into()
}

pub(super) fn team_todo_conflict() -> String {
    "团队任务已被更新，请刷新后重试。".into()
}
