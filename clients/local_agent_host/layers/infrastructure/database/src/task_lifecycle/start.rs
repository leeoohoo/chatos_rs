pub(super) struct ReportedTaskOutcome {
    pub result_status: String,
    pub reason: String,
    report: Value,
}

pub(super) async fn start_next_task_run(
    connection: &mut SqliteConnection,
    owner_user_id: &str,
    graph_id: Option<&str>,
    run_id: &str,
    event_id: &str,
    now_unix_ms: i64,
) -> Result<Option<LocalAgentRunRecord>, ClientStorageError> {
    // Recover pending Tasks whose external prerequisites had already completed
    // before creation (including records left by older Hosts).
    let owner_graph: Option<String> = sqlx::query_scalar(
        "SELECT graph_id FROM local_task_graphs WHERE owner_user_id = ? LIMIT 1",
    )
    .bind(owner_user_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?;
    if let Some(owner_graph) = owner_graph {
        propagate_blocked(connection, &owner_graph, now_unix_ms).await?;
        unlock_satisfied(connection, &owner_graph, now_unix_ms).await?;
    }
    let candidates = sqlx::query(
        "SELECT t.task_id, t.graph_id, g.owner_user_id, t.profile_key, \
         t.model_config_ref, t.model_config_revision, t.capability_policy_revision, \
         t.input_json, t.max_iterations, t.version, \
         CASE \
           WHEN source_conversation.resource_kind = 'project' \
             AND source_conversation.resource_id IS NOT NULL \
             THEN 'project:' || source_conversation.resource_id \
           WHEN turn_conversation.resource_kind = 'project' \
             AND turn_conversation.resource_id IS NOT NULL \
             THEN 'project:' || turn_conversation.resource_id \
           WHEN source_conversation.conversation_id IS NOT NULL \
             THEN 'conversation:' || source_conversation.conversation_id \
           WHEN turn_conversation.conversation_id IS NOT NULL \
             THEN 'conversation:' || turn_conversation.conversation_id \
           ELSE g.source_entity_type || ':' || g.source_entity_id \
         END AS write_scope_key FROM local_tasks t \
         JOIN local_task_graphs g ON g.graph_id = t.graph_id \
         LEFT JOIN local_conversations source_conversation \
           ON g.source_entity_type = 'conversation' \
          AND source_conversation.conversation_id = g.source_entity_id \
         LEFT JOIN local_conversation_turns source_turn \
           ON g.source_entity_type = 'conversation_turn' \
          AND source_turn.turn_id = g.source_entity_id \
         LEFT JOIN local_conversations turn_conversation \
           ON turn_conversation.conversation_id = source_turn.conversation_id \
         WHERE g.owner_user_id = ? AND (? IS NULL OR t.graph_id = ?) \
         AND t.status = 'ready' AND t.active_run_id IS NULL \
         AND NOT EXISTS (SELECT 1 FROM local_task_followup_reviews f \
           WHERE f.task_id = t.task_id AND f.state IN ('pending','held')) \
         AND (json_extract(t.input_json, '$.schedule.run_at_unix_ms') IS NULL \
              OR json_extract(t.input_json, '$.schedule.run_at_unix_ms') <= ?) \
         ORDER BY t.created_at_unix_ms, t.task_id",
    )
    .bind(owner_user_id)
    .bind(graph_id)
    .bind(graph_id)
    .bind(now_unix_ms)
    .fetch_all(&mut *connection)
    .await
    .db()?;
    let active_write_scopes = active_write_scopes(connection, owner_user_id).await?;
    let mut candidate = None;
    for row in candidates {
        let task_id: String = row.try_get("task_id").db()?;
        let input_json: String = row.try_get("input_json").db()?;
        let input: Value = serde_json::from_str(&input_json)?;
        let write_scope = local_task_runtime_write_scope(&input, &task_id)
            .map_err(ClientStorageError::InvalidState)?;
        let scope_key: String = row.try_get("write_scope_key").db()?;
        let blocked = write_scope.as_ref().is_some_and(|paths| {
            active_write_scopes.iter().any(|active| {
                active.scope_key == scope_key
                    && local_task_write_scope_overlap(paths, &active.paths).is_some()
            })
        });
        if !blocked {
            candidate = Some(row);
            break;
        }
    }
    let Some(candidate) = candidate else {
        return Ok(None);
    };
    let task_id: String = candidate.try_get("task_id").db()?;
    let graph_id: String = candidate.try_get("graph_id").db()?;
    let task_version: i64 = candidate.try_get("version").db()?;
    let input_json: String = candidate.try_get("input_json").db()?;
    let input_json =
        task_input_with_prerequisites(connection, &graph_id, &task_id, &input_json).await?;
    sqlx::query(
        "INSERT INTO local_agent_runs(\
         run_id, owner_user_id, owner_entity_type, owner_entity_id, profile_key, \
         model_config_ref, model_config_revision, capability_policy_revision, input_json, \
         status, iteration, model_attempt, max_iterations, version, claim_token, \
         claim_until_unix_ms, next_attempt_at_unix_ms, pending_tool_batch_json, \
         terminal_outcome_json, checkpoint_json, continuation_input_json, \
         created_at_unix_ms, updated_at_unix_ms) \
         VALUES(?, ?, 'task', ?, ?, ?, ?, ?, ?, 'queued', 0, 1, ?, 1, NULL, NULL, NULL, \
         NULL, NULL, 'null', NULL, ?, ?)",
    )
    .bind(run_id)
    .bind(candidate.try_get::<String, _>("owner_user_id").db()?)
    .bind(&task_id)
    .bind(candidate.try_get::<String, _>("profile_key").db()?)
    .bind(candidate.try_get::<String, _>("model_config_ref").db()?)
    .bind(
        candidate
            .try_get::<String, _>("model_config_revision")
            .db()?,
    )
    .bind(
        candidate
            .try_get::<String, _>("capability_policy_revision")
            .db()?,
    )
    .bind(input_json)
    .bind(candidate.try_get::<i64, _>("max_iterations").db()?)
    .bind(now_unix_ms)
    .bind(now_unix_ms)
    .execute(&mut *connection)
    .await
    .db()?;
    let updated = sqlx::query(
        "UPDATE local_tasks SET active_run_id = ?, \
         version = version + 1, updated_at_unix_ms = ? \
         WHERE task_id = ? AND graph_id = ? AND status = 'ready' \
         AND active_run_id IS NULL AND version = ?",
    )
    .bind(run_id)
    .bind(now_unix_ms)
    .bind(&task_id)
    .bind(&graph_id)
    .bind(task_version)
    .execute(&mut *connection)
    .await
    .db()?;
    if updated.rows_affected() != 1 {
        return Err(ClientStorageError::Conflict(format!(
            "task changed while starting: {task_id}"
        )));
    }
    SqliteClientStorage::insert_event(
        connection,
        event_id,
        run_id,
        "task_run_created",
        &serde_json::json!({"graph_id": graph_id, "task_id": task_id}),
        now_unix_ms,
    )
    .await
    .db()?;
    SqliteClientStorage::fetch_run_on(connection, run_id).await
}
