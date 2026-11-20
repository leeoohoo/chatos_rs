pub(super) async fn reconcile_task_after_run(
    connection: &mut SqliteConnection,
    run: &LocalAgentRunRecord,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    if run.owner_entity_type != "task"
        || (!run.status.is_terminal() && run.status != LocalAgentRunStatus::NeedsReview)
    {
        return Ok(());
    }
    // Read and validate the durable result gate before mutating the Task graph.
    // A blocked acceptance report is terminal only after every other criterion
    // has passed, and it intentionally leaves the Task blocked for explicit
    // human or external intervention instead of presenting false success.
    let reported_outcome = reported_task_outcome(connection, &run.run_id).await?;
    let result_status = reported_outcome
        .as_ref()
        .map(|outcome| outcome.result_status.clone());
    if result_status.as_deref() == Some("blocked")
        && run
            .input
            .get("acceptance_report_version")
            .and_then(Value::as_u64)
            == Some(1)
        && !reported_outcome.as_ref().is_some_and(|outcome| {
            task_acceptance_report_is_blocked(&run.input, &outcome.report).unwrap_or(false)
        })
    {
        return Err(ClientStorageError::InvalidState(
            "Task cannot become blocked while actionable acceptance checks remain".to_string(),
        ));
    }
    let task_status = match run.status {
        LocalAgentRunStatus::Succeeded if result_status.as_deref() == Some("blocked") => "blocked",
        LocalAgentRunStatus::Succeeded => "succeeded",
        LocalAgentRunStatus::Failed => "failed",
        LocalAgentRunStatus::Cancelled => "cancelled",
        // A model claim can expire after its Host disappears. The Run keeps the more
        // precise needs_review state, while the owning Task becomes blocked so it cannot
        // remain falsely active and can be explicitly retried by the user.
        LocalAgentRunStatus::NeedsReview => "blocked",
        _ => return Ok(()),
    };
    let updated = sqlx::query(
        "UPDATE local_tasks SET status = ?, active_run_id = NULL, version = version + 1, \
         updated_at_unix_ms = ? WHERE task_id = ? AND status IN ('ready','running') \
         AND active_run_id = ?",
    )
    .bind(task_status)
    .bind(now_unix_ms)
    .bind(&run.owner_entity_id)
    .bind(&run.run_id)
    .execute(&mut *connection)
    .await
    .db()?;
    if updated.rows_affected() != 1 {
        return Err(ClientStorageError::Conflict(format!(
            "task run ownership changed: {}",
            run.owner_entity_id
        )));
    }
    let graph_id: String = sqlx::query_scalar("SELECT graph_id FROM local_tasks WHERE task_id = ?")
        .bind(&run.owner_entity_id)
        .fetch_one(&mut *connection)
        .await
        .db()?;
    propagate_blocked(connection, &graph_id, now_unix_ms)
        .await
        .db()?;
    unlock_satisfied(connection, &graph_id, now_unix_ms)
        .await
        .db()?;
    SqliteClientStorage::insert_event(
        connection,
        &format!("task-reconciled:{}:{}", run.run_id, run.version),
        &run.run_id,
        "task_state_reconciled",
        &serde_json::json!({
            "graph_id": graph_id,
            "task_id": run.owner_entity_id,
            "status": task_status,
            "result_status": result_status
        }),
        now_unix_ms,
    )
    .await
    .db()?;
    Ok(())
}

pub(super) async fn reported_task_outcome(
    connection: &mut SqliteConnection,
    run_id: &str,
) -> Result<Option<ReportedTaskOutcome>, ClientStorageError> {
    let row = sqlx::query(
        "SELECT result_json, arguments_json FROM local_agent_tool_invocations \
         WHERE run_id = ? AND tool_name = ? AND status = 'succeeded' \
         ORDER BY updated_at_unix_ms DESC, invocation_id DESC LIMIT 1",
    )
    .bind(run_id)
    .bind(TASK_OUTCOME_REPORT_TOOL)
    .fetch_optional(&mut *connection)
    .await
    .db()?;
    let Some(row) = row else {
        return Ok(None);
    };
    let result_json: Option<String> = row.try_get("result_json").db()?;
    let arguments_json: String = row.try_get("arguments_json").db()?;
    let result = result_json
        .as_deref()
        .map(serde_json::from_str::<Value>)
        .transpose()?;
    // The tool executor is the policy boundary for the internal result. Consume
    // its accepted result instead of replaying raw model arguments. Legacy rows
    // that predate a structured result retain the former arguments fallback.
    let value = result
        .filter(|value| value.get("reported").and_then(Value::as_bool) == Some(true))
        .unwrap_or(serde_json::from_str(&arguments_json)?);
    let result_status = value
        .get("result_status")
        .or_else(|| value.get("status"))
        .and_then(Value::as_str)
        .filter(|status| matches!(*status, "succeeded" | "failed" | "blocked"))
        .ok_or_else(|| {
            ClientStorageError::InvalidState(
                "stored Task outcome report has an invalid result_status".to_string(),
            )
        })?;
    let reason = value
        .get("reason")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|reason| !reason.is_empty())
        .ok_or_else(|| {
            ClientStorageError::InvalidState(
                "stored Task outcome report has an empty reason".to_string(),
            )
        })?;
    Ok(Some(ReportedTaskOutcome {
        result_status: result_status.to_string(),
        reason: reason.to_string(),
        report: value,
    }))
}

pub(super) async fn propagate_blocked(
    connection: &mut SqliteConnection,
    graph_id: &str,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    loop {
        let updated = sqlx::query(PROPAGATE_BLOCKED_SQL)
            .bind(now_unix_ms)
            .bind(graph_id)
            .execute(&mut *connection)
            .await
            .db()?;
        if updated.rows_affected() == 0 {
            return Ok(());
        }
    }
}

pub(super) async fn unlock_satisfied(
    connection: &mut SqliteConnection,
    graph_id: &str,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    sqlx::query(UNLOCK_SATISFIED_SQL)
        .bind(now_unix_ms)
        .bind(graph_id)
        .execute(&mut *connection)
        .await
        .db()?;
    Ok(())
}
