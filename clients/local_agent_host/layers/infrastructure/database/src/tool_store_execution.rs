// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    conversation_lifecycle, task_conversation_writeback, task_lifecycle,
    tool_store_record::decode_invocation, ClientStorageError, SqliteClientStorage, SqliteResultExt,
};
use chatos_local_agent_protocol::LocalAgentToolStatus;
use serde_json::{json, Value};
use sqlx::{Row, SqliteConnection};

pub(super) async fn recover_expired_on(
    connection: &mut SqliteConnection,
    owner_user_id: &str,
    now_unix_ms: i64,
) -> Result<u64, ClientStorageError> {
    let rows = sqlx::query(
        "SELECT invocation_id, run_id, batch_id, call_id, side_effecting, version \
         FROM local_agent_tool_invocations WHERE status = 'running' \
         AND claim_until_unix_ms IS NOT NULL AND claim_until_unix_ms <= ? \
         AND run_id IN (SELECT run_id FROM local_agent_runs WHERE owner_user_id = ?) \
         ORDER BY invocation_id",
    )
    .bind(now_unix_ms)
    .bind(owner_user_id)
    .fetch_all(&mut *connection)
    .await
    .db()?;
    for row in &rows {
        let invocation_id: String = row.try_get("invocation_id").db()?;
        let run_id: String = row.try_get("run_id").db()?;
        let batch_id: String = row.try_get("batch_id").db()?;
        let call_id: String = row.try_get("call_id").db()?;
        let side_effecting: bool = row.try_get("side_effecting").db()?;
        let version: i64 = row.try_get("version").db()?;
        let next_status = if side_effecting {
            LocalAgentToolStatus::NeedsReview
        } else {
            LocalAgentToolStatus::Pending
        };
        sqlx::query(
            "UPDATE local_agent_tool_invocations SET status = ?, version = version + 1, \
             claim_token = NULL, claim_until_unix_ms = NULL, error_text = ?, \
             updated_at_unix_ms = ? WHERE invocation_id = ? AND version = ? \
             AND status = 'running' AND run_id IN (\
               SELECT run_id FROM local_agent_runs WHERE owner_user_id = ?\
             )",
        )
        .bind(next_status.as_str())
        .bind(side_effecting.then_some("tool result was unknown when its claim expired"))
        .bind(now_unix_ms)
        .bind(&invocation_id)
        .bind(version)
        .bind(owner_user_id)
        .execute(&mut *connection)
        .await
        .db()?;
        let event_type = if side_effecting {
            let continuation = tool_batch_continuation(connection, &run_id, &batch_id).await?;
            let outcome = json!({
                "status": "needs_review",
                "reason_code": "tool_execution_interrupted",
                "error": "A side-effecting tool stopped before its result could be confirmed. Review the execution record before retrying."
            });
            let updated = sqlx::query(
                "UPDATE local_agent_runs SET status = 'needs_review', version = version + 1, \
                 continuation_input_json = ?, terminal_outcome_json = COALESCE(terminal_outcome_json, ?), \
                 updated_at_unix_ms = ? \
                 WHERE run_id = ? AND status = 'waiting_tool_result'",
            )
            .bind(serde_json::to_string(&continuation)?)
            .bind(serde_json::to_string(&outcome)?)
            .bind(now_unix_ms)
            .bind(&run_id)
            .execute(&mut *connection)
            .await
            .db()?;
            if updated.rows_affected() == 1 {
                reconcile_needs_review_run(connection, &run_id, now_unix_ms).await?;
            }
            "tool_claim_expired_needs_review"
        } else {
            "tool_claim_expired_requeued"
        };
        let event_id = format!("tool-recovery:{invocation_id}:{}", version + 1);
        SqliteClientStorage::insert_event(
            connection,
            &event_id,
            &run_id,
            event_type,
            &json!({
                "invocation_id": invocation_id,
                "call_id": call_id,
                "side_effecting": side_effecting
            }),
            now_unix_ms,
        )
        .await
        .db()?;
    }
    Ok(rows.len() as u64)
}

async fn reconcile_needs_review_run(
    connection: &mut SqliteConnection,
    run_id: &str,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    let run = SqliteClientStorage::fetch_run_on(connection, run_id)
        .await?
        .ok_or_else(|| ClientStorageError::NotFound(run_id.to_string()))?;
    task_lifecycle::reconcile_task_after_run(connection, &run, now_unix_ms).await?;
    task_conversation_writeback::write_back_terminal_task_run(connection, &run, now_unix_ms)
        .await?;
    conversation_lifecycle::reconcile_conversation_after_run(connection, &run, now_unix_ms).await
}

pub(super) async fn advance_run_after_tool(
    connection: &mut SqliteConnection,
    run_id: &str,
    batch_id: &str,
    completed_status: LocalAgentToolStatus,
    event_id: &str,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    if completed_status == LocalAgentToolStatus::NeedsReview {
        let continuation = tool_batch_continuation(connection, run_id, batch_id).await?;
        let outcome = json!({
            "status": "needs_review",
            "reason_code": "tool_result_requires_review",
            "error": "A tool result could not be confirmed safely. Review the execution record before retrying."
        });
        let updated = sqlx::query(
            "UPDATE local_agent_runs SET status = 'needs_review', version = version + 1, \
             continuation_input_json = ?, terminal_outcome_json = COALESCE(terminal_outcome_json, ?), \
             updated_at_unix_ms = ? \
             WHERE run_id = ? AND status = 'waiting_tool_result'",
        )
        .bind(serde_json::to_string(&continuation)?)
        .bind(serde_json::to_string(&outcome)?)
        .bind(now_unix_ms)
        .bind(run_id)
        .execute(&mut *connection)
        .await
        .db()?;
        if updated.rows_affected() == 1 {
            reconcile_needs_review_run(connection, run_id, now_unix_ms).await?;
        }
        return Ok(());
    }
    let remaining: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM local_agent_tool_invocations WHERE run_id = ? AND batch_id = ? \
         AND status IN ('pending','running')",
    )
    .bind(run_id)
    .bind(batch_id)
    .fetch_one(&mut *connection)
    .await
    .db()?;
    if remaining > 0 {
        return Ok(());
    }
    let waiting_prompt = sqlx::query(
        "SELECT invocation_id, call_id, result_json FROM local_agent_tool_invocations \
         WHERE run_id = ? AND batch_id = ? AND status = 'succeeded' \
         AND tool_name IN ('ask_user_prompt_key_values','ask_user_prompt_choices',\
                           'ask_user_prompt_mixed_form') \
         ORDER BY invocation_id LIMIT 1",
    )
    .bind(run_id)
    .bind(batch_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?;
    if let Some(prompt_row) = waiting_prompt {
        let invocation_id: String = prompt_row.try_get("invocation_id").db()?;
        let call_id: String = prompt_row.try_get("call_id").db()?;
        let result_json: String = prompt_row.try_get("result_json").db()?;
        let input_json: String =
            sqlx::query_scalar("SELECT input_json FROM local_agent_runs WHERE run_id = ?")
                .bind(run_id)
                .fetch_one(&mut *connection)
                .await
                .db()?;
        let run_input: Value = serde_json::from_str(&input_json)?;
        let result: Value = serde_json::from_str(&result_json)?;
        let mut prompt = result
            .get("prompt")
            .and_then(Value::as_object)
            .cloned()
            .ok_or_else(|| {
                ClientStorageError::InvalidState(
                    "Ask User tool result is missing prompt".to_string(),
                )
            })?;
        prompt.insert("tool_call_id".to_string(), Value::String(call_id));
        let mut payload = json!({
            "prompt": Value::Object(prompt),
            "invocation_id": invocation_id,
            "batch_id": batch_id
        });
        if let Some(object) = payload.as_object_mut() {
            if let Some(conversation_id) = run_input
                .get("conversation_id")
                .or_else(|| run_input.get("source_conversation_id"))
                .and_then(Value::as_str)
            {
                object.insert(
                    "conversation_id".to_string(),
                    Value::String(conversation_id.to_string()),
                );
            }
            if let Some(turn_id) = run_input
                .get("turn_id")
                .or_else(|| run_input.get("source_turn_id"))
                .and_then(Value::as_str)
            {
                object.insert("turn_id".to_string(), Value::String(turn_id.to_string()));
            }
        }
        let updated = sqlx::query(
            "UPDATE local_agent_runs SET status = 'waiting_user', version = version + 1, \
             pending_tool_batch_json = NULL, continuation_input_json = NULL, \
             claim_token = NULL, claim_until_unix_ms = NULL, updated_at_unix_ms = ? \
             WHERE run_id = ? AND status = 'waiting_tool_result'",
        )
        .bind(now_unix_ms)
        .bind(run_id)
        .execute(&mut *connection)
        .await
        .db()?;
        if updated.rows_affected() == 1 {
            SqliteClientStorage::insert_event(
                connection,
                event_id,
                run_id,
                "user_input_requested",
                &payload,
                now_unix_ms,
            )
            .await
            .db()?;
        }
        return Ok(());
    }
    let waiting_survey = sqlx::query(
        "SELECT survey.survey_id, survey.project_resource_id \
         FROM local_requirement_surveys survey \
         INNER JOIN local_agent_tool_invocations invocation \
           ON invocation.run_id = survey.source_run_id \
          AND survey.survey_id = ('local-survey-' || invocation.invocation_id) \
         WHERE survey.source_run_id = ? AND survey.status = 'open' \
           AND invocation.batch_id = ? \
           AND invocation.tool_name = 'requirement_survey_create' \
           AND invocation.status = 'succeeded' \
         ORDER BY survey.created_at_unix_ms, survey.survey_id LIMIT 1",
    )
    .bind(run_id)
    .bind(batch_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?;
    if let Some(survey) = waiting_survey {
        let survey_id: String = survey.try_get("survey_id").db()?;
        let project_resource_id: String = survey.try_get("project_resource_id").db()?;
        let payload = json!({
            "survey_id": survey_id,
            "project_resource_id": project_resource_id,
            "batch_id": batch_id
        });
        let updated = sqlx::query(
            "UPDATE local_agent_runs SET status = 'waiting_user', version = version + 1, \
             pending_tool_batch_json = NULL, continuation_input_json = NULL, \
             claim_token = NULL, claim_until_unix_ms = NULL, updated_at_unix_ms = ? \
             WHERE run_id = ? AND status = 'waiting_tool_result'",
        )
        .bind(now_unix_ms)
        .bind(run_id)
        .execute(&mut *connection)
        .await
        .db()?;
        if updated.rows_affected() == 1 {
            SqliteClientStorage::insert_event(
                connection,
                event_id,
                run_id,
                "requirement_survey_waiting",
                &payload,
                now_unix_ms,
            )
            .await
            .db()?;
            super::requirement_survey_store::write_back_main_chat_survey_status(
                connection,
                &survey_id,
                false,
                now_unix_ms,
            )
            .await?;
        }
        return Ok(());
    }
    let continuation = tool_batch_continuation(connection, run_id, batch_id).await?;
    let updated = sqlx::query(
        "UPDATE local_agent_runs SET status = 'continuation_ready', version = version + 1, \
         pending_tool_batch_json = NULL, continuation_input_json = ?, updated_at_unix_ms = ? \
         WHERE run_id = ? AND status = 'waiting_tool_result'",
    )
    .bind(serde_json::to_string(&continuation)?)
    .bind(now_unix_ms)
    .bind(run_id)
    .execute(&mut *connection)
    .await
    .db()?;
    if updated.rows_affected() == 1 {
        SqliteClientStorage::insert_event(
            connection,
            event_id,
            run_id,
            "tool_batch_completed",
            &continuation,
            now_unix_ms,
        )
        .await
        .db()?;
    }
    Ok(())
}

async fn tool_batch_continuation(
    connection: &mut SqliteConnection,
    run_id: &str,
    batch_id: &str,
) -> Result<Value, ClientStorageError> {
    let rows = sqlx::query(
        "SELECT invocation_id, run_id, batch_id, call_id, tool_name, execution_route, arguments_json, \
         side_effecting, requires_approval, approval_status, approval_decided_by, \
         approval_reason, approval_decided_at_unix_ms, status, result_json, error_text, version, claim_token, \
         claim_until_unix_ms, created_at_unix_ms, updated_at_unix_ms \
         FROM local_agent_tool_invocations WHERE run_id = ? AND batch_id = ? \
         ORDER BY invocation_id",
    )
    .bind(run_id)
    .bind(batch_id)
    .fetch_all(&mut *connection)
    .await
    .db()?;
    let invocations = rows
        .into_iter()
        .map(decode_invocation)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({
        "type": "tool_results",
        "batch_id": batch_id,
        "invocations": invocations
    }))
}
