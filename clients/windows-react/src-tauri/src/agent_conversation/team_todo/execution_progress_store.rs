use super::*;

impl AgentConversationStore {
    pub(crate) fn todo_execution_tool_context(
        &self,
        owner_user_id: &str,
        run_id: &str,
    ) -> Result<(AgentAssetToolBinding, Value), String> {
        validate_identifier(run_id).map_err(|_| team_todo_store_error())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let binding = load_agent_asset_tool_binding(&connection, owner_user_id, run_id)?;
        let todo_id = binding
            .todo_id
            .as_deref()
            .ok_or_else(team_todo_store_error)?;
        let todo = self.load_project_team_todo(owner_user_id, &binding.team_id, todo_id)?;
        if todo.agent_id != binding.agent_id || todo.status != "in_progress" {
            return Err(team_todo_conflict());
        }
        let progress = load_todo_progress(&connection, owner_user_id, todo_id, 50)?;
        let prerequisites =
            load_todo_prerequisites(&connection, owner_user_id, &binding.team_id, todo_id)?;
        let team_assets = load_todo_asset_summaries(&connection, owner_user_id, todo_id)?;
        let output = json!({
            "title": todo.title,
            "detail": todo.detail,
            "objective": todo.execution_contract.objective,
            "scope": todo.execution_contract.scope,
            "expected_outputs": todo.execution_contract.expected_outputs,
            "acceptance_criteria": todo.execution_contract.acceptance_criteria,
            "constraints": todo.execution_contract.constraints,
            "priority": todo.priority,
            "builtin_capabilities": todo.execution_plan.builtin_capabilities,
            "plugins": todo.execution_plan.plugins.iter().map(|plugin| plugin.display_name.clone()).collect::<Vec<_>>(),
            "source_messages": [],
            "prerequisites": prerequisites,
            "team_assets": team_assets,
            "progress": progress,
        });
        Ok((binding, output))
    }

    pub(crate) fn append_todo_execution_progress(
        &self,
        owner_user_id: &str,
        run_id: &str,
        stage: &str,
        detail: &str,
        now_unix_ms: i64,
    ) -> Result<(AgentAssetToolBinding, Value), String> {
        validate_identifier(run_id).map_err(|_| team_todo_store_error())?;
        validate_text(stage, 240, true).map_err(|_| invalid_team_todo_input())?;
        validate_text(detail, 16_000, false).map_err(|_| invalid_team_todo_input())?;
        if now_unix_ms < 0 {
            return Err(invalid_team_todo_input());
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_todo_store_error())?;
        let binding = load_agent_asset_tool_binding(&transaction, owner_user_id, run_id)?;
        let todo_id =
            require_running_todo_tool_binding(&transaction, owner_user_id, run_id, &binding)?;
        let progress = insert_todo_progress(
            &transaction,
            owner_user_id,
            &todo_id,
            run_id,
            "progress",
            stage,
            detail,
            &[],
            now_unix_ms,
        )?;
        transaction.commit().map_err(|_| team_todo_store_error())?;
        Ok((binding, progress))
    }

    pub(crate) fn finish_todo_execution(
        &self,
        owner_user_id: &str,
        run_id: &str,
        completed: bool,
        detail: &str,
        asset_update_suggestions: &[AgentTeamAssetUpdateSuggestion],
        project_write_evidence_verified: bool,
        now_unix_ms: i64,
    ) -> Result<(AgentAssetToolBinding, Value), String> {
        validate_identifier(run_id).map_err(|_| team_todo_store_error())?;
        validate_text(detail, if completed { 16_000 } else { 8_000 }, false)
            .map_err(|_| invalid_team_todo_input())?;
        if now_unix_ms < 0 {
            return Err(invalid_team_todo_input());
        }
        validate_asset_update_suggestions(asset_update_suggestions)?;
        if !completed && !asset_update_suggestions.is_empty() {
            return Err(invalid_team_asset_review_input());
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_todo_store_error())?;
        let binding = load_agent_asset_tool_binding(&transaction, owner_user_id, run_id)?;
        let todo_id = binding
            .todo_id
            .as_deref()
            .ok_or_else(team_todo_store_error)?;
        let (agent_id, status, plan_json): (String, String, String) = transaction
            .query_row(
                "SELECT agent_id, status, execution_plan_json FROM local_agent_todos \
                 WHERE owner_user_id = ?1 AND team_room_id = ?2 AND id = ?3",
                params![owner_user_id, binding.team_id, todo_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|_| team_todo_store_error())?;
        if agent_id != binding.agent_id {
            return Err(team_todo_conflict());
        }
        let plan: AgentTodoExecutionPlan =
            serde_json::from_str(&plan_json).map_err(|_| team_todo_store_error())?;
        validate_execution_plan(&plan)?;
        if completed
            && plan
                .builtin_capabilities
                .iter()
                .any(|capability| capability == "project_write")
            && !project_write_evidence_verified
        {
            return Err("当前运行缺少事务提交后的项目读取回执；请先提交并重新读取文件。".into());
        }
        let target_status = if completed { "completed" } else { "blocked" };
        if status == target_status {
            let progress = load_todo_progress(&transaction, owner_user_id, todo_id, 1)?
                .into_iter()
                .next()
                .ok_or_else(team_todo_store_error)?;
            transaction.commit().map_err(|_| team_todo_store_error())?;
            return Ok((
                binding,
                json!({"todo_status": target_status, "progress": progress}),
            ));
        }
        require_running_todo_tool_binding(&transaction, owner_user_id, run_id, &binding)?;
        let progress = insert_todo_progress(
            &transaction,
            owner_user_id,
            todo_id,
            run_id,
            target_status,
            target_status,
            detail,
            asset_update_suggestions,
            now_unix_ms,
        )?;
        let changed = transaction
            .execute(
                r#"
                UPDATE local_agent_todos
                SET status = ?1,
                    blocked_reason = CASE WHEN ?1 = 'blocked' THEN ?2 ELSE '' END,
                    result = CASE WHEN ?1 = 'completed' THEN ?2 ELSE '' END,
                    updated_at_unix_ms = CASE
                      WHEN updated_at_unix_ms >= ?3 THEN updated_at_unix_ms + 1 ELSE ?3 END
                WHERE owner_user_id = ?4 AND team_room_id = ?5 AND id = ?6
                  AND agent_id = ?7 AND status = 'in_progress'
                "#,
                params![
                    target_status,
                    detail,
                    now_unix_ms,
                    owner_user_id,
                    binding.team_id,
                    todo_id,
                    binding.agent_id,
                ],
            )
            .map_err(|_| team_todo_store_error())?;
        if changed != 1 {
            return Err(team_todo_conflict());
        }
        transaction.commit().map_err(|_| team_todo_store_error())?;
        Ok((
            binding,
            json!({"todo_status": target_status, "progress": progress}),
        ))
    }
}
