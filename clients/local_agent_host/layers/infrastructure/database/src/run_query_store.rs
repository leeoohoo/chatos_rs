// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{decode_run_summary, ClientStorageError, SqliteClientStorage, SqliteResultExt};
use chatos_local_agent_protocol::{LocalAgentRunListScope, LocalAgentRunPage, LocalAgentRunStatus};

pub(super) async fn list_runs(
    storage: &SqliteClientStorage,
    owner_user_id: &str,
    scope: LocalAgentRunListScope,
    status: Option<LocalAgentRunStatus>,
    updated_after_unix_ms: Option<i64>,
    before_updated_at_unix_ms: Option<i64>,
    before_run_id: Option<&str>,
    limit: u32,
) -> Result<LocalAgentRunPage, ClientStorageError> {
    validate_page(
        owner_user_id,
        scope,
        status,
        updated_after_unix_ms,
        before_updated_at_unix_ms,
        before_run_id,
        limit,
    )?;
    let scope_filter = match scope {
        LocalAgentRunListScope::Active => {
            " AND run.status NOT IN ('succeeded','failed','cancelled') \
             AND NOT (run.status = 'needs_review' AND run.owner_entity_type = 'task' \
               AND EXISTS (SELECT 1 FROM local_tasks task \
                 WHERE task.task_id = run.owner_entity_id AND task.status <> 'blocked'))"
        }
        LocalAgentRunListScope::Terminal => " AND run.status IN ('succeeded','failed','cancelled')",
        LocalAgentRunListScope::All => "",
    };
    let status_filter = if status.is_some() {
        " AND run.status = ?"
    } else {
        ""
    };
    let cursor_filter = if before_updated_at_unix_ms.is_some() {
        " AND (run.updated_at_unix_ms < ? OR \
         (run.updated_at_unix_ms = ? AND run.run_id < ?))"
    } else {
        ""
    };
    let updated_after_filter = if updated_after_unix_ms.is_some() {
        " AND run.updated_at_unix_ms >= ?"
    } else {
        ""
    };
    let sql = format!(
        "SELECT run.run_id, run.owner_user_id, run.owner_entity_type, run.owner_entity_id, \
         run.profile_key, run.input_json, run.status, run.version, run.terminal_outcome_json, \
         run.created_at_unix_ms, run.updated_at_unix_ms FROM local_agent_runs run \
         WHERE run.owner_user_id = ?{scope_filter}{status_filter}{updated_after_filter}{cursor_filter} \
         ORDER BY run.updated_at_unix_ms DESC, run.run_id DESC LIMIT ?"
    );
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(owner_user_id);
    if let Some(status) = status {
        query = query.bind(status.as_str());
    }
    if let Some(updated_after_unix_ms) = updated_after_unix_ms {
        query = query.bind(updated_after_unix_ms);
    }
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
        .map(decode_run_summary)
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
    scope: LocalAgentRunListScope,
    status: Option<LocalAgentRunStatus>,
    updated_after_unix_ms: Option<i64>,
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
    let status_matches_scope = status.is_none_or(|status| match scope {
        LocalAgentRunListScope::Active => !status.is_terminal(),
        LocalAgentRunListScope::Terminal => status.is_terminal(),
        LocalAgentRunListScope::All => true,
    });
    if !(1..=100).contains(&limit)
        || updated_after_unix_ms.is_some_and(|value| value < 0)
        || !status_matches_scope
        || !cursor_valid
    {
        return Err(ClientStorageError::InvalidState(
            "invalid Run list page request".to_string(),
        ));
    }
    Ok(())
}
