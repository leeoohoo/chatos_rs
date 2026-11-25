pub(super) async fn write_back_task_run_started(
    connection: &mut SqliteConnection,
    run: &LocalAgentRunRecord,
    now_unix_ms: i64,
) -> Result<bool, ClientStorageError> {
    if run.owner_entity_type != "task" || run.status.is_terminal() {
        return Ok(false);
    }
    let Some(source) = callback_source(connection, &run.owner_entity_id).await? else {
        return Ok(false);
    };
    if !should_write_task_callback(connection, &source, "task.run.started").await? {
        return Ok(false);
    }
    write_callback(
        connection,
        &source,
        Some(run),
        "task.run.started",
        "running",
        None,
        now_unix_ms,
    )
    .await
}

pub(super) async fn write_back_terminal_task_run(
    connection: &mut SqliteConnection,
    run: &LocalAgentRunRecord,
    now_unix_ms: i64,
) -> Result<bool, ClientStorageError> {
    if run.owner_entity_type != "task"
        || (!run.status.is_terminal() && run.status != LocalAgentRunStatus::NeedsReview)
    {
        return Ok(false);
    }
    let Some(source) = callback_source(connection, &run.owner_entity_id).await? else {
        return Ok(false);
    };
    let (event, status) = terminal_event_and_status(&source.task_status)?;
    let reported_outcome =
        super::task_lifecycle::reported_task_outcome(connection, &run.run_id).await?;
    let reported_detail = reported_outcome.as_ref().map(|outcome| {
        json!({
            "result_status": outcome.result_status,
            "status": outcome.result_status,
            "reason": outcome.reason,
        })
    });
    let review_detail = if run.status == LocalAgentRunStatus::NeedsReview {
        needs_review_tool_detail(connection, &run.run_id).await?
    } else {
        None
    };
    let terminal_outcome = if run.status == LocalAgentRunStatus::NeedsReview {
        run.terminal_outcome
            .as_ref()
            .or(reported_detail.as_ref())
            .or(review_detail.as_ref())
    } else if matches!(event, "task.failed" | "task.blocked") {
        reported_detail.as_ref().or(run.terminal_outcome.as_ref())
    } else {
        run.terminal_outcome.as_ref()
    };
    let changed = if should_write_task_callback(connection, &source, event).await? {
        write_callback(
            connection,
            &source,
            Some(run),
            event,
            status,
            terminal_outcome,
            now_unix_ms,
        )
        .await?
    } else {
        false
    };
    let graph_changed = write_back_graph(connection, &source.graph_id, now_unix_ms).await?;
    let recovery_changed =
        schedule_graph_recovery(connection, &source.graph_id, now_unix_ms).await?;
    Ok(changed || graph_changed || recovery_changed)
}

async fn schedule_graph_recovery(
    connection: &mut SqliteConnection,
    graph_id: &str,
    now_unix_ms: i64,
) -> Result<bool, ClientStorageError> {
    let Some(graph) = fetch_graph(connection, graph_id).await? else {
        return Ok(false);
    };
    if graph.status != LocalTaskGraphStatus::Failed
        || graph.source_entity_type != "conversation_turn"
    {
        return Ok(false);
    }
    let has_unreviewed_side_effects: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM local_agent_runs run \
         JOIN local_tasks task ON task.task_id = run.owner_entity_id \
         WHERE task.graph_id = ? AND run.owner_entity_type = 'task' \
           AND run.status = 'needs_review')",
    )
    .bind(graph_id)
    .fetch_one(&mut *connection)
    .await
    .db()?;
    if has_unreviewed_side_effects {
        return Ok(false);
    }
    let source = sqlx::query(
        "SELECT turn.conversation_id, turn.turn_id, turn.run_id, turn.user_message_id, \
         conversation.owner_user_id, message.content_json AS source_message_content \
         FROM local_conversation_turns turn \
         JOIN local_conversations conversation \
           ON conversation.conversation_id = turn.conversation_id \
         JOIN local_conversation_messages message \
           ON message.message_id = turn.user_message_id \
         WHERE turn.turn_id = ?",
    )
    .bind(&graph.source_entity_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?;
    let Some(source) = source else {
        return Ok(false);
    };
    let owner_user_id: String = source.try_get("owner_user_id").db()?;
    if owner_user_id != graph.owner_user_id {
        return Err(ClientStorageError::InvalidState(format!(
            "Task Graph owner does not match source Conversation: {graph_id}"
        )));
    }
    let conversation_id: String = source.try_get("conversation_id").db()?;
    let turn_id: String = source.try_get("turn_id").db()?;
    let source_run_id: String = source.try_get("run_id").db()?;
    let latest_turn_id: Option<String> = sqlx::query_scalar(
        "SELECT turn_id FROM local_conversation_turns WHERE conversation_id = ? \
         ORDER BY created_at_unix_ms DESC, rowid DESC LIMIT 1",
    )
    .bind(&conversation_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?;
    if latest_turn_id.as_deref() != Some(turn_id.as_str()) {
        return Ok(false);
    }
    let Some(mut source_run) = SqliteClientStorage::fetch_run_on(connection, &source_run_id).await?
    else {
        return Ok(false);
    };
    if source_run.status != LocalAgentRunStatus::Succeeded {
        return Ok(false);
    }
    let terminal_signature = graph_terminal_signature(&graph)?;
    if source_run
        .input
        .pointer("/task_graph_recovery/terminal_signature")
        .and_then(Value::as_str)
        == Some(terminal_signature.as_str())
    {
        return Ok(false);
    }
    let source_message_content: String = source.try_get("source_message_content").db()?;
    let source_message_content: Value = serde_json::from_str(&source_message_content)?;
    let summaries = graph_task_summaries(connection, graph_id, !contains_cjk(&source_message_content))
        .await?;
    let tasks = graph
        .tasks
        .iter()
        .map(|task| {
            let summary = summaries
                .iter()
                .find(|summary| summary.get("task_id").and_then(Value::as_str) == Some(&task.task_id));
            json!({
                "task_id": task.task_id,
                "title": task.title,
                "objective": task.input.get("objective"),
                "status": task.status,
                "display_status": summary.and_then(|value| value.get("display_status")),
                "summary": summary.and_then(|value| value.get("summary")),
                "version": task.version,
                "retry_allowed": matches!(task.status, LocalTaskStatus::Failed | LocalTaskStatus::Blocked),
            })
        })
        .collect::<Vec<_>>();
    let recovery_generation = source_run
        .input
        .pointer("/task_graph_recovery/recovery_generation")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .saturating_add(1);
    let recovery = json!({
        "type": "task_graph_recovery",
        "context_authority": "current_run",
        "graph_id": graph_id,
        "graph_status": graph.status,
        "terminal_signature": terminal_signature,
        "recovery_generation": recovery_generation,
        "graph_goal": graph.tasks.iter().find_map(|task| task.input.get("graph_goal")),
        "decomposition_rationale": graph.tasks.iter().find_map(|task| task.input.get("decomposition_rationale")),
        "tasks": tasks,
        "guidance": "This failed Task Graph has been returned to Main Chat for autonomous recovery. Decide from current evidence, not a fixed retry count. If the contract is unchanged and a concrete different safe correction exists, call retry_task with that correction. If method, capability, scope, ownership, or dependency design must change, create a replacement Task or graph using supersedes_task_ids. Never retry needs_review, cancelled work, or an operation with unknown side effects. Ask the user only when recovery truly requires credentials, permission, a product decision, an external human action, or no new safe path remains."
    });
    source_run.input["task_graph_recovery"] = recovery.clone();
    let updated = sqlx::query(
        "UPDATE local_agent_runs SET input_json = ?, status = 'continuation_ready', \
         version = version + 1, continuation_input_json = ?, \
         updated_at_unix_ms = ? WHERE run_id = ? AND status = 'succeeded' AND version = ?",
    )
    .bind(serde_json::to_string(&source_run.input)?)
    .bind(serde_json::to_string(&recovery)?)
    .bind(now_unix_ms)
    .bind(&source_run_id)
    .bind(source_run.version as i64)
    .execute(&mut *connection)
    .await
    .db()?;
    if updated.rows_affected() != 1 {
        return Err(ClientStorageError::Conflict(format!(
            "source Main Chat changed while scheduling Task recovery: {source_run_id}"
        )));
    }
    SqliteClientStorage::insert_event(
        connection,
        &format!("task-recovery:{graph_id}:{recovery_generation}"),
        &source_run_id,
        "task_graph_recovery_scheduled",
        &json!({
            "conversation_id": conversation_id,
            "turn_id": turn_id,
            "graph_id": graph_id,
            "recovery_generation": recovery_generation,
        }),
        now_unix_ms,
    )
    .await?;
    Ok(true)
}

fn graph_terminal_signature(graph: &LocalTaskGraph) -> Result<String, ClientStorageError> {
    serde_json::to_string(
        &graph
            .tasks
            .iter()
            .map(|task| {
                json!({
                    "task_id": task.task_id,
                    "status": task.status,
                    "version": task.version,
                })
            })
            .collect::<Vec<_>>(),
    )
    .map_err(Into::into)
}

pub(super) async fn write_back_graph(
    connection: &mut SqliteConnection,
    graph_id: &str,
    now_unix_ms: i64,
) -> Result<bool, ClientStorageError> {
    let Some(graph) = fetch_graph(connection, graph_id).await? else {
        return Ok(false);
    };
    let notification_policy = resolve_local_task_graph_notification_policy(
        graph.tasks.iter().map(|task| &task.input),
        graph.tasks.len(),
    )
    .map_err(ClientStorageError::InvalidState)?;
    let task_callbacks_changed =
        if notification_policy != LocalTaskGraphNotificationPolicy::GraphSummary {
            write_back_terminal_task_callbacks(connection, graph_id, now_unix_ms).await?
        } else {
            false
        };
    if notification_policy == LocalTaskGraphNotificationPolicy::AllTasks
        || matches!(
            graph.status,
            LocalTaskGraphStatus::Pending | LocalTaskGraphStatus::Running
        )
        || graph.source_entity_type != "conversation_turn"
    {
        return Ok(task_callbacks_changed);
    }
    let terminal_signature = graph_terminal_signature(&graph)?;
    let already_written: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM local_task_graph_writebacks \
         WHERE graph_id = ? AND terminal_signature = ?)",
    )
    .bind(graph_id)
    .bind(&terminal_signature)
    .fetch_one(&mut *connection)
    .await
    .db()?;
    if already_written {
        return Ok(task_callbacks_changed);
    }
    let source = sqlx::query(
        "SELECT turn.conversation_id, turn.turn_id, turn.run_id, conversation.owner_user_id, \
         source_run.input_json AS source_run_input_json, \
         message.content_json AS source_message_content \
         FROM local_conversation_turns turn \
         JOIN local_conversations conversation \
           ON conversation.conversation_id = turn.conversation_id \
         JOIN local_agent_runs source_run ON source_run.run_id = turn.run_id \
         JOIN local_conversation_messages message \
           ON message.message_id = turn.user_message_id \
         WHERE turn.turn_id = ?",
    )
    .bind(&graph.source_entity_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?;
    let Some(source) = source else {
        return Ok(task_callbacks_changed);
    };
    let owner_user_id: String = source.try_get("owner_user_id").db()?;
    if owner_user_id != graph.owner_user_id {
        return Err(ClientStorageError::InvalidState(format!(
            "Task Graph owner does not match source Conversation: {graph_id}"
        )));
    }
    let conversation_id: String = source.try_get("conversation_id").db()?;
    let turn_id: String = source.try_get("turn_id").db()?;
    let source_run_id: String = source.try_get("run_id").db()?;
    let source_message_content: String = source.try_get("source_message_content").db()?;
    let source_message_content: Value = serde_json::from_str(&source_message_content)?;
    let source_run_input_json: String = source.try_get("source_run_input_json").db()?;
    let source_run_input: Value = serde_json::from_str(&source_run_input_json)?;
    let english = !contains_cjk(&source_message_content);
    let generation: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(generation), 0) + 1 FROM local_task_graph_writebacks \
         WHERE graph_id = ?",
    )
    .bind(graph_id)
    .fetch_one(&mut *connection)
    .await
    .db()?;
    let message_id = format!("task-graph:{graph_id}:{generation}");
    let tasks = graph_task_summaries(connection, graph_id, english).await?;
    let summary = graph_summary_text(graph.status, &tasks, english);
    let delivery_text = if notification_policy == LocalTaskGraphNotificationPolicy::ProgressSummary
    {
        Some(graph_delivery_text(graph.status, &tasks, english))
    } else {
        None
    };
    let content = json!({
        "type": "task_graph_terminal",
        "graph_id": graph_id,
        "status": graph.status,
        "notification_policy": notification_policy,
        "summary": summary,
        "delivery_text": delivery_text,
        "tasks": tasks,
    });
    let ordinal = next_message_ordinal(connection, &conversation_id).await?;
    sqlx::query(
        "INSERT INTO local_conversation_messages(\
         message_id, conversation_id, turn_id, ordinal, role, content_json, metadata_json, \
         created_at_unix_ms) VALUES(?, ?, ?, ?, 'assistant', ?, ?, ?)",
    )
    .bind(&message_id)
    .bind(&conversation_id)
    .bind(&turn_id)
    .bind(ordinal)
    .bind(serde_json::to_string(&content)?)
    .bind(serde_json::to_string(&json!({
        "kind": "task_graph_terminal",
        "graph_id": graph_id,
        "generation": generation,
        "notification_policy": notification_policy,
    }))?)
    .bind(now_unix_ms)
    .execute(&mut *connection)
    .await
    .db()?;
    project_program_message_to_memory(
        connection,
        &owner_user_id,
        &source_run_id,
        &source_run_input,
        &conversation_id,
        &turn_id,
        &message_id,
        &content,
        &json!({
            "kind": "task_graph_terminal",
            "graph_id": graph_id,
            "generation": generation,
            "notification_policy": notification_policy,
        }),
        now_unix_ms,
    )
    .await?;
    sqlx::query(
        "INSERT INTO local_task_graph_writebacks(\
         graph_id, generation, terminal_signature, message_id, terminal_status, \
         created_at_unix_ms) VALUES(?, ?, ?, ?, ?, ?)",
    )
    .bind(graph_id)
    .bind(generation)
    .bind(&terminal_signature)
    .bind(&message_id)
    .bind(graph.status.as_str())
    .bind(now_unix_ms)
    .execute(&mut *connection)
    .await
    .db()?;
    sqlx::query(
        "UPDATE local_conversations SET version = version + 1, updated_at_unix_ms = ? \
         WHERE conversation_id = ?",
    )
    .bind(now_unix_ms)
    .bind(&conversation_id)
    .execute(&mut *connection)
    .await
    .db()?;
    SqliteClientStorage::insert_event(
        connection,
        &format!("task-writeback:{graph_id}:{generation}"),
        &source_run_id,
        "task_graph_written_back",
        &json!({
            "conversation_id": conversation_id,
            "turn_id": turn_id,
            "graph_id": graph_id,
            "status": graph.status,
            "notification_policy": notification_policy,
            "message_id": message_id,
            "generation": generation,
        }),
        now_unix_ms,
    )
    .await?;
    Ok(true)
}
