use super::*;

impl AgentConversationStore {
    pub(crate) fn create_todo_from_planning_tool(
        &self,
        owner_user_id: &str,
        run_id: &str,
        input: TodoPlanningCreateInput,
        now_unix_ms: i64,
    ) -> Result<(AgentAssetToolBinding, Value), String> {
        validate_identifier(run_id).map_err(|_| team_todo_store_error())?;
        validate_text(&input.request_key, 512, false).map_err(|_| invalid_team_todo_input())?;
        validate_text(&input.team_reference, 600, false).map_err(|_| invalid_team_todo_input())?;
        validate_text(&input.assignee_reference, 600, false)
            .map_err(|_| invalid_team_todo_input())?;
        validate_text(&input.title, 500, false).map_err(|_| invalid_team_todo_input())?;
        validate_text(&input.detail, 16_000, true).map_err(|_| invalid_team_todo_input())?;
        if !(0..=100).contains(&input.priority) || now_unix_ms < 0 {
            return Err(invalid_team_todo_input());
        }
        validate_reference_list(&input.dependency_references, 64)?;
        validate_execution_contract(&input.execution_contract)
            .map_err(|_| invalid_team_todo_input())?;
        let capabilities = normalized_planning_capabilities(
            &input.builtin_capabilities,
            input.requires_execution,
        )?;
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_todo_store_error())?;
        let (binding, source_message_id) =
            require_todo_planning_manager_tool_binding(&transaction, owner_user_id, run_id)?;
        let expected_team_reference =
            todo_planning_reference(owner_user_id, run_id, "team", &binding.team_id);
        if input.team_reference != expected_team_reference {
            return Err("The team reference is unavailable for this Run.".into());
        }
        let assignee_id = resolve_assignee_reference(
            &transaction,
            owner_user_id,
            run_id,
            &binding.team_id,
            &input.assignee_reference,
        )?;
        let dependency_ids = resolve_todo_references(
            &transaction,
            owner_user_id,
            run_id,
            &binding.team_id,
            &input.dependency_references,
            false,
        )?;
        validate_dependencies(
            &transaction,
            owner_user_id,
            &binding.team_id,
            None,
            &dependency_ids,
        )?;
        if let Some(existing_id) = transaction
            .query_row(
                "SELECT id FROM local_agent_todos WHERE owner_user_id = ?1 \
                 AND agent_id = ?2 AND request_key = ?3",
                params![owner_user_id, assignee_id, input.request_key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|_| team_todo_store_error())?
        {
            transaction.commit().map_err(|_| team_todo_store_error())?;
            let (_, snapshot) = self.todo_planning_tool_snapshot(owner_user_id, run_id, true)?;
            let todo = planning_snapshot_todo(&snapshot, owner_user_id, run_id, &existing_id)?;
            return Ok((binding, todo));
        }
        let sort_order = transaction
            .query_row(
                "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM local_agent_todos \
                 WHERE owner_user_id = ?1 AND agent_id = ?2",
                params![owner_user_id, assignee_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|_| team_todo_store_error())?;
        let todo_id = identifier("todo");
        let execution_plan = AgentTodoExecutionPlan {
            requires_execution: input.requires_execution,
            builtin_capabilities: capabilities,
            plugins: input.plugins,
            selection_revision: "windows-host-todo-planning-v2".into(),
            selected_at_unix_ms: now_unix_ms,
        };
        validate_execution_plan(&execution_plan).map_err(|_| invalid_team_todo_input())?;
        let execution_plan_json =
            serde_json::to_string(&execution_plan).map_err(|_| team_todo_store_error())?;
        let execution_contract_json = serde_json::to_string(&input.execution_contract)
            .map_err(|_| team_todo_store_error())?;
        transaction
            .execute(
                r#"
                INSERT INTO local_agent_todos (
                    owner_user_id, id, agent_id, team_room_id, source_room_id,
                    source_message_id, request_key, title, detail, priority, sort_order,
                    status, blocked_reason, result, created_at_unix_ms, updated_at_unix_ms,
                    execution_plan_json, execution_contract_json
                ) VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                          'pending', '', '', ?11, ?11, ?12, ?13)
                "#,
                params![
                    owner_user_id,
                    todo_id,
                    assignee_id,
                    binding.team_id,
                    source_message_id,
                    input.request_key,
                    input.title,
                    input.detail,
                    input.priority,
                    sort_order,
                    now_unix_ms,
                    execution_plan_json,
                    execution_contract_json,
                ],
            )
            .map_err(|_| team_todo_store_error())?;
        replace_dependencies(
            &transaction,
            owner_user_id,
            &todo_id,
            &dependency_ids,
            now_unix_ms,
        )?;
        transaction
            .execute(
                r#"
                INSERT INTO local_agent_todo_sources (
                    owner_user_id, todo_id, conversation_id, message_id, relation,
                    created_at_unix_ms
                ) VALUES (?1, ?2, ?3, ?4, 'created', ?5)
                "#,
                params![
                    owner_user_id,
                    todo_id,
                    binding.team_id,
                    source_message_id,
                    now_unix_ms,
                ],
            )
            .map_err(|_| team_todo_store_error())?;
        transaction.commit().map_err(|_| team_todo_store_error())?;
        let (_, snapshot) = self.todo_planning_tool_snapshot(owner_user_id, run_id, true)?;
        let todo = planning_snapshot_todo(&snapshot, owner_user_id, run_id, &todo_id)?;
        Ok((binding, todo))
    }

    pub(crate) fn update_todo_from_planning_tool(
        &self,
        owner_user_id: &str,
        run_id: &str,
        input: TodoPlanningUpdateInput,
        now_unix_ms: i64,
    ) -> Result<(AgentAssetToolBinding, Value), String> {
        validate_identifier(run_id).map_err(|_| team_todo_store_error())?;
        validate_text(&input.todo_reference, 600, false).map_err(|_| invalid_team_todo_input())?;
        if now_unix_ms < 0 {
            return Err(invalid_team_todo_input());
        }
        if let Some(title) = input.title.as_deref() {
            validate_text(title, 500, false).map_err(|_| invalid_team_todo_input())?;
        }
        if let Some(detail) = input.detail.as_deref() {
            validate_text(detail, 16_000, true).map_err(|_| invalid_team_todo_input())?;
        }
        if input
            .priority
            .is_some_and(|priority| !(0..=100).contains(&priority))
        {
            return Err(invalid_team_todo_input());
        }
        if let Some(status) = input.status.as_deref() {
            if !matches!(status, "pending" | "cancelled") {
                return Err("Todo execution status is owned by its executor Run.".into());
            }
        }
        if let Some(references) = input.dependency_references.as_deref() {
            validate_reference_list(references, 64)?;
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_todo_store_error())?;
        let (binding, source_message_id) =
            require_todo_planning_manager_tool_binding(&transaction, owner_user_id, run_id)?;
        let todo_id = resolve_todo_references(
            &transaction,
            owner_user_id,
            run_id,
            &binding.team_id,
            std::slice::from_ref(&input.todo_reference),
            false,
        )?
        .pop()
        .ok_or_else(team_todo_store_error)?;
        let existing: (String, String, i64, String, String) = transaction
            .query_row(
                "SELECT title, detail, priority, status, execution_contract_json \
                 FROM local_agent_todos WHERE owner_user_id = ?1 AND team_room_id = ?2 AND id = ?3",
                params![owner_user_id, binding.team_id, todo_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .map_err(|_| team_todo_store_error())?;
        if matches!(existing.3.as_str(), "completed" | "cancelled") {
            return Err("Terminal Todos cannot be changed by the planning thread.".into());
        }
        if existing.3 == "in_progress" {
            return Err(
                "The Todo is executing; stop it through the durable Run control before editing."
                    .into(),
            );
        }
        if input.status.as_deref() == Some("pending") && existing.3 != "blocked" {
            return Err("Only a blocked Todo can be reopened as pending.".into());
        }
        if input.status.as_deref() == Some("cancelled")
            && !matches!(existing.3.as_str(), "pending" | "blocked")
        {
            return Err("Only a pending or blocked Todo can be cancelled.".into());
        }
        if todo_has_active_host_run(&transaction, owner_user_id, &todo_id)? {
            return Err("The Todo still has an active Host Run.".into());
        }
        let mut contract: AgentTodoExecutionContract =
            serde_json::from_str(&existing.4).map_err(|_| team_todo_store_error())?;
        if let Some(value) = input.objective {
            contract.objective = value;
        }
        if let Some(value) = input.scope {
            contract.scope = value;
        }
        if let Some(value) = input.expected_outputs {
            contract.expected_outputs = value;
        }
        if let Some(value) = input.acceptance_criteria {
            contract.acceptance_criteria = value;
        }
        if let Some(value) = input.constraints {
            contract.constraints = value;
        }
        validate_execution_contract(&contract).map_err(|_| invalid_team_todo_input())?;
        let dependency_ids = if let Some(references) = input.dependency_references.as_deref() {
            let resolved = resolve_todo_references(
                &transaction,
                owner_user_id,
                run_id,
                &binding.team_id,
                references,
                false,
            )?;
            validate_dependencies(
                &transaction,
                owner_user_id,
                &binding.team_id,
                Some(&todo_id),
                &resolved,
            )?;
            Some(resolved)
        } else {
            None
        };
        let target_status = input.status.as_deref().unwrap_or(&existing.3);
        let title = input.title.unwrap_or(existing.0);
        let detail = input.detail.unwrap_or(existing.1);
        let priority = input.priority.unwrap_or(existing.2);
        let contract_json =
            serde_json::to_string(&contract).map_err(|_| team_todo_store_error())?;
        let updated_at_unix_ms: i64 = transaction
            .query_row(
                "SELECT CASE WHEN updated_at_unix_ms >= ?1 THEN updated_at_unix_ms + 1 ELSE ?1 END \
                 FROM local_agent_todos WHERE owner_user_id = ?2 AND id = ?3",
                params![now_unix_ms, owner_user_id, todo_id],
                |row| row.get(0),
            )
            .map_err(|_| team_todo_store_error())?;
        let changed = transaction
            .execute(
                r#"
                UPDATE local_agent_todos
                SET title = ?1, detail = ?2, priority = ?3, status = ?4,
                    blocked_reason = CASE WHEN ?4 = 'pending' THEN '' ELSE blocked_reason END,
                    result = CASE WHEN ?4 = 'pending' THEN '' ELSE result END,
                    execution_contract_json = ?5, updated_at_unix_ms = ?6
                WHERE owner_user_id = ?7 AND team_room_id = ?8 AND id = ?9
                  AND status IN ('pending', 'blocked')
                "#,
                params![
                    title,
                    detail,
                    priority,
                    target_status,
                    contract_json,
                    updated_at_unix_ms,
                    owner_user_id,
                    binding.team_id,
                    todo_id,
                ],
            )
            .map_err(|_| team_todo_store_error())?;
        if changed != 1 {
            return Err(team_todo_conflict());
        }
        if let Some(dependency_ids) = dependency_ids {
            transaction
                .execute(
                    "DELETE FROM local_agent_todo_dependencies \
                     WHERE owner_user_id = ?1 AND todo_id = ?2",
                    params![owner_user_id, todo_id],
                )
                .map_err(|_| team_todo_store_error())?;
            replace_dependencies(
                &transaction,
                owner_user_id,
                &todo_id,
                &dependency_ids,
                updated_at_unix_ms,
            )?;
        }
        let relation = if input.priority.is_some() {
            "reprioritized"
        } else {
            "updated"
        };
        transaction
            .execute(
                r#"
                INSERT OR IGNORE INTO local_agent_todo_sources (
                    owner_user_id, todo_id, conversation_id, message_id, relation,
                    created_at_unix_ms
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                "#,
                params![
                    owner_user_id,
                    todo_id,
                    binding.team_id,
                    source_message_id,
                    relation,
                    updated_at_unix_ms,
                ],
            )
            .map_err(|_| team_todo_store_error())?;
        if target_status == "cancelled" {
            insert_todo_progress(
                &transaction,
                owner_user_id,
                &todo_id,
                run_id,
                "cancelled",
                "cancelled",
                "项目经理已停止该任务。",
                &[],
                updated_at_unix_ms,
            )?;
            transaction
                .execute(
                    r#"
                    UPDATE project_agent_deliveries
                    SET status = 'cancelled', last_error = 'Todo 已由项目经理取消。',
                        completed_at_unix_ms = ?1
                    WHERE owner_user_id = ?2 AND trigger_kind = 'todo' AND status = 'pending'
                      AND (deduplication_key = ?3 OR deduplication_key LIKE ?4)
                    "#,
                    params![
                        updated_at_unix_ms,
                        owner_user_id,
                        format!("todo:{todo_id}"),
                        format!("todo:{todo_id}:attempt:%"),
                    ],
                )
                .map_err(|_| team_todo_store_error())?;
        }
        transaction.commit().map_err(|_| team_todo_store_error())?;
        let (_, snapshot) = self.todo_planning_tool_snapshot(owner_user_id, run_id, true)?;
        let todo = planning_snapshot_todo(&snapshot, owner_user_id, run_id, &todo_id)?;
        Ok((binding, todo))
    }

    pub(crate) fn reorder_todos_from_planning_tool(
        &self,
        owner_user_id: &str,
        run_id: &str,
        todo_references: &[String],
        now_unix_ms: i64,
    ) -> Result<(AgentAssetToolBinding, Value), String> {
        validate_identifier(run_id).map_err(|_| team_todo_store_error())?;
        validate_reference_list(todo_references, 500)?;
        if todo_references.is_empty() || now_unix_ms < 0 {
            return Err(invalid_team_todo_input());
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_todo_store_error())?;
        let (binding, _) =
            require_todo_planning_manager_tool_binding(&transaction, owner_user_id, run_id)?;
        let requested_ids = resolve_todo_references(
            &transaction,
            owner_user_id,
            run_id,
            &binding.team_id,
            todo_references,
            true,
        )?;
        let active_ids = {
            let mut statement = transaction
                .prepare(
                    "SELECT id FROM local_agent_todos WHERE owner_user_id = ?1 \
                     AND team_room_id = ?2 AND status NOT IN ('completed', 'cancelled') \
                     ORDER BY priority DESC, sort_order, created_at_unix_ms, id",
                )
                .map_err(|_| team_todo_store_error())?;
            let rows = statement
                .query_map(params![owner_user_id, binding.team_id], |row| {
                    row.get::<_, String>(0)
                })
                .map_err(|_| team_todo_store_error())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|_| team_todo_store_error())?
        };
        let requested = requested_ids.iter().collect::<HashSet<_>>();
        let ordered = requested_ids
            .iter()
            .cloned()
            .chain(
                active_ids
                    .into_iter()
                    .filter(|todo_id| !requested.contains(todo_id)),
            )
            .collect::<Vec<_>>();
        for (offset, todo_id) in ordered.iter().enumerate() {
            let priority = 100_i64.saturating_sub(offset as i64).max(0);
            let changed = transaction
                .execute(
                    r#"
                    UPDATE local_agent_todos
                    SET priority = ?1, sort_order = ?2,
                        updated_at_unix_ms = CASE
                          WHEN updated_at_unix_ms >= ?3 THEN updated_at_unix_ms + 1
                          ELSE ?3
                        END
                    WHERE owner_user_id = ?4 AND team_room_id = ?5 AND id = ?6
                      AND status NOT IN ('completed', 'cancelled')
                    "#,
                    params![
                        priority,
                        offset as i64,
                        now_unix_ms,
                        owner_user_id,
                        binding.team_id,
                        todo_id,
                    ],
                )
                .map_err(|_| team_todo_store_error())?;
            if changed != 1 {
                return Err(team_todo_conflict());
            }
        }
        transaction.commit().map_err(|_| team_todo_store_error())?;
        let (_, snapshot) = self.todo_planning_tool_snapshot(owner_user_id, run_id, false)?;
        Ok((binding, json!({"todos": snapshot["todos"].clone()})))
    }
}
