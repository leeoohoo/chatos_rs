// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{decode_run, ClientStorageError, SqliteClientStorage, SqliteResultExt};
use chatos_local_agent_protocol::{LocalAgentRunListScope, LocalAgentRunPage};

pub(super) async fn list_runs(
    storage: &SqliteClientStorage,
    owner_user_id: &str,
    scope: LocalAgentRunListScope,
    before_updated_at_unix_ms: Option<i64>,
    before_run_id: Option<&str>,
    limit: u32,
) -> Result<LocalAgentRunPage, ClientStorageError> {
    validate_page(
        owner_user_id,
        before_updated_at_unix_ms,
        before_run_id,
        limit,
    )?;
    let scope_filter = match scope {
        LocalAgentRunListScope::Active => " AND status NOT IN ('succeeded','failed','cancelled')",
        LocalAgentRunListScope::Terminal => " AND status IN ('succeeded','failed','cancelled')",
        LocalAgentRunListScope::All => "",
    };
    let cursor_filter = if before_updated_at_unix_ms.is_some() {
        " AND (updated_at_unix_ms < ? OR (updated_at_unix_ms = ? AND run_id < ?))"
    } else {
        ""
    };
    let sql = format!(
        "SELECT run_id, owner_user_id, owner_entity_type, owner_entity_id, profile_key, \
         model_config_ref, model_config_revision, capability_policy_revision, input_json, \
         status, iteration, model_attempt, max_iterations, version, claim_token, \
         claim_until_unix_ms, next_attempt_at_unix_ms, pending_tool_batch_json, \
         terminal_outcome_json, checkpoint_json, continuation_input_json, \
         created_at_unix_ms, updated_at_unix_ms FROM local_agent_runs \
         WHERE owner_user_id = ?{scope_filter}{cursor_filter} \
         ORDER BY updated_at_unix_ms DESC, run_id DESC LIMIT ?"
    );
    let mut query = sqlx::query(&sql).bind(owner_user_id);
    if let (Some(timestamp), Some(run_id)) = (before_updated_at_unix_ms, before_run_id) {
        query = query.bind(timestamp).bind(timestamp).bind(run_id);
    }
    let mut connection = storage.pool.acquire().await.db()?;
    let rows = query
        .bind(i64::from(limit) + 1)
        .fetch_all(&mut *connection)
        .await
        .db()?;
    let mut runs = rows
        .into_iter()
        .map(decode_run)
        .collect::<Result<Vec<_>, _>>()?;
    let has_more = runs.len() > limit as usize;
    runs.truncate(limit as usize);
    let (next_before_updated_at_unix_ms, next_before_run_id) = if has_more {
        let last = runs.last().expect("positive validated page limit");
        (Some(last.updated_at_unix_ms), Some(last.run_id.clone()))
    } else {
        (None, None)
    };
    Ok(LocalAgentRunPage {
        runs,
        next_before_updated_at_unix_ms,
        next_before_run_id,
    })
}

fn validate_page(
    owner_user_id: &str,
    before_updated_at_unix_ms: Option<i64>,
    before_run_id: Option<&str>,
    limit: u32,
) -> Result<(), ClientStorageError> {
    if owner_user_id.trim().is_empty()
        || owner_user_id.len() > 256
        || owner_user_id.chars().any(char::is_control)
    {
        return Err(ClientStorageError::InvalidState(
            "owner_user_id must contain 1..=256 characters".to_string(),
        ));
    }
    let cursor_valid = match (before_updated_at_unix_ms, before_run_id) {
        (None, None) => true,
        (Some(timestamp), Some(run_id)) => {
            timestamp >= 0
                && !run_id.trim().is_empty()
                && run_id.len() <= 256
                && !run_id.chars().any(char::is_control)
        }
        _ => false,
    };
    if !(1..=100).contains(&limit) || !cursor_valid {
        return Err(ClientStorageError::InvalidState(
            "invalid Run list page request".to_string(),
        ));
    }
    Ok(())
}
