// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use sqlx::types::Json;
use sqlx::{Postgres, QueryBuilder};

use crate::db::Db;
use crate::models::{EngineJobRun, EngineThread};
use crate::repositories::postgres::decode;

pub async fn list_job_runs(
    db: &Db,
    job_type: Option<&str>,
    trigger_type: Option<&str>,
    thread_id: Option<&str>,
    status: Option<&str>,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    limit: i64,
) -> Result<Vec<EngineJobRun>, String> {
    let mut query = QueryBuilder::<Postgres>::new("SELECT data FROM engine_job_runs WHERE TRUE");
    for (column, value) in [
        ("job_type", normalized(job_type)),
        ("trigger_type", normalized(trigger_type)),
        ("thread_id", normalized(thread_id)),
        ("status", normalized(status)),
        ("tenant_id", normalized(tenant_id)),
        ("source_id", normalized(source_id)),
    ] {
        if let Some(value) = value {
            query.push(" AND ").push(column).push("=").push_bind(value);
        }
    }
    query
        .push(" ORDER BY started_at DESC,id LIMIT ")
        .push_bind(limit.clamp(1, 1000));
    let items = query
        .build_query_scalar::<Json<serde_json::Value>>()
        .fetch_all(db)
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(decode)
        .collect::<Result<Vec<EngineJobRun>, _>>()?;
    enrich_thread_display_names(db, items).await
}

pub async fn get_job_run_by_id(db: &Db, job_run_id: &str) -> Result<Option<EngineJobRun>, String> {
    sqlx::query_scalar::<_, Json<serde_json::Value>>("SELECT data FROM engine_job_runs WHERE id=$1")
        .bind(job_run_id)
        .fetch_optional(db)
        .await
        .map_err(|error| error.to_string())?
        .map(decode)
        .transpose()
}

async fn enrich_thread_display_names(
    db: &Db,
    mut items: Vec<EngineJobRun>,
) -> Result<Vec<EngineJobRun>, String> {
    let ids = items
        .iter()
        .filter_map(|run| normalized(run.thread_id.as_deref()))
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(items);
    }
    let threads = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_threads WHERE id=ANY($1)",
    )
    .bind(&ids)
    .fetch_all(db)
    .await
    .map_err(|error| error.to_string())?
    .into_iter()
    .map(decode)
    .collect::<Result<Vec<EngineThread>, _>>()?;
    let names = threads
        .into_iter()
        .map(|thread| (thread.id.clone(), preferred_thread_display_name(&thread)))
        .collect::<HashMap<_, _>>();
    for run in &mut items {
        if let Some(name) = run
            .thread_id
            .as_ref()
            .and_then(|thread_id| names.get(thread_id))
        {
            run.thread_display_name = Some(name.clone());
        }
    }
    Ok(items)
}

fn preferred_thread_display_name(thread: &EngineThread) -> String {
    thread
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| thread.subject_id.clone())
}

fn normalized(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::preferred_thread_display_name;
    use crate::models::EngineThread;

    #[test]
    fn preferred_name_uses_title_then_subject() {
        let mut thread = EngineThread {
            id: "thread-a".into(),
            tenant_id: "tenant-a".into(),
            source_id: "source-a".into(),
            subject_id: "subject-a".into(),
            thread_type: "chat".into(),
            external_thread_id: None,
            title: Some(" Useful title ".into()),
            labels: None,
            metadata: None,
            status: "active".into(),
            summary_status: "idle".into(),
            summary_job_run_id: None,
            summary_locked_at: None,
            summary_lock_expires_at: None,
            pending_record_count: 0,
            pending_summary_tokens: 0,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            archived_at: None,
        };
        assert_eq!(preferred_thread_display_name(&thread), "Useful title");
        thread.title = None;
        assert_eq!(preferred_thread_display_name(&thread), "subject-a");
    }
}
