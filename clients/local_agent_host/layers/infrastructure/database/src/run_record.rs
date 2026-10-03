// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{ClientStorageError, SqliteResultExt};
use chatos_local_agent_protocol::{
    LocalAgentEventRecord, LocalAgentRunRecord, LocalAgentRunStatus, LocalAgentRunSummary,
};
use sqlx::{sqlite::SqliteRow, Row};
use std::str::FromStr;

pub(super) fn decode_run(row: SqliteRow) -> Result<LocalAgentRunRecord, ClientStorageError> {
    let status: String = row.try_get("status").db()?;
    let input: String = row.try_get("input_json").db()?;
    let pending_tool_batch: Option<String> = row.try_get("pending_tool_batch_json").db()?;
    let terminal_outcome: Option<String> = row.try_get("terminal_outcome_json").db()?;
    let checkpoint: String = row.try_get("checkpoint_json").db()?;
    let continuation_input: Option<String> = row.try_get("continuation_input_json").db()?;
    Ok(LocalAgentRunRecord {
        run_id: row.try_get("run_id").db()?,
        owner_user_id: row.try_get("owner_user_id").db()?,
        owner_entity_type: row.try_get("owner_entity_type").db()?,
        owner_entity_id: row.try_get("owner_entity_id").db()?,
        profile_key: row.try_get("profile_key").db()?,
        model_config_ref: row.try_get("model_config_ref").db()?,
        model_config_revision: row.try_get("model_config_revision").db()?,
        capability_policy_revision: row.try_get("capability_policy_revision").db()?,
        input: serde_json::from_str(&input)?,
        status: LocalAgentRunStatus::from_str(&status).map_err(ClientStorageError::InvalidState)?,
        iteration: integer_to_u32(row.try_get("iteration").db()?, "iteration")?,
        model_attempt: integer_to_u32(row.try_get("model_attempt").db()?, "model_attempt")?,
        max_iterations: integer_to_u32(row.try_get("max_iterations").db()?, "max_iterations")?,
        version: integer_to_u64(row.try_get("version").db()?, "version")?,
        claim_token: row.try_get("claim_token").db()?,
        claim_until_unix_ms: row.try_get("claim_until_unix_ms").db()?,
        next_attempt_at_unix_ms: row.try_get("next_attempt_at_unix_ms").db()?,
        pending_tool_batch: pending_tool_batch
            .map(|value| serde_json::from_str(&value))
            .transpose()?,
        checkpoint: serde_json::from_str(&checkpoint)?,
        continuation_input: continuation_input
            .map(|value| serde_json::from_str(&value))
            .transpose()?,
        terminal_outcome: terminal_outcome
            .map(|value| serde_json::from_str(&value))
            .transpose()?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
        updated_at_unix_ms: row.try_get("updated_at_unix_ms").db()?,
    })
}

pub(super) fn decode_run_summary(
    row: SqliteRow,
) -> Result<LocalAgentRunSummary, ClientStorageError> {
    let status: String = row.try_get("status").db()?;
    let input: String = row.try_get("input_json").db()?;
    let terminal_outcome: Option<String> = row.try_get("terminal_outcome_json").db()?;
    Ok(LocalAgentRunSummary {
        run_id: row.try_get("run_id").db()?,
        owner_user_id: row.try_get("owner_user_id").db()?,
        owner_entity_type: row.try_get("owner_entity_type").db()?,
        owner_entity_id: row.try_get("owner_entity_id").db()?,
        profile_key: row.try_get("profile_key").db()?,
        input: serde_json::from_str(&input)?,
        status: LocalAgentRunStatus::from_str(&status).map_err(ClientStorageError::InvalidState)?,
        version: integer_to_u64(row.try_get("version").db()?, "version")?,
        terminal_outcome: terminal_outcome
            .map(|value| serde_json::from_str(&value))
            .transpose()?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
        updated_at_unix_ms: row.try_get("updated_at_unix_ms").db()?,
    })
}

pub(super) fn decode_event(row: SqliteRow) -> Result<LocalAgentEventRecord, ClientStorageError> {
    let payload: String = row.try_get("payload_json").db()?;
    Ok(LocalAgentEventRecord {
        cursor: row.try_get("cursor").db()?,
        event_id: row.try_get("event_id").db()?,
        run_id: row.try_get("run_id").db()?,
        event_type: row.try_get("event_type").db()?,
        payload: serde_json::from_str(&payload)?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
    })
}

fn integer_to_u32(value: i64, field: &str) -> Result<u32, ClientStorageError> {
    u32::try_from(value)
        .map_err(|_| ClientStorageError::InvalidState(format!("invalid {field}: {value}")))
}

fn integer_to_u64(value: i64, field: &str) -> Result<u64, ClientStorageError> {
    u64::try_from(value)
        .map_err(|_| ClientStorageError::InvalidState(format!("invalid {field}: {value}")))
}
