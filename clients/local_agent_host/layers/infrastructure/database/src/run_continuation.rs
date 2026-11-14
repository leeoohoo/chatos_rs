// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{tool_store_record::decode_invocation, ClientStorageError, SqliteResultExt};
use chatos_local_agent_protocol::{LocalAgentRunRecord, LocalAgentRunStatus};
use serde_json::{json, Value};
use sqlx::SqliteConnection;
use std::collections::{HashMap, HashSet};

/// Preserve an unconsumed tool continuation when a reviewed Run is resumed. Hosts released
/// before this repair may already have cleared it, so reconstruct it from the durable tool
/// invocations that correspond exactly to the latest checkpoint's function calls.
pub(super) async fn for_resume(
    connection: &mut SqliteConnection,
    run: &LocalAgentRunRecord,
    requested: &Value,
) -> Result<Value, ClientStorageError> {
    // Interactive tools intentionally keep their completed invocation while waiting for
    // a human answer; their resume payload is translated into that call's output by the
    // profile. It is not a lost ordinary tool continuation.
    if run.status == LocalAgentRunStatus::WaitingUser {
        return Ok(requested.clone());
    }
    let tool_results = match run.continuation_input.as_ref() {
        Some(existing) if existing.get("type").and_then(Value::as_str) == Some("tool_results") => {
            Some(existing.clone())
        }
        _ => reconstruct_tool_results(connection, run).await?,
    };
    let Some(tool_results) = tool_results else {
        return Ok(requested.clone());
    };
    merge_resume_guidance(tool_results, requested)
}

async fn reconstruct_tool_results(
    connection: &mut SqliteConnection,
    run: &LocalAgentRunRecord,
) -> Result<Option<Value>, ClientStorageError> {
    let calls = latest_function_calls(&run.checkpoint);
    if calls.is_empty() {
        return Ok(None);
    }
    let rows = sqlx::query(
        "SELECT invocation_id, run_id, batch_id, call_id, tool_name, execution_route, \
         arguments_json, side_effecting, requires_approval, approval_status, \
         approval_decided_by, approval_reason, approval_decided_at_unix_ms, status, \
         result_json, error_text, version, claim_token, claim_until_unix_ms, \
         created_at_unix_ms, updated_at_unix_ms \
         FROM local_agent_tool_invocations WHERE run_id = ? \
           AND status IN ('succeeded','failed','needs_review') \
         ORDER BY created_at_unix_ms, invocation_id",
    )
    .bind(&run.run_id)
    .fetch_all(&mut *connection)
    .await
    .db()?;
    let requested_ids = calls
        .iter()
        .map(|(call_id, _)| call_id.as_str())
        .collect::<HashSet<_>>();
    let mut by_call_id = HashMap::new();
    for row in rows {
        let invocation = decode_invocation(row)?;
        if !requested_ids.contains(invocation.call_id.as_str()) {
            continue;
        }
        if by_call_id
            .insert(invocation.call_id.clone(), invocation)
            .is_some()
        {
            // Ambiguous historical data must not be guessed at.
            return Ok(None);
        }
    }
    let mut invocations = Vec::with_capacity(calls.len());
    let mut batch_id = None;
    for (call_id, tool_name) in calls {
        let Some(invocation) = by_call_id.remove(&call_id) else {
            return Ok(None);
        };
        if invocation.tool_name != tool_name {
            return Ok(None);
        }
        if let Some(existing) = batch_id.as_ref() {
            if existing != &invocation.batch_id {
                return Ok(None);
            }
        } else {
            batch_id = Some(invocation.batch_id.clone());
        }
        invocations.push(invocation);
    }
    Ok(Some(json!({
        "type": "tool_results",
        "batch_id": batch_id,
        "invocations": invocations,
        "recovered_from_durable_invocations": true
    })))
}

pub(super) async fn recover_tool_results(
    connection: &mut SqliteConnection,
    run: &LocalAgentRunRecord,
) -> Result<Option<Value>, ClientStorageError> {
    reconstruct_tool_results(connection, run).await
}

fn latest_function_calls(checkpoint: &Value) -> Vec<(String, String)> {
    checkpoint
        .pointer("/response/response_output_items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("function_call"))
        .filter_map(|item| {
            let call_id = item.get("call_id")?.as_str()?.trim();
            let tool_name = item.get("name")?.as_str()?.trim();
            (!call_id.is_empty() && !tool_name.is_empty())
                .then(|| (call_id.to_string(), tool_name.to_string()))
        })
        .collect()
}

fn merge_resume_guidance(
    mut tool_results: Value,
    requested: &Value,
) -> Result<Value, ClientStorageError> {
    let Some(object) = tool_results.as_object_mut() else {
        return Err(ClientStorageError::InvalidState(
            "tool results continuation must be an object".to_string(),
        ));
    };
    let mut pending = match requested.get("type").and_then(Value::as_str) {
        Some("guidance") => requested
            .get("guidance")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        Some("resume") => resume_guidance(requested).into_iter().collect(),
        _ => return Ok(requested.clone()),
    };
    if pending.is_empty() {
        return Ok(tool_results);
    }
    let guidance = object
        .entry("guidance")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| {
            ClientStorageError::InvalidState(
                "tool results continuation guidance must be an array".to_string(),
            )
        })?;
    guidance.append(&mut pending);
    Ok(tool_results)
}

fn resume_guidance(resume: &Value) -> Option<Value> {
    let input = resume.get("input").cloned().unwrap_or(Value::Null);
    let message = input
        .get("message")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            (!input.is_null()).then(|| {
                let reason = resume
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("resumed");
                format!("Local Agent resumed ({reason}). User input: {input}")
            })
        });
    let attachments = input
        .get("attachments")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    (message.is_some() || !attachments.is_empty()).then(|| {
        json!({
            "message": message,
            "attachments": attachments,
            "source": "run_resume"
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IdempotentCommand, LocalAgentRunStore, SqliteClientStorage};
    use chatos_local_agent_protocol::LocalAgentRunStatus;

    fn historical_run() -> LocalAgentRunRecord {
        LocalAgentRunRecord {
            run_id: "run-recover-tools".into(),
            owner_user_id: "user-1".into(),
            owner_entity_type: "conversation_turn".into(),
            owner_entity_id: "turn-1".into(),
            profile_key: "task_execution".into(),
            model_config_ref: "model-1".into(),
            model_config_revision: "revision-1".into(),
            capability_policy_revision: "policy-1".into(),
            input: json!({"message":"write the plan"}),
            status: LocalAgentRunStatus::NeedsReview,
            iteration: 2,
            model_attempt: 1,
            max_iterations: 8,
            version: 4,
            claim_token: None,
            claim_until_unix_ms: None,
            next_attempt_at_unix_ms: None,
            pending_tool_batch: None,
            checkpoint: json!({"response":{"response_output_items":[{
                "type":"function_call",
                "call_id":"call-stage",
                "name":"stage_edit_batch",
                "arguments":"{}"
            }]}}),
            continuation_input: None,
            terminal_outcome: Some(json!({"error":"request exceeds budget"})),
            created_at_unix_ms: 1_000,
            updated_at_unix_ms: 2_000,
        }
    }

    #[test]
    fn resume_is_added_without_replacing_tool_results() {
        let merged = merge_resume_guidance(
            json!({"type":"tool_results", "invocations":[{"call_id":"call-1"}]}),
            &json!({
                "type":"resume",
                "reason":"human retry",
                "input":{"message":"continue", "attachments":[]}
            }),
        )
        .expect("merge");
        assert_eq!(merged["type"], "tool_results");
        assert_eq!(merged["invocations"][0]["call_id"], "call-1");
        assert_eq!(merged["guidance"][0]["message"], "continue");
    }

    #[tokio::test]
    async fn missing_historical_continuation_is_rebuilt_without_reexecuting_tool() {
        let storage = SqliteClientStorage::connect_memory()
            .await
            .expect("storage");
        let run = historical_run();
        storage
            .create_run(
                &IdempotentCommand {
                    command_id: "create-recovery-run".into(),
                    request_fingerprint: "create-recovery-run".into(),
                    persist_receipt: true,
                },
                &run,
                "event-created",
            )
            .await
            .expect("create run");
        sqlx::query(
            "INSERT INTO local_agent_tool_invocations(\
             invocation_id, run_id, batch_id, call_id, tool_name, arguments_json, \
             side_effecting, status, result_json, error_text, version, claim_token, \
             claim_until_unix_ms, created_at_unix_ms, updated_at_unix_ms) \
             VALUES('invocation-stage', ?, 'batch-stage', 'call-stage', \
             'stage_edit_batch', '{}', 1, 'succeeded', ?, NULL, 3, NULL, NULL, 1500, 1600)",
        )
        .bind(&run.run_id)
        .bind(r#"{"changed":true,"path":"plan.md"}"#)
        .execute(&storage.pool)
        .await
        .expect("insert completed invocation");
        let mut connection = storage.pool.acquire().await.expect("connection");
        let recovered = for_resume(
            &mut connection,
            &run,
            &json!({
                "type":"resume",
                "reason":"human_team_run_resume",
                "input":{"message":"continue from saved tool results", "attachments":[]}
            }),
        )
        .await
        .expect("recover continuation");
        assert_eq!(recovered["type"], "tool_results");
        assert_eq!(recovered["batch_id"], "batch-stage");
        assert_eq!(recovered["invocations"].as_array().map(Vec::len), Some(1));
        assert_eq!(recovered["invocations"][0]["call_id"], "call-stage");
        assert_eq!(recovered["invocations"][0]["status"], "succeeded");
        assert_eq!(
            recovered["guidance"][0]["message"],
            "continue from saved tool results"
        );
        drop(connection);
        let invocation_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM local_agent_tool_invocations WHERE run_id = ?",
        )
        .bind(&run.run_id)
        .fetch_one(&storage.pool)
        .await
        .expect("count invocations");
        assert_eq!(
            invocation_count, 1,
            "recovery must not execute the tool again"
        );
    }

    #[tokio::test]
    async fn waiting_user_resume_remains_owned_by_interactive_tool_protocol() {
        let storage = SqliteClientStorage::connect_memory()
            .await
            .expect("storage");
        let mut run = historical_run();
        run.status = LocalAgentRunStatus::WaitingUser;
        let requested = json!({
            "type":"resume",
            "reason":"ask_user_submitted",
            "input":{"source":"ask_user", "values":{"choice":"yes"}}
        });
        let mut connection = storage.pool.acquire().await.expect("connection");
        let result = for_resume(&mut connection, &run, &requested)
            .await
            .expect("preserve interactive resume");
        assert_eq!(result, requested);
    }
}
