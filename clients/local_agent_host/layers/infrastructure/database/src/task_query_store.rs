// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{ClientStorageError, SqliteClientStorage, SqliteResultExt};
use chatos_local_agent_protocol::{
    LocalTaskGraphListScope, LocalTaskGraphPage, LocalTaskGraphStatus, LocalTaskGraphSummary,
};
use sqlx::Row;
use std::str::FromStr;

pub(super) async fn list_graphs(
    storage: &SqliteClientStorage,
    owner_user_id: &str,
    scope: LocalTaskGraphListScope,
    before_updated_at_unix_ms: Option<i64>,
    before_graph_id: Option<&str>,
    limit: u32,
) -> Result<LocalTaskGraphPage, ClientStorageError> {
    validate_page(
        owner_user_id,
        before_updated_at_unix_ms,
        before_graph_id,
        limit,
    )?;
    let scope_filter = match scope {
        LocalTaskGraphListScope::Active => " AND status IN ('pending','running')",
        LocalTaskGraphListScope::Terminal => " AND status IN ('succeeded','failed','cancelled')",
        LocalTaskGraphListScope::All => "",
    };
    let cursor_filter = if before_updated_at_unix_ms.is_some() {
        " AND (updated_at_unix_ms < ? OR (updated_at_unix_ms = ? AND graph_id < ?))"
    } else {
        ""
    };
    let sql = format!(
        "WITH summaries AS (\
           SELECT g.graph_id, g.owner_user_id, g.source_entity_type, g.source_entity_id, \
             g.created_at_unix_ms, MAX(t.updated_at_unix_ms) AS updated_at_unix_ms, \
             COUNT(*) AS task_count, \
             SUM(CASE WHEN t.status = 'succeeded' THEN 1 ELSE 0 END) AS succeeded_task_count, \
             CASE \
               WHEN SUM(CASE WHEN t.status <> 'succeeded' THEN 1 ELSE 0 END) = 0 \
                 THEN 'succeeded' \
               WHEN SUM(CASE WHEN t.status IN ('pending','ready','running') THEN 1 ELSE 0 END) > 0 \
                 THEN CASE WHEN SUM(CASE WHEN t.status NOT IN ('pending','ready') THEN 1 ELSE 0 END) > 0 \
                   THEN 'running' ELSE 'pending' END \
               WHEN SUM(CASE WHEN t.status = 'failed' THEN 1 ELSE 0 END) > 0 THEN 'failed' \
               WHEN SUM(CASE WHEN t.status = 'cancelled' THEN 1 ELSE 0 END) > 0 THEN 'cancelled' \
               ELSE 'failed' \
             END AS status \
           FROM local_task_graphs g JOIN local_tasks t ON t.graph_id = g.graph_id \
           WHERE g.owner_user_id = ? GROUP BY g.graph_id\
         ) SELECT graph_id, owner_user_id, source_entity_type, source_entity_id, status, \
           task_count, succeeded_task_count, created_at_unix_ms, updated_at_unix_ms \
         FROM summaries WHERE 1 = 1{scope_filter}{cursor_filter} \
         ORDER BY updated_at_unix_ms DESC, graph_id DESC LIMIT ?"
    );
    let mut query = sqlx::query(&sql).bind(owner_user_id);
    if let (Some(timestamp), Some(graph_id)) = (before_updated_at_unix_ms, before_graph_id) {
        query = query.bind(timestamp).bind(timestamp).bind(graph_id);
    }
    let mut connection = storage.pool.acquire().await.db()?;
    let rows = query
        .bind(i64::from(limit) + 1)
        .fetch_all(&mut *connection)
        .await
        .db()?;
    let mut graphs = rows
        .into_iter()
        .map(|row| {
            let status: String = row.try_get("status").db()?;
            Ok(LocalTaskGraphSummary {
                graph_id: row.try_get("graph_id").db()?,
                owner_user_id: row.try_get("owner_user_id").db()?,
                source_entity_type: row.try_get("source_entity_type").db()?,
                source_entity_id: row.try_get("source_entity_id").db()?,
                status: LocalTaskGraphStatus::from_str(&status)
                    .map_err(ClientStorageError::InvalidState)?,
                task_count: count(row.try_get("task_count").db()?)?,
                succeeded_task_count: count(row.try_get("succeeded_task_count").db()?)?,
                created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
                updated_at_unix_ms: row.try_get("updated_at_unix_ms").db()?,
            })
        })
        .collect::<Result<Vec<_>, ClientStorageError>>()?;
    let has_more = graphs.len() > limit as usize;
    graphs.truncate(limit as usize);
    let (next_before_updated_at_unix_ms, next_before_graph_id) = if has_more {
        let last = graphs.last().expect("positive validated page limit");
        (Some(last.updated_at_unix_ms), Some(last.graph_id.clone()))
    } else {
        (None, None)
    };
    Ok(LocalTaskGraphPage {
        graphs,
        next_before_updated_at_unix_ms,
        next_before_graph_id,
    })
}

fn count(value: i64) -> Result<u32, ClientStorageError> {
    u32::try_from(value)
        .map_err(|_| ClientStorageError::InvalidState("invalid Task Graph count".to_string()))
}

fn validate_page(
    owner_user_id: &str,
    before_updated_at_unix_ms: Option<i64>,
    before_graph_id: Option<&str>,
    limit: u32,
) -> Result<(), ClientStorageError> {
    let owner_valid = !owner_user_id.trim().is_empty()
        && owner_user_id.len() <= 256
        && !owner_user_id.chars().any(char::is_control);
    let cursor_valid = match (before_updated_at_unix_ms, before_graph_id) {
        (None, None) => true,
        (Some(timestamp), Some(graph_id)) => {
            timestamp >= 0
                && !graph_id.trim().is_empty()
                && graph_id.len() <= 256
                && !graph_id.chars().any(char::is_control)
        }
        _ => false,
    };
    if !owner_valid || !cursor_valid || !(1..=100).contains(&limit) {
        return Err(ClientStorageError::InvalidState(
            "invalid Task Graph list page request".to_string(),
        ));
    }
    Ok(())
}
