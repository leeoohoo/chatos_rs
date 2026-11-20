use super::*;

pub(super) fn reconcile_todo_from_delivery(
    connection: &Connection,
    owner_user_id: &str,
    update: &TeamRunReconciliation,
) -> Result<(), String> {
    let requires_project_write =
        delivery_todo_requires_project_write(connection, owner_user_id, &update.delivery_id)?;
    let has_terminal_receipt = todo_has_terminal_receipt(
        connection,
        owner_user_id,
        &update.delivery_id,
        &update.run_id,
    )?;
    let (status, blocked_reason, result) = match update.status {
        LocalConversationTurnStatus::Running => return Ok(()),
        LocalConversationTurnStatus::Succeeded
            if requires_project_write && !update.project_write_evidence_verified =>
        {
            (
                "blocked",
                "Host 已结束运行，但缺少事务提交后的项目读取回执；任务没有被伪装为已完成。",
                "",
            )
        }
        LocalConversationTurnStatus::Succeeded if !has_terminal_receipt => (
            "blocked",
            "Host 已结束运行，但执行线程没有调用 todo_complete 或 todo_block 保存终态回执。",
            "",
        ),
        LocalConversationTurnStatus::Succeeded => (
            "completed",
            "",
            "Todo 已由 Local Agent Host 完成；详细过程保留在运行详情中。",
        ),
        LocalConversationTurnStatus::Failed => (
            "blocked",
            "Local Agent Host 执行失败，请在运行详情中检查失败记录。",
            "",
        ),
        LocalConversationTurnStatus::Cancelled => (
            "blocked",
            "Local Agent Host 运行已取消；任务保留为阻塞，等待明确重试或取消。",
            "",
        ),
    };
    connection
        .execute(
            r#"
            UPDATE local_agent_todos
            SET status = ?1, blocked_reason = ?2, result = ?3,
                updated_at_unix_ms = CASE
                  WHEN updated_at_unix_ms >= ?4 THEN updated_at_unix_ms + 1
                  ELSE ?4
                END
            WHERE owner_user_id = ?5 AND status = 'in_progress'
              AND EXISTS (
                SELECT 1 FROM project_agent_deliveries delivery
                WHERE delivery.owner_user_id = local_agent_todos.owner_user_id
                  AND delivery.id = ?6 AND delivery.trigger_kind = 'todo'
                  AND delivery.target_agent_id = local_agent_todos.agent_id
                  AND (delivery.deduplication_key = 'todo:' || local_agent_todos.id
                       OR delivery.deduplication_key LIKE
                          'todo:' || local_agent_todos.id || ':attempt:%')
              )
            "#,
            params![
                status,
                blocked_reason,
                result,
                update.updated_at_unix_ms,
                owner_user_id,
                update.delivery_id,
            ],
        )
        .map_err(|_| team_todo_store_error())?;
    Ok(())
}

pub(super) fn delivery_todo_requires_project_write(
    connection: &Connection,
    owner_user_id: &str,
    delivery_id: &str,
) -> Result<bool, String> {
    let plan = connection
        .query_row(
            r#"
            SELECT todo.execution_plan_json
            FROM local_agent_todos todo
            JOIN project_agent_deliveries delivery
              ON delivery.owner_user_id = todo.owner_user_id
             AND delivery.target_agent_id = todo.agent_id
             AND delivery.trigger_kind = 'todo'
             AND (delivery.deduplication_key = 'todo:' || todo.id
                  OR delivery.deduplication_key LIKE 'todo:' || todo.id || ':attempt:%')
            WHERE todo.owner_user_id = ?1 AND delivery.id = ?2
            LIMIT 1
            "#,
            params![owner_user_id, delivery_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|_| team_todo_store_error())?;
    let Some(plan) = plan else { return Ok(false) };
    let plan: AgentTodoExecutionPlan =
        serde_json::from_str(&plan).map_err(|_| team_todo_store_error())?;
    validate_execution_plan(&plan).map_err(|_| team_todo_store_error())?;
    Ok(plan.requires_execution
        && plan
            .builtin_capabilities
            .iter()
            .any(|capability| capability == "project_write"))
}
