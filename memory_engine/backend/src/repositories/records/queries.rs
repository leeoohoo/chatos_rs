// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use sqlx::types::Json;
use sqlx::{Postgres, QueryBuilder};

use crate::db::Db;
use crate::models::{EngineRecord, ThreadRecordsPageResponse, TurnRecordSlice};
use crate::repositories::postgres::decode;

use super::common::{build_record_query, decode_records};
use super::compact_turns;
use super::ListRecordsQuery;

pub async fn count_records(
    db: &Db,
    thread_id: &str,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    role: Option<&str>,
    record_type: Option<&str>,
    summary_status: Option<&str>,
) -> Result<i64, String> {
    build_record_query(
        "SELECT count(*) FROM engine_records",
        thread_id,
        tenant_id,
        source_id,
        role,
        record_type,
        summary_status,
    )
    .build_query_scalar::<i64>()
    .fetch_one(db)
    .await
    .map_err(|error| error.to_string())
}

pub async fn list_records_page(
    db: &Db,
    values: ListRecordsQuery<'_>,
) -> Result<ThreadRecordsPageResponse, String> {
    let base_query = |select| {
        let mut query = build_record_query(
            select,
            values.thread_id,
            values.tenant_id,
            values.source_id,
            values.role,
            values.record_type,
            values.summary_status,
        );
        if let Some(conversation_id) = values
            .conversation_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            query
                .push(" AND BTRIM(COALESCE(data #>> '{metadata,conversation_id}',''))=")
                .push_bind(conversation_id);
        }
        if values.conversation_id_missing {
            query.push(" AND BTRIM(COALESCE(data->'metadata'->>'conversation_id',''))=''");
        }
        if let Some(project_id) = values
            .project_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let raw_project_id = project_id.strip_prefix("project:").unwrap_or(project_id);
            query
                .push(" AND (BTRIM(COALESCE(data->'metadata'->>'memory_project_id',''))=")
                .push_bind(project_id)
                .push(" OR BTRIM(COALESCE(data->'metadata'->>'memory_project_id',''))=")
                .push_bind(raw_project_id)
                .push(")");
        }
        if values.project_id_missing {
            query.push(" AND BTRIM(COALESCE(data->'metadata'->>'memory_project_id',''))=''");
        }
        query
    };
    let total = base_query("SELECT count(*) FROM engine_records")
        .build_query_scalar::<i64>()
        .fetch_one(db)
        .await
        .map_err(|error| error.to_string())?;
    let mut query = base_query("SELECT data FROM engine_records");
    if let Some(cursor) = values.cursor()? {
        query
            .push(" AND (created_at,id)")
            .push(if values.asc { ">(" } else { "<(" })
            .push_bind(cursor.created_at)
            .push(",")
            .push_bind(cursor.id)
            .push(")");
    }
    let limit = values.limit.clamp(1, 2000);
    query
        .push(if values.asc {
            " ORDER BY created_at ASC,id ASC"
        } else {
            " ORDER BY created_at DESC,id DESC"
        })
        .push(" LIMIT ")
        .push_bind(limit + 1)
        .push(" OFFSET ")
        .push_bind(values.offset.max(0));
    let mut items = fetch(db, query).await?;
    let has_more = items.len() > limit as usize;
    if has_more {
        items.truncate(limit as usize);
    }
    Ok(ThreadRecordsPageResponse {
        items,
        total,
        has_more,
    })
}

pub async fn get_record_by_id(
    db: &Db,
    record_id: &str,
    tenant_id: &str,
    source_id: &str,
    thread_id: Option<&str>,
) -> Result<Option<EngineRecord>, String> {
    let mut query = QueryBuilder::<Postgres>::new("SELECT data FROM engine_records WHERE id=");
    query
        .push_bind(record_id)
        .push(" AND tenant_id=")
        .push_bind(tenant_id)
        .push(" AND source_id=")
        .push_bind(source_id);
    if let Some(value) = normalized(thread_id) {
        query.push(" AND thread_id=").push_bind(value);
    }
    query
        .build_query_scalar::<Json<serde_json::Value>>()
        .fetch_optional(db)
        .await
        .map_err(|error| error.to_string())?
        .map(decode)
        .transpose()
}

pub(crate) async fn list_records_by_ids(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    record_ids: &[String],
) -> Result<Vec<EngineRecord>, String> {
    if record_ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_records WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND id=ANY($4)"
    ).bind(tenant_id).bind(source_id).bind(thread_id).bind(record_ids).fetch_all(db).await.map_err(|error| error.to_string())?;
    let records = decode_records(rows)?;
    let mut by_id = records
        .into_iter()
        .map(|r| (r.id.clone(), r))
        .collect::<HashMap<_, _>>();
    record_ids
        .iter()
        .map(|id| {
            by_id.remove(id).ok_or_else(|| {
                format!("frozen summary record is missing from its original scope: {id}")
            })
        })
        .collect()
}

pub async fn list_pending_records(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    limit: i64,
) -> Result<Vec<EngineRecord>, String> {
    let mut query = build_record_query(
        "SELECT data FROM engine_records",
        thread_id,
        Some(tenant_id),
        Some(source_id),
        None,
        None,
        Some("pending"),
    );
    query
        .push(" ORDER BY created_at ASC,id ASC LIMIT ")
        .push_bind(limit.max(1));
    fetch(db, query).await
}

pub async fn list_context_records(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    limit: Option<i64>,
) -> Result<Vec<EngineRecord>, String> {
    list_context_records_inner(db, tenant_id, source_id, thread_id, None, false, limit).await
}

pub async fn list_context_records_for_project(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_subject_id: Option<&str>,
    limit: Option<i64>,
) -> Result<Vec<EngineRecord>, String> {
    list_context_records_inner(
        db,
        tenant_id,
        source_id,
        thread_id,
        project_subject_id,
        true,
        limit,
    )
    .await
}

async fn list_context_records_inner(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_subject_id: Option<&str>,
    filter_project: bool,
    limit: Option<i64>,
) -> Result<Vec<EngineRecord>, String> {
    let mut query =
        QueryBuilder::<Postgres>::new("SELECT data FROM engine_records WHERE tenant_id=");
    query
        .push_bind(tenant_id)
        .push(" AND source_id=")
        .push_bind(source_id)
        .push(" AND thread_id=")
        .push_bind(thread_id)
        .push(" AND summary_status IN ('pending','summarizing','')");
    if filter_project {
        append_context_project_filter(&mut query, project_subject_id);
    }
    let reverse = limit.is_some();
    if let Some(limit) = limit {
        query
            .push(" ORDER BY created_at DESC,id DESC LIMIT ")
            .push_bind(limit.max(1));
    } else {
        query.push(" ORDER BY created_at ASC,id ASC");
    }
    let mut records = fetch(db, query).await?;
    if reverse {
        records.reverse();
    }
    Ok(records)
}

fn append_context_project_filter(
    query: &mut QueryBuilder<Postgres>,
    project_subject_id: Option<&str>,
) {
    const PROJECT_PATH: &str = "data #>> '{metadata,memory_project_id}'";
    if let Some(subject_id) = project_subject_id {
        let project_id = subject_id.strip_prefix("project:").unwrap_or(subject_id);
        query
            .push(" AND (BTRIM(COALESCE(")
            .push(PROJECT_PATH)
            .push(",''))=")
            .push_bind(subject_id)
            .push(" OR BTRIM(COALESCE(")
            .push(PROJECT_PATH)
            .push(",''))=")
            .push_bind(project_id)
            .push(")");
    } else {
        query
            .push(" AND BTRIM(COALESCE(")
            .push(PROJECT_PATH)
            .push(",''))=''");
    }
}

pub async fn list_compact_turn_slices(
    db: &Db,
    thread_id: &str,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    record_type: Option<&str>,
    limit: i64,
    before_turn_id: Option<&str>,
) -> Result<(Vec<TurnRecordSlice>, bool, Option<String>), String> {
    compact_turns::list_compact_turn_slices(
        db,
        thread_id,
        tenant_id,
        source_id,
        record_type,
        limit,
        before_turn_id,
    )
    .await
}

pub async fn list_turn_process_records(
    db: &Db,
    thread_id: &str,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    record_type: Option<&str>,
    turn_id: &str,
) -> Result<Vec<EngineRecord>, String> {
    let turn_id = turn_id.trim();
    if turn_id.is_empty() {
        return Ok(Vec::new());
    }
    let mut query = build_record_query(
        "SELECT data FROM engine_records",
        thread_id,
        tenant_id,
        source_id,
        None,
        record_type,
        None,
    );
    query
        .push(" AND data #>> '{metadata,conversation_turn_id}'=")
        .push_bind(turn_id)
        .push(" ORDER BY created_at,id");
    let records = fetch(db, query).await?;
    let final_id = compact_turns::select_final_assistant_record(&records).map(|r| r.id);
    let mut items = records
        .iter()
        .filter(|record| {
            ((record.role == "assistant" && !compact_turns::is_session_summary_record(record))
                || record.role == "tool")
                && final_id.as_deref() != Some(record.id.as_str())
        })
        .cloned()
        .collect::<Vec<_>>();
    if items.is_empty() {
        if let Some(record) = final_id
            .as_deref()
            .and_then(|id| records.iter().find(|r| r.id == id))
        {
            if compact_turns::parse_tool_call_count(record) > 0
                || compact_turns::parse_thinking_count(record) > 0
            {
                items.push(record.clone());
            }
        }
    }
    Ok(items)
}

async fn fetch(db: &Db, mut query: QueryBuilder<Postgres>) -> Result<Vec<EngineRecord>, String> {
    let rows = query
        .build_query_scalar::<Json<serde_json::Value>>()
        .fetch_all(db)
        .await
        .map_err(|error| error.to_string())?;
    decode_records(rows)
}

fn normalized(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|v| !v.is_empty())
}
