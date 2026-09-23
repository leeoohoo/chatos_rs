// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::types::Json;
use sqlx::{Postgres, QueryBuilder};

use crate::db::Db;
use crate::models::EngineThread;
use crate::repositories::postgres::decode;

use super::common::{decode_threads, normalize_optional_text};
use super::ListThreadsQuery;

pub async fn get_thread_by_id(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
) -> Result<Option<EngineThread>, String> {
    get_thread(db, Some(tenant_id), Some(source_id), thread_id).await
}

pub async fn get_thread(
    db: &Db,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    thread_id: &str,
) -> Result<Option<EngineThread>, String> {
    let mut query = QueryBuilder::<Postgres>::new("SELECT data FROM engine_threads WHERE id=");
    query.push_bind(thread_id);
    if let Some(value) = normalize_optional_text(tenant_id) {
        query.push(" AND tenant_id=").push_bind(value);
    }
    if let Some(value) = normalize_optional_text(source_id) {
        query.push(" AND source_id=").push_bind(value);
    }
    query
        .build_query_scalar::<Json<serde_json::Value>>()
        .fetch_optional(db)
        .await
        .map_err(|error| error.to_string())?
        .map(decode)
        .transpose()
}

pub async fn list_threads_with_pending_records_by_token_threshold(
    db: &Db,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    token_threshold: i64,
    limit: i64,
) -> Result<Vec<EngineThread>, String> {
    let mut query = QueryBuilder::<Postgres>::new(
        "SELECT data FROM engine_threads WHERE summary_status='pending' AND pending_summary_tokens>=",
    );
    query.push_bind(token_threshold.max(1));
    append_optional_eq(&mut query, "tenant_id", tenant_id);
    append_optional_eq(&mut query, "source_id", source_id);
    query
        .push(" ORDER BY updated_at ASC,created_at ASC LIMIT ")
        .push_bind(limit.clamp(1, 500));
    fetch_threads(db, query).await
}

pub async fn list_threads_by_label(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_label: &str,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<EngineThread>, String> {
    let label = thread_label.trim();
    if label.is_empty() {
        return Ok(Vec::new());
    }
    let mut query =
        QueryBuilder::<Postgres>::new("SELECT data FROM engine_threads WHERE tenant_id=");
    query
        .push_bind(tenant_id)
        .push(" AND source_id=")
        .push_bind(source_id)
        .push(" AND data->'labels' ? ")
        .push_bind(label);
    append_optional_eq(&mut query, "status", status);
    query
        .push(" ORDER BY updated_at DESC,created_at DESC LIMIT ")
        .push_bind(limit.clamp(1, 5_000))
        .push(" OFFSET ")
        .push_bind(offset.max(0));
    fetch_threads(db, query).await
}

pub async fn list_threads(
    db: &Db,
    values: ListThreadsQuery<'_>,
) -> Result<Vec<EngineThread>, String> {
    let mut query = QueryBuilder::<Postgres>::new("SELECT data FROM engine_threads WHERE TRUE");
    append_optional_eq(&mut query, "tenant_id", values.tenant_id);
    append_optional_eq(&mut query, "source_id", values.source_id);
    append_optional_eq(&mut query, "subject_id", values.subject_id);
    append_optional_eq(&mut query, "external_thread_id", values.external_thread_id);
    append_json_aliases(
        &mut query,
        values.contact_id,
        &[
            "{metadata,legacy_session_mapping,contact_id}",
            "{metadata,source_metadata,chat_runtime,contact_id}",
            "{metadata,source_metadata,chat_runtime,contactId}",
            "{metadata,source_metadata,contact,contact_id}",
            "{metadata,source_metadata,contact,contactId}",
            "{metadata,source_metadata,ui_contact,contact_id}",
            "{metadata,source_metadata,ui_contact,contactId}",
        ],
        false,
    );
    append_json_aliases(
        &mut query,
        values.project_id,
        &[
            "{metadata,legacy_session_mapping,project_id}",
            "{metadata,source_metadata,chat_runtime,project_id}",
            "{metadata,source_metadata,chat_runtime,projectId}",
        ],
        false,
    );
    append_json_aliases(
        &mut query,
        values.agent_id,
        &[
            "{metadata,legacy_session_mapping,agent_id}",
            "{metadata,source_metadata,chat_runtime,contact_agent_id}",
            "{metadata,source_metadata,chat_runtime,contactAgentId}",
            "{metadata,source_metadata,contact,agent_id}",
            "{metadata,source_metadata,contact,agentId}",
            "{metadata,source_metadata,ui_contact,agent_id}",
            "{metadata,source_metadata,ui_contact,agentId}",
            "{metadata,source_metadata,ui_chat_selection,selected_agent_id}",
            "{metadata,source_metadata,ui_chat_selection,selectedAgentId}",
        ],
        false,
    );
    append_json_aliases(
        &mut query,
        values.mapping_source,
        &["{metadata,mapping_source}"],
        false,
    );
    append_json_aliases(
        &mut query,
        values.mapping_version,
        &["{metadata,mapping_version}"],
        false,
    );
    if let Some(value) = normalize_optional_text(values.thread_label) {
        query.push(" AND data->'labels' ? ").push_bind(value);
    }
    append_optional_eq(&mut query, "status", values.status);
    append_json_aliases(
        &mut query,
        values.session_id,
        &["id", "{metadata,legacy_session_mapping,session_id}"],
        true,
    );
    if let Some(cursor) = values.cursor()? {
        query
            .push(" AND (updated_at,created_at,id)<(")
            .push_bind(cursor.updated_at)
            .push(",")
            .push_bind(cursor.created_at)
            .push(",")
            .push_bind(cursor.id)
            .push(")");
    }
    query
        .push(" ORDER BY updated_at DESC,created_at DESC,id DESC LIMIT ")
        .push_bind(values.limit.clamp(1, 10_000))
        .push(" OFFSET ")
        .push_bind(values.offset.max(0));
    fetch_threads(db, query).await
}

fn append_optional_eq<'a>(
    query: &mut QueryBuilder<'a, Postgres>,
    column: &str,
    value: Option<&'a str>,
) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        query.push(" AND ").push(column).push("=").push_bind(value);
    }
}

fn append_json_aliases<'a>(
    query: &mut QueryBuilder<'a, Postgres>,
    value: Option<&'a str>,
    paths: &[&str],
    first_is_column: bool,
) {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };
    query.push(" AND (");
    for (index, path) in paths.iter().enumerate() {
        if index > 0 {
            query.push(" OR ");
        }
        if first_is_column && index == 0 {
            query.push(*path);
        } else {
            query.push("data #>> '").push(*path).push("'");
        }
        query.push("=").push_bind(value);
    }
    query.push(")");
}

async fn fetch_threads(
    db: &Db,
    mut query: QueryBuilder<'_, Postgres>,
) -> Result<Vec<EngineThread>, String> {
    let rows = query
        .build_query_scalar::<Json<serde_json::Value>>()
        .fetch_all(db)
        .await
        .map_err(|error| error.to_string())?;
    decode_threads(rows)
}
