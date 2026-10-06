// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    conversation_store::next_message_ordinal, ClientStorageError, SqliteClientStorage,
    SqliteResultExt,
};
use chatos_local_agent_protocol::LocalAgentRunRecord;
use serde_json::{json, Value};
use sqlx::{Row, SqliteConnection};

struct CallbackSource {
    graph_id: String,
    task_id: String,
    task_title: String,
    task_status: String,
    task_version: u64,
    task_updated_at_unix_ms: i64,
    conversation_id: String,
    turn_id: String,
    source_user_message_id: String,
    source_run_id: String,
    prefers_english: bool,
}

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
    if run.owner_entity_type != "task" || !run.status.is_terminal() {
        return Ok(false);
    }
    let Some(source) = callback_source(connection, &run.owner_entity_id).await? else {
        return Ok(false);
    };
    let (event, status) = terminal_event_and_status(&source.task_status)?;
    let changed = write_callback(
        connection,
        &source,
        Some(run),
        event,
        status,
        run.terminal_outcome.as_ref(),
        now_unix_ms,
    )
    .await?;
    write_back_graph(connection, &source.graph_id, now_unix_ms).await?;
    Ok(changed)
}

pub(super) async fn write_back_graph(
    connection: &mut SqliteConnection,
    graph_id: &str,
    now_unix_ms: i64,
) -> Result<bool, ClientStorageError> {
    let rows = sqlx::query(
        "SELECT t.task_id, \
         (SELECT r.run_id FROM local_agent_runs r \
          WHERE r.owner_entity_type = 'task' AND r.owner_entity_id = t.task_id \
          AND r.status IN ('succeeded', 'failed', 'cancelled') \
          ORDER BY r.updated_at_unix_ms DESC, r.created_at_unix_ms DESC, r.rowid DESC LIMIT 1) \
          AS terminal_run_id \
         FROM local_tasks t WHERE t.graph_id = ? \
         AND t.status IN ('succeeded', 'failed', 'cancelled', 'blocked') \
         ORDER BY t.task_id",
    )
    .bind(graph_id)
    .fetch_all(&mut *connection)
    .await
    .db()?;
    let mut changed = false;
    for row in rows {
        let task_id: String = row.try_get("task_id").db()?;
        let run_id: Option<String> = row.try_get("terminal_run_id").db()?;
        let run = if let Some(run_id) = run_id {
            SqliteClientStorage::fetch_run_on(connection, &run_id).await?
        } else {
            None
        };
        let Some(source) = callback_source(connection, &task_id).await? else {
            continue;
        };
        let (event, status) = terminal_event_and_status(&source.task_status)?;
        changed |= write_callback(
            connection,
            &source,
            run.as_ref(),
            event,
            status,
            run.as_ref().and_then(|run| run.terminal_outcome.as_ref()),
            now_unix_ms,
        )
        .await?;
    }
    Ok(changed)
}

async fn callback_source(
    connection: &mut SqliteConnection,
    task_id: &str,
) -> Result<Option<CallbackSource>, ClientStorageError> {
    let row = sqlx::query(
        "SELECT g.graph_id, t.task_id, t.title, t.status, t.version, \
         t.updated_at_unix_ms AS task_updated_at_unix_ms, \
         turn.conversation_id, turn.turn_id, turn.user_message_id, turn.run_id AS source_run_id, \
         message.content_json AS source_message_content \
         FROM local_tasks t \
         JOIN local_task_graphs g ON g.graph_id = t.graph_id \
         JOIN local_conversation_turns turn \
           ON g.source_entity_type = 'conversation_turn' \
          AND turn.turn_id = g.source_entity_id \
         JOIN local_conversations conversation \
           ON conversation.conversation_id = turn.conversation_id \
          AND conversation.owner_user_id = g.owner_user_id \
         JOIN local_conversation_messages message \
           ON message.message_id = turn.user_message_id \
         WHERE t.task_id = ?",
    )
    .bind(task_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?;
    row.map(|row| {
        let source_message_content: String = row.try_get("source_message_content").db()?;
        let source_message_content: Value = serde_json::from_str(&source_message_content)?;
        Ok(CallbackSource {
            graph_id: row.try_get("graph_id").db()?,
            task_id: row.try_get("task_id").db()?,
            task_title: row.try_get("title").db()?,
            task_status: row.try_get("status").db()?,
            task_version: u64::try_from(row.try_get::<i64, _>("version").db()?).map_err(|_| {
                ClientStorageError::InvalidState("invalid Task version".to_string())
            })?,
            task_updated_at_unix_ms: row.try_get("task_updated_at_unix_ms").db()?,
            conversation_id: row.try_get("conversation_id").db()?,
            turn_id: row.try_get("turn_id").db()?,
            source_user_message_id: row.try_get("user_message_id").db()?,
            source_run_id: row.try_get("source_run_id").db()?,
            prefers_english: !contains_cjk(&source_message_content),
        })
    })
    .transpose()
}

#[allow(clippy::too_many_arguments)]
async fn write_callback(
    connection: &mut SqliteConnection,
    source: &CallbackSource,
    run: Option<&LocalAgentRunRecord>,
    event: &str,
    status: &str,
    terminal_outcome: Option<&Value>,
    now_unix_ms: i64,
) -> Result<bool, ClientStorageError> {
    let run_scope = run.map(|run| run.run_id.as_str()).unwrap_or(status);
    let message_id = format!(
        "task_runner_callback::{}::{}::{}",
        source.source_user_message_id, source.task_id, run_scope
    );
    let content = callback_content(
        &source.task_title,
        event,
        terminal_outcome,
        source.prefers_english,
    );
    let callback_at_unix_ms = run
        .map(|run| run.updated_at_unix_ms)
        .unwrap_or(source.task_updated_at_unix_ms);
    let mut task_runner_async = json!({
        "mode": "contact_async",
        "message_kind": "task_lifecycle_update",
        "event": event,
        "task_id": source.task_id,
        "run_id": run.map(|run| run.run_id.as_str()),
        "status": status,
        "task_title": source.task_title,
        "source_session_id": source.conversation_id,
        "source_turn_id": source.turn_id,
        "source_user_message_id": source.source_user_message_id,
        "source_run_id": source.source_run_id,
        "callback_at_unix_ms": callback_at_unix_ms,
    });
    if event == "task.run.started" {
        task_runner_async["started_at_unix_ms"] = json!(callback_at_unix_ms);
    } else {
        task_runner_async["finished_at_unix_ms"] = json!(callback_at_unix_ms);
    }
    let mut metadata = json!({
        "kind": "task_execution_callback",
        "graph_id": source.graph_id,
        "conversation_turn_id": source.turn_id,
        "task_runner_async": task_runner_async,
    });
    let existing = sqlx::query(
        "SELECT content_json, metadata_json FROM local_conversation_messages \
         WHERE message_id = ?",
    )
    .bind(&message_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?;
    if event != "task.run.started" {
        if let Some(started_at) = existing
            .as_ref()
            .and_then(|row| row.try_get::<String, _>("metadata_json").ok())
            .and_then(|value| serde_json::from_str::<Value>(&value).ok())
            .and_then(|value| {
                value
                    .pointer("/task_runner_async/started_at_unix_ms")
                    .cloned()
            })
        {
            metadata["task_runner_async"]["started_at_unix_ms"] = started_at;
        }
    }
    let content_json = serde_json::to_string(&Value::String(content))?;
    let metadata_json = serde_json::to_string(&metadata)?;
    let changed = if let Some(existing) = existing {
        let current_content: String = existing.try_get("content_json").db()?;
        let current_metadata: String = existing.try_get("metadata_json").db()?;
        if current_content == content_json && current_metadata == metadata_json {
            false
        } else {
            sqlx::query(
                "UPDATE local_conversation_messages SET content_json = ?, metadata_json = ? \
                 WHERE message_id = ?",
            )
            .bind(&content_json)
            .bind(&metadata_json)
            .bind(&message_id)
            .execute(&mut *connection)
            .await
            .db()?;
            true
        }
    } else {
        let ordinal = next_message_ordinal(connection, &source.conversation_id).await?;
        sqlx::query(
            "INSERT INTO local_conversation_messages(\
             message_id, conversation_id, turn_id, ordinal, role, content_json, metadata_json, \
             created_at_unix_ms) VALUES(?, ?, ?, ?, 'assistant', ?, ?, ?)",
        )
        .bind(&message_id)
        .bind(&source.conversation_id)
        .bind(&source.turn_id)
        .bind(ordinal)
        .bind(&content_json)
        .bind(&metadata_json)
        .bind(now_unix_ms)
        .execute(&mut *connection)
        .await
        .db()?;
        true
    };
    if !changed {
        return Ok(false);
    }
    sqlx::query(
        "UPDATE local_conversations SET version = version + 1, updated_at_unix_ms = ? \
         WHERE conversation_id = ?",
    )
    .bind(now_unix_ms)
    .bind(&source.conversation_id)
    .execute(&mut *connection)
    .await
    .db()?;
    let callback_version = run.map(|run| run.version).unwrap_or(source.task_version);
    SqliteClientStorage::insert_event(
        connection,
        &format!(
            "task-callback:{}:{}:{event}:{callback_version}",
            source.task_id, run_scope
        ),
        run.map(|run| run.run_id.as_str())
            .unwrap_or(source.source_run_id.as_str()),
        "task_callback_written_back",
        &json!({
            "conversation_id": source.conversation_id,
            "turn_id": source.turn_id,
            "graph_id": source.graph_id,
            "task_id": source.task_id,
            "run_id": run.map(|run| run.run_id.as_str()),
            "event": event,
            "status": status,
            "message_id": message_id,
        }),
        now_unix_ms,
    )
    .await?;
    Ok(true)
}

fn terminal_event_and_status(
    status: &str,
) -> Result<(&'static str, &'static str), ClientStorageError> {
    match status {
        "succeeded" => Ok(("task.completed", "succeeded")),
        "failed" => Ok(("task.failed", "failed")),
        "blocked" => Ok(("task.blocked", "blocked")),
        "cancelled" => Ok(("task.cancelled", "cancelled")),
        _ => Err(ClientStorageError::InvalidState(format!(
            "Task callback requires terminal status: {status}"
        ))),
    }
}

fn callback_content(
    _title: &str,
    event: &str,
    terminal_outcome: Option<&Value>,
    english: bool,
) -> String {
    let detail = terminal_outcome.and_then(visible_detail);
    if event == "task.completed" {
        return detail.unwrap_or_else(|| {
            if english {
                "I've finished working on it.".to_string()
            } else {
                "我已经处理完了。".to_string()
            }
        });
    }
    let headline = if english {
        match event {
            "task.run.started" => "I've started working on it.".to_string(),
            "task.failed" => "I couldn't complete this.".to_string(),
            "task.blocked" => "I can't continue yet.".to_string(),
            "task.cancelled" => "I've stopped working on it.".to_string(),
            _ => "I'm continuing to work on it.".to_string(),
        }
    } else {
        match event {
            "task.run.started" => "我已经开始处理了。".to_string(),
            "task.failed" => "我这次没有处理完成。".to_string(),
            "task.blocked" => "我暂时还无法继续处理。".to_string(),
            "task.cancelled" => "我已经停下来了。".to_string(),
            _ => "我还在继续处理。".to_string(),
        }
    };
    let Some(detail) = detail else {
        return headline;
    };
    format!("{headline}\n\n{detail}")
}

fn visible_detail(value: &Value) -> Option<String> {
    let object = value.as_object()?;
    ["content", "report", "answer", "error", "message", "reason"]
        .iter()
        .find_map(|key| object.get(*key)?.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn contains_cjk(value: &Value) -> bool {
    match value {
        Value::String(value) => value.chars().any(|character| {
            ('\u{3400}'..='\u{4dbf}').contains(&character)
                || ('\u{4e00}'..='\u{9fff}').contains(&character)
        }),
        Value::Array(values) => values.iter().any(contains_cjk),
        Value::Object(values) => values.values().any(contains_cjk),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        IdempotentCommand, LocalAgentRunStore, LocalAgentTaskStore, LocalConversationStore,
        RunTransition, SqliteClientStorage,
    };
    use chatos_local_agent_protocol::{
        CreateConversationCommand, CreateTaskGraphCommand, LocalAgentRunRecord,
        LocalAgentRunStatus, LocalConversationTurnStatus, LocalTaskSpec,
        StartConversationTurnCommand,
    };
    use serde_json::{json, Value};

    fn command(command_id: &str) -> IdempotentCommand {
        IdempotentCommand {
            command_id: command_id.to_string(),
            request_fingerprint: command_id.to_string(),
            persist_receipt: true,
        }
    }

    fn conversation_run(turn: &StartConversationTurnCommand) -> LocalAgentRunRecord {
        LocalAgentRunRecord {
            run_id: turn.run_id.clone(),
            owner_user_id: "user-1".to_string(),
            owner_entity_type: "conversation_turn".to_string(),
            owner_entity_id: turn.turn_id.clone(),
            profile_key: "main_chat".to_string(),
            model_config_ref: turn.model_config_ref.clone(),
            model_config_revision: turn.model_config_revision.clone(),
            capability_policy_revision: turn.capability_policy_revision.clone(),
            input: json!({"message": turn.message, "attachments": []}),
            status: LocalAgentRunStatus::Queued,
            iteration: 0,
            model_attempt: 1,
            max_iterations: turn.max_iterations,
            version: 1,
            claim_token: None,
            claim_until_unix_ms: None,
            next_attempt_at_unix_ms: None,
            pending_tool_batch: None,
            checkpoint: Value::Null,
            continuation_input: None,
            terminal_outcome: None,
            created_at_unix_ms: 2_000,
            updated_at_unix_ms: 2_000,
        }
    }

    async fn finish_task_run(
        storage: &SqliteClientStorage,
        run_id: &str,
        status: LocalAgentRunStatus,
        now_unix_ms: i64,
    ) {
        storage
            .start_next_task_run("user-1", run_id, &format!("start-{run_id}"), now_unix_ms)
            .await
            .expect("start Task Run")
            .expect("runnable Task");
        let claim = storage
            .claim_next_run(
                &command(&format!("claim-{run_id}")),
                "user-1",
                "worker-1",
                &format!("token-{run_id}"),
                now_unix_ms + 1,
                now_unix_ms + 10_000,
                &format!("claim-event-{run_id}"),
            )
            .await
            .expect("claim Task Run")
            .expect("Task Run claim");
        storage
            .apply_transition(
                &command(&format!("finish-{run_id}")),
                &RunTransition {
                    run_id: claim.run.run_id,
                    claim_token: claim.claim_token,
                    expected_version: claim.run.version,
                    expected_status: LocalAgentRunStatus::ModelRunning,
                    next_status: status,
                    next_model_attempt: 1,
                    next_attempt_at_unix_ms: None,
                    pending_tool_batch: None,
                    tool_batch: None,
                    checkpoint: None,
                    clear_continuation_input: true,
                    terminal_outcome: Some(json!({
                        "content": format!("Result from {run_id}"),
                        "reasoning": "internal reasoning must not be displayed",
                    })),
                    event_id: format!("finish-event-{run_id}"),
                    event_type: format!("run_{}", status.as_str()),
                    event_payload: json!({"status": status}),
                    occurred_at_unix_ms: now_unix_ms + 2,
                },
            )
            .await
            .expect("finish Task Run");
    }

    #[tokio::test]
    async fn task_runs_write_back_original_task_callback_protocol() {
        let storage = SqliteClientStorage::connect_memory()
            .await
            .expect("storage");
        storage
            .create_conversation(
                &command("create-conversation"),
                &CreateConversationCommand {
                    conversation_id: "conversation-1".to_string(),
                    owner_user_id: "user-1".to_string(),
                    title: "Local conversation".to_string(),
                    resource: None,
                },
                1_000,
            )
            .await
            .expect("create Conversation");
        let turn = StartConversationTurnCommand {
            owner_user_id: "user-1".to_string(),
            conversation_id: "conversation-1".to_string(),
            expected_conversation_version: 1,
            turn_id: "turn-1".to_string(),
            message_id: "message-1".to_string(),
            run_id: "conversation-run-1".to_string(),
            message: "Do the task".to_string(),
            message_metadata: json!({}),
            attachments: Vec::new(),
            model_config_ref: "model-1".to_string(),
            model_config_revision: "revision-1".to_string(),
            capability_policy_revision: "policy-1".to_string(),
            max_iterations: 8,
        };
        storage
            .start_conversation_turn(
                &command("start-turn"),
                &turn,
                &conversation_run(&turn),
                "start-turn-event",
                2_000,
            )
            .await
            .expect("start Turn");
        storage
            .cancel_run(
                &command("cancel-conversation-run"),
                &turn.run_id,
                Some(1),
                "main response completed elsewhere",
                "cancel-conversation-event",
                3_000,
            )
            .await
            .expect("close source Turn");
        storage
            .create_task_graph(
                &command("create-graph"),
                &CreateTaskGraphCommand {
                    graph_id: "graph-1".to_string(),
                    owner_user_id: "user-1".to_string(),
                    source_entity_type: "conversation_turn".to_string(),
                    source_entity_id: "turn-1".to_string(),
                    tasks: vec![LocalTaskSpec {
                        task_id: "task-1".to_string(),
                        title: "Inspect project".to_string(),
                        profile_key: "task_execution".to_string(),
                        model_config_ref: "model-1".to_string(),
                        model_config_revision: "revision-1".to_string(),
                        capability_policy_revision: "policy-1".to_string(),
                        input: json!({"prompt": "inspect"}),
                        max_iterations: 8,
                    }],
                    dependencies: Vec::new(),
                },
                4_000,
            )
            .await
            .expect("create Task Graph");

        finish_task_run(
            &storage,
            "task-run-failed",
            LocalAgentRunStatus::Failed,
            5_000,
        )
        .await;
        storage
            .retry_task(&command("retry-task"), "user-1", "task-1", 3, None, 6_000)
            .await
            .expect("retry Task");
        finish_task_run(
            &storage,
            "task-run-succeeded",
            LocalAgentRunStatus::Succeeded,
            7_000,
        )
        .await;

        storage
            .create_task_graph(
                &command("create-cancelled-graph"),
                &CreateTaskGraphCommand {
                    graph_id: "graph-cancelled".to_string(),
                    owner_user_id: "user-1".to_string(),
                    source_entity_type: "conversation_turn".to_string(),
                    source_entity_id: "turn-1".to_string(),
                    tasks: vec![LocalTaskSpec {
                        task_id: "task-cancelled".to_string(),
                        title: "Cancelled task".to_string(),
                        profile_key: "task_execution".to_string(),
                        model_config_ref: "model-1".to_string(),
                        model_config_revision: "revision-1".to_string(),
                        capability_policy_revision: "policy-1".to_string(),
                        input: json!({"prompt": "cancel"}),
                        max_iterations: 8,
                    }],
                    dependencies: Vec::new(),
                },
                8_000,
            )
            .await
            .expect("create cancellable Task Graph");
        storage
            .cancel_task(
                &command("cancel-task"),
                "user-1",
                "task-cancelled",
                Some(1),
                "no longer needed",
                "unused-run-event",
                9_000,
            )
            .await
            .expect("cancel pending Task");

        let conversation = storage
            .get_conversation("user-1", "conversation-1")
            .await
            .expect("load Conversation")
            .expect("Conversation");
        assert_eq!(conversation.conversation.version, 8);
        assert_eq!(
            conversation.turns[0].status,
            LocalConversationTurnStatus::Cancelled
        );
        assert_eq!(conversation.messages.len(), 4);
        assert_eq!(
            conversation.messages[1].message_id,
            "task_runner_callback::message-1::task-1::task-run-failed"
        );
        assert_eq!(
            conversation.messages[1].metadata["task_runner_async"]["event"],
            "task.failed"
        );
        assert_eq!(
            conversation.messages[1].metadata["task_runner_async"]["run_id"],
            "task-run-failed"
        );
        assert_eq!(
            conversation.messages[1].metadata["task_runner_async"]["started_at_unix_ms"],
            5_000
        );
        assert_eq!(
            conversation.messages[1].metadata["task_runner_async"]["finished_at_unix_ms"],
            5_002
        );
        assert_eq!(
            conversation.messages[1].content,
            "I couldn't complete this.\n\nResult from task-run-failed"
        );
        assert!(!conversation.messages[1]
            .content
            .as_str()
            .expect("callback text")
            .contains("internal reasoning"));
        assert_eq!(
            conversation.messages[2].message_id,
            "task_runner_callback::message-1::task-1::task-run-succeeded"
        );
        assert_eq!(
            conversation.messages[2].metadata["task_runner_async"]["event"],
            "task.completed"
        );
        assert_eq!(
            conversation.messages[2].content,
            "Result from task-run-succeeded"
        );
        assert_eq!(
            conversation.messages[3].message_id,
            "task_runner_callback::message-1::task-cancelled::cancelled"
        );
        assert_eq!(
            conversation.messages[3].metadata["task_runner_async"]["event"],
            "task.cancelled"
        );
        assert!(conversation.messages.iter().skip(1).all(|message| {
            message.metadata["kind"] == "task_execution_callback"
                && message.metadata["task_runner_async"]["source_session_id"] == "conversation-1"
                && message.metadata["task_runner_async"]["source_turn_id"] == "turn-1"
                && message.metadata["task_runner_async"]["source_user_message_id"] == "message-1"
        }));
        let writebacks: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM local_task_graph_writebacks WHERE graph_id = 'graph-1'",
        )
        .fetch_one(&storage.pool)
        .await
        .expect("count writebacks");
        assert_eq!(writebacks, 0);
        let cancelled_writebacks: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM local_task_graph_writebacks \
             WHERE graph_id = 'graph-cancelled'",
        )
        .fetch_one(&storage.pool)
        .await
        .expect("count cancelled writebacks");
        assert_eq!(cancelled_writebacks, 0);
        let events = storage
            .list_events(0, 100, None)
            .await
            .expect("list callback events");
        assert_eq!(
            events
                .iter()
                .filter(|event| event.event_type == "task_callback_written_back")
                .count(),
            5
        );
    }
}
