// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    conversation_store::next_message_ordinal, task_store::fetch_graph, ClientStorageError,
    SqliteClientStorage, SqliteResultExt,
};
use chatos_local_agent_protocol::{LocalAgentRunRecord, LocalTaskGraphStatus, LocalTaskStatus};
use serde_json::{json, Value};
use sqlx::{Row, SqliteConnection};

pub(super) async fn write_back_terminal_graph(
    connection: &mut SqliteConnection,
    cause_run: &LocalAgentRunRecord,
    now_unix_ms: i64,
) -> Result<bool, ClientStorageError> {
    if cause_run.owner_entity_type != "task" || !cause_run.status.is_terminal() {
        return Ok(false);
    }
    let graph_id: Option<String> =
        sqlx::query_scalar("SELECT graph_id FROM local_tasks WHERE task_id = ?")
            .bind(&cause_run.owner_entity_id)
            .fetch_optional(&mut *connection)
            .await
            .db()?;
    let Some(graph_id) = graph_id else {
        return Ok(false);
    };
    write_back_graph(connection, &graph_id, now_unix_ms).await
}

pub(super) async fn write_back_graph(
    connection: &mut SqliteConnection,
    graph_id: &str,
    now_unix_ms: i64,
) -> Result<bool, ClientStorageError> {
    let Some(graph) = fetch_graph(connection, graph_id).await? else {
        return Ok(false);
    };
    if matches!(
        graph.status,
        LocalTaskGraphStatus::Pending | LocalTaskGraphStatus::Running
    ) || graph.source_entity_type != "conversation_turn"
    {
        return Ok(false);
    }
    let terminal_signature = serde_json::to_string(
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
    )?;
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
        return Ok(false);
    }
    let source = sqlx::query(
        "SELECT t.conversation_id, t.turn_id, t.run_id, c.owner_user_id \
         FROM local_conversation_turns t \
         JOIN local_conversations c ON c.conversation_id = t.conversation_id \
         WHERE t.turn_id = ?",
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
    let generation: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(generation), 0) + 1 FROM local_task_graph_writebacks \
         WHERE graph_id = ?",
    )
    .bind(graph_id)
    .fetch_one(&mut *connection)
    .await
    .db()?;
    let message_id = format!("task-graph:{graph_id}:{generation}");
    let ordinal = next_message_ordinal(connection, &conversation_id).await?;
    let tasks = task_summaries(connection, graph_id).await?;
    let content = json!({
        "type": "task_graph_terminal",
        "graph_id": graph_id,
        "status": graph.status,
        "tasks": tasks,
    });
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
    }))?)
    .bind(now_unix_ms)
    .execute(&mut *connection)
    .await
    .db()?;
    sqlx::query(
        "INSERT INTO local_task_graph_writebacks(\
         graph_id, generation, terminal_signature, message_id, terminal_status, \
         created_at_unix_ms) VALUES(?, ?, ?, ?, ?, ?)",
    )
    .bind(graph_id)
    .bind(generation)
    .bind(&terminal_signature)
    .bind(&message_id)
    .bind(graph_status_value(graph.status))
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
            "message_id": message_id,
            "generation": generation,
        }),
        now_unix_ms,
    )
    .await?;
    Ok(true)
}

fn graph_status_value(status: LocalTaskGraphStatus) -> &'static str {
    match status {
        LocalTaskGraphStatus::Succeeded => "succeeded",
        LocalTaskGraphStatus::Failed => "failed",
        LocalTaskGraphStatus::Cancelled => "cancelled",
        LocalTaskGraphStatus::Pending | LocalTaskGraphStatus::Running => {
            unreachable!("only terminal Task Graphs are written back")
        }
    }
}

async fn task_summaries(
    connection: &mut SqliteConnection,
    graph_id: &str,
) -> Result<Vec<Value>, ClientStorageError> {
    let rows = sqlx::query(
        "SELECT t.task_id, t.title, t.status, \
         (SELECT r.terminal_outcome_json FROM local_agent_runs r \
          WHERE r.owner_entity_type = 'task' AND r.owner_entity_id = t.task_id \
          AND r.status IN ('succeeded', 'failed', 'cancelled') \
          ORDER BY r.updated_at_unix_ms DESC, r.created_at_unix_ms DESC, r.rowid DESC LIMIT 1) \
          AS terminal_outcome_json \
         FROM local_tasks t WHERE t.graph_id = ? ORDER BY t.task_id",
    )
    .bind(graph_id)
    .fetch_all(&mut *connection)
    .await
    .db()?;
    rows.into_iter()
        .map(|row| {
            let status: String = row.try_get("status").db()?;
            let status = status
                .parse::<LocalTaskStatus>()
                .map_err(ClientStorageError::InvalidState)?;
            let terminal_outcome: Option<String> = row.try_get("terminal_outcome_json").db()?;
            Ok(json!({
                "task_id": row.try_get::<String, _>("task_id").db()?,
                "title": row.try_get::<String, _>("title").db()?,
                "status": status,
                "terminal_outcome": terminal_outcome
                    .map(|value| serde_json::from_str::<Value>(&value))
                    .transpose()?,
            }))
        })
        .collect()
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
                    terminal_outcome: Some(json!({"run": run_id, "status": status})),
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
    async fn terminal_task_graphs_write_back_each_terminal_generation() {
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
                        profile_key: "task_runner".to_string(),
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
            .retry_task(&command("retry-task"), "user-1", "task-1", 3, 6_000)
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
                        profile_key: "task_runner".to_string(),
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
        assert_eq!(conversation.conversation.version, 6);
        assert_eq!(
            conversation.turns[0].status,
            LocalConversationTurnStatus::Cancelled
        );
        assert_eq!(conversation.messages.len(), 4);
        assert_eq!(conversation.messages[1].content["status"], "failed");
        assert_eq!(conversation.messages[2].content["status"], "succeeded");
        assert_eq!(
            conversation.messages[2].content["tasks"][0]["terminal_outcome"]["run"],
            "task-run-succeeded"
        );
        assert_eq!(conversation.messages[3].content["status"], "cancelled");
        let writebacks: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM local_task_graph_writebacks WHERE graph_id = 'graph-1'",
        )
        .fetch_one(&storage.pool)
        .await
        .expect("count writebacks");
        assert_eq!(writebacks, 2);
        let cancelled_writebacks: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM local_task_graph_writebacks \
             WHERE graph_id = 'graph-cancelled'",
        )
        .fetch_one(&storage.pool)
        .await
        .expect("count cancelled writebacks");
        assert_eq!(cancelled_writebacks, 1);
        let events = storage
            .list_events(0, 100, Some("conversation-run-1"))
            .await
            .expect("list source Run events");
        assert_eq!(
            events
                .iter()
                .filter(|event| event.event_type == "task_graph_written_back")
                .count(),
            3
        );
    }
}
