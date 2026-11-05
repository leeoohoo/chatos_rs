// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Deserialize;
use sqlx::types::Json;
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

use crate::db::Db;
use crate::models::{
    now_rfc3339, EngineSubjectMemoryScope, EngineThread, UpsertSubjectMemoryScopeRequest,
};
use crate::repositories::postgres::{decode, json, timestamp};

#[derive(Debug, Clone, Deserialize, sqlx::FromRow, PartialEq, Eq)]
pub struct SubjectMemoryScopeDispatchOutbox {
    pub id: String,
    pub tenant_id: String,
    pub source_id: String,
    pub scope_key: String,
    pub subject_memory_dispatch_version: i64,
    pub subject_memory_dispatch_published_version: i64,
    pub subject_memory_dispatch_consumed_version: i64,
    pub subject_memory_dispatch_pending: bool,
}

const MEMORY_SCHEMA_V2_LABEL: &str = "memory-schema:v2";

pub(crate) fn is_canonical_agent_subject_id(value: &str) -> bool {
    let value = value.trim();
    let Some(agent_id) = value.strip_prefix("agent:") else {
        return false;
    };
    !agent_id.is_empty()
        && !agent_id.chars().any(char::is_control)
        && !matches!(
            agent_id,
            "main-chat" | "main_chat" | "task-execution" | "task_execution"
        )
}

pub async fn ensure_thread_identity_scopes(db: &Db, thread: &EngineThread) -> Result<(), String> {
    for (scope_key, request) in thread_identity_scope_requests(thread)? {
        if get_subject_memory_scope(db, &request.tenant_id, &request.source_id, &scope_key)
            .await?
            .as_ref()
            .is_some_and(|existing| identity_scope_is_current(existing, &request))
        {
            continue;
        }
        upsert_subject_memory_scope(db, scope_key.as_str(), request).await?;
    }
    Ok(())
}

pub async fn ensure_project_identity_scope(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    project_subject_id: &str,
) -> Result<(), String> {
    let project_subject_id = project_subject_id.trim();
    if !project_subject_id.starts_with("project:")
        || project_subject_id.len() == "project:".len()
        || project_subject_id.chars().any(char::is_control)
    {
        return Err("project identity subject is invalid".to_string());
    }
    let scope_key = format!("identity:{project_subject_id}");
    let request = identity_scope_request(
        tenant_id,
        source_id,
        project_subject_id,
        "project_summary",
        "Project shared summary",
    );
    if get_subject_memory_scope(db, tenant_id, source_id, scope_key.as_str())
        .await?
        .as_ref()
        .is_some_and(|existing| identity_scope_is_current(existing, &request))
    {
        return Ok(());
    }
    upsert_subject_memory_scope(db, scope_key.as_str(), request).await?;
    Ok(())
}

fn identity_scope_is_current(
    existing: &EngineSubjectMemoryScope,
    request: &UpsertSubjectMemoryScopeRequest,
) -> bool {
    existing.subject_id == request.subject_id
        && existing.memory_type == request.memory_type
        && existing.source_thread_label == request.source_thread_label
        && existing.relation_subject_id == request.relation_subject_id
        && existing.source_summary_type == request.source_summary_type
        && existing.status == "active"
}

pub(crate) fn thread_identity_scope_requests(
    thread: &EngineThread,
) -> Result<Vec<(String, UpsertSubjectMemoryScopeRequest)>, String> {
    let labels = thread.labels.as_deref().unwrap_or_default();
    if !labels
        .iter()
        .any(|label| label.trim() == MEMORY_SCHEMA_V2_LABEL)
    {
        return Ok(Vec::new());
    }

    let agents = identity_labels(labels, "agent:");
    if agents.len() != 1 {
        return Err("memory-schema:v2 thread requires exactly one agent:<agent_id> label".into());
    }
    let agent_subject = agents[0].clone();
    if !is_canonical_agent_subject_id(agent_subject.as_str()) {
        return Err("memory-schema:v2 agent identity must not use a runtime profile key".into());
    }
    if thread.subject_id.trim() != agent_subject {
        return Err("memory-schema:v2 thread subject_id must equal its agent label".into());
    }

    let projects = identity_labels(labels, "project:");
    if projects.len() > 1 {
        return Err("memory-schema:v2 thread allows at most one project:<project_id> label".into());
    }

    let mut scopes = vec![(
        format!("identity:{agent_subject}"),
        identity_scope_request(
            thread.tenant_id.as_str(),
            thread.source_id.as_str(),
            agent_subject.as_str(),
            "agent_memory",
            "Agent durable memory",
        ),
    )];
    if let Some(project_subject) = projects.first() {
        scopes.push((
            format!("identity:{project_subject}"),
            identity_scope_request(
                thread.tenant_id.as_str(),
                thread.source_id.as_str(),
                project_subject.as_str(),
                "project_summary",
                "Project shared summary",
            ),
        ));
    }
    Ok(scopes)
}

fn identity_labels(labels: &[String], prefix: &str) -> Vec<String> {
    let mut values = labels
        .iter()
        .map(|label| label.trim())
        .filter(|label| label.starts_with(prefix) && label.len() > prefix.len())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

fn identity_scope_request(
    tenant_id: &str,
    source_id: &str,
    subject_id: &str,
    memory_type: &str,
    prompt_title: &str,
) -> UpsertSubjectMemoryScopeRequest {
    UpsertSubjectMemoryScopeRequest {
        tenant_id: tenant_id.to_string(),
        source_id: source_id.to_string(),
        subject_id: subject_id.to_string(),
        memory_type: memory_type.to_string(),
        source_thread_label: subject_id.to_string(),
        relation_subject_id: Some(subject_id.to_string()),
        source_summary_type: Some("thread_incremental".to_string()),
        prompt_title: Some(format!("{prompt_title} for {subject_id}")),
        memory_metadata: Some(serde_json::json!({
            "managed_by": "thread_identity_v2",
            "identity_subject_id": subject_id,
        })),
        status: Some("active".to_string()),
    }
}

pub async fn upsert_subject_memory_scope(
    db: &Db,
    scope_key: &str,
    req: UpsertSubjectMemoryScopeRequest,
) -> Result<EngineSubjectMemoryScope, String> {
    let scope_key = scope_key.trim();
    if scope_key.is_empty() {
        return Err("empty scope_key".to_string());
    }
    let existing = get_subject_memory_scope(db, &req.tenant_id, &req.source_id, scope_key).await?;
    let now = now_rfc3339();
    let status = req.status.unwrap_or_else(|| "active".to_string());
    let active = status == "active";
    let scope = EngineSubjectMemoryScope {
        id: existing
            .as_ref()
            .map(|item| item.id.clone())
            .unwrap_or_else(|| format!("sms_{}", Uuid::new_v4())),
        tenant_id: req.tenant_id,
        source_id: req.source_id,
        scope_key: scope_key.to_string(),
        subject_id: req.subject_id,
        memory_type: req.memory_type,
        source_thread_label: req.source_thread_label,
        relation_subject_id: req.relation_subject_id,
        source_summary_type: req.source_summary_type,
        prompt_title: req.prompt_title,
        memory_metadata: req.memory_metadata,
        status: status.clone(),
        created_at: existing
            .as_ref()
            .map(|item| item.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now.clone(),
        last_run_at: existing.and_then(|item| item.last_run_at),
    };
    let requested_at = active.then(|| timestamp(&now)).transpose()?;
    sqlx::query(
        "INSERT INTO engine_subject_memory_scopes \
         (id,tenant_id,source_id,scope_key,subject_id,memory_type,source_thread_label, \
          relation_subject_id,source_summary_type,status,subject_memory_status, \
          subject_memory_dispatch_pending,subject_memory_dispatch_version, \
          subject_memory_dispatch_requested_at,created_at,updated_at,data) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'idle',$11,$12,$13,$14,$15,$16) \
         ON CONFLICT(tenant_id,source_id,scope_key) DO UPDATE SET subject_id=EXCLUDED.subject_id, \
         memory_type=EXCLUDED.memory_type,source_thread_label=EXCLUDED.source_thread_label, \
         relation_subject_id=EXCLUDED.relation_subject_id,source_summary_type=EXCLUDED.source_summary_type, \
         status=EXCLUDED.status,subject_memory_dispatch_pending=EXCLUDED.subject_memory_dispatch_pending, \
         subject_memory_dispatch_version=engine_subject_memory_scopes.subject_memory_dispatch_version + \
             CASE WHEN EXCLUDED.status='active' THEN 1 ELSE 0 END, \
         subject_memory_dispatch_requested_at=CASE WHEN EXCLUDED.status='active' \
             THEN EXCLUDED.subject_memory_dispatch_requested_at \
             ELSE engine_subject_memory_scopes.subject_memory_dispatch_requested_at END, \
         subject_memory_dispatch_last_error=CASE WHEN EXCLUDED.status='active' THEN NULL \
             ELSE engine_subject_memory_scopes.subject_memory_dispatch_last_error END, \
         updated_at=EXCLUDED.updated_at,data=EXCLUDED.data",
    )
    .bind(&scope.id)
    .bind(&scope.tenant_id)
    .bind(&scope.source_id)
    .bind(&scope.scope_key)
    .bind(&scope.subject_id)
    .bind(&scope.memory_type)
    .bind(&scope.source_thread_label)
    .bind(&scope.relation_subject_id)
    .bind(&scope.source_summary_type)
    .bind(&scope.status)
    .bind(active)
    .bind(if active { 1_i64 } else { 0 })
    .bind(requested_at)
    .bind(timestamp(&scope.created_at)?)
    .bind(timestamp(&scope.updated_at)?)
    .bind(json(&scope)?)
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(scope)
}

pub async fn get_subject_memory_scope(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
) -> Result<Option<EngineSubjectMemoryScope>, String> {
    sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_subject_memory_scopes \
         WHERE tenant_id=$1 AND source_id=$2 AND scope_key=$3",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(scope_key)
    .fetch_optional(db)
    .await
    .map_err(|error| error.to_string())?
    .map(decode)
    .transpose()
}

pub async fn list_active_subject_memory_scopes(
    db: &Db,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    limit: i64,
) -> Result<Vec<EngineSubjectMemoryScope>, String> {
    list_active_subject_memory_scopes_page(db, tenant_id, source_id, limit, 0).await
}

pub async fn list_active_subject_memory_scopes_page(
    db: &Db,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    limit: i64,
    offset: u64,
) -> Result<Vec<EngineSubjectMemoryScope>, String> {
    let mut query = QueryBuilder::<Postgres>::new(
        "SELECT data FROM engine_subject_memory_scopes WHERE status='active'",
    );
    for (column, value) in [
        ("tenant_id", normalized(tenant_id)),
        ("source_id", normalized(source_id)),
    ] {
        if let Some(value) = value {
            query.push(" AND ").push(column).push("=").push_bind(value);
        }
    }
    query
        .push(" ORDER BY updated_at DESC,created_at DESC LIMIT ")
        .push_bind(limit.clamp(1, 10_000))
        .push(" OFFSET ")
        .push_bind(i64::try_from(offset).unwrap_or(i64::MAX));
    decode_many(
        query
            .build_query_scalar()
            .fetch_all(db)
            .await
            .map_err(|error| error.to_string())?,
    )
}

pub async fn list_matching_active_subject_memory_scopes(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_labels: &[String],
    summary_type: &str,
    limit: i64,
) -> Result<Vec<EngineSubjectMemoryScope>, String> {
    if thread_labels.is_empty() {
        return Ok(Vec::new());
    }
    let allow_default = summary_type.trim() == "thread_incremental";
    let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_subject_memory_scopes WHERE tenant_id=$1 AND source_id=$2 \
         AND status='active' AND source_thread_label=ANY($3) AND \
         (source_summary_type=$4 OR ($5 AND COALESCE(source_summary_type,'')='')) \
         ORDER BY updated_at DESC,created_at DESC LIMIT $6",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_labels)
    .bind(summary_type.trim())
    .bind(allow_default)
    .bind(limit.clamp(1, 10_000))
    .fetch_all(db)
    .await
    .map_err(|error| error.to_string())?;
    decode_many(rows)
}

pub async fn touch_subject_memory_scope_run(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
) -> Result<(), String> {
    let now = now_rfc3339();
    sqlx::query(
        "UPDATE engine_subject_memory_scopes SET updated_at=$4, \
         data=jsonb_set(jsonb_set(data,'{last_run_at}',to_jsonb($5::text)), \
         '{updated_at}',to_jsonb($5::text)) WHERE tenant_id=$1 AND source_id=$2 AND scope_key=$3",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(scope_key)
    .bind(timestamp(&now)?)
    .bind(&now)
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn try_acquire_subject_memory_scope_slot(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
    lock_owner: &str,
    lock_timeout_secs: i64,
) -> Result<bool, String> {
    let now = chrono::Utc::now();
    let expires_at = now + chrono::Duration::seconds(lock_timeout_secs.max(30));
    sqlx::query(
        "UPDATE engine_subject_memory_scopes SET subject_memory_status='running',lock_owner=$4, \
         lock_expires_at=$5,updated_at=$6 WHERE tenant_id=$1 AND source_id=$2 AND scope_key=$3 \
         AND status='active' AND (subject_memory_status<>'running' OR lock_expires_at<=$6 OR lock_owner=$4)",
    )
    .bind(tenant_id).bind(source_id).bind(scope_key).bind(lock_owner).bind(expires_at).bind(now)
    .execute(db).await.map(|result| result.rows_affected() > 0).map_err(|error| error.to_string())
}

pub async fn release_subject_memory_scope_slot(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
    lock_owner: &str,
) -> Result<(), String> {
    sqlx::query(
        "UPDATE engine_subject_memory_scopes SET subject_memory_status='idle',lock_owner=NULL, \
         lock_expires_at=NULL,updated_at=now() WHERE tenant_id=$1 AND source_id=$2 \
         AND scope_key=$3 AND lock_owner=$4",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(scope_key)
    .bind(lock_owner)
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn refresh_subject_memory_scope_slot(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
    lock_owner: &str,
    lock_timeout_secs: i64,
) -> Result<bool, String> {
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(lock_timeout_secs.max(30));
    sqlx::query(
        "UPDATE engine_subject_memory_scopes SET lock_expires_at=$5,updated_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND scope_key=$3 \
         AND subject_memory_status='running' AND lock_owner=$4",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(scope_key)
    .bind(lock_owner)
    .bind(expires_at)
    .execute(db)
    .await
    .map(|result| result.rows_affected() > 0)
    .map_err(|error| error.to_string())
}

pub async fn get_pending_subject_memory_dispatch(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
) -> Result<Option<SubjectMemoryScopeDispatchOutbox>, String> {
    load_dispatch(db, tenant_id, source_id, scope_key, true).await
}

pub async fn get_subject_memory_dispatch_state(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
) -> Result<Option<SubjectMemoryScopeDispatchOutbox>, String> {
    load_dispatch(db, tenant_id, source_id, scope_key, false).await
}

pub async fn claim_pending_subject_memory_dispatches(
    db: &Db,
    limit: i64,
) -> Result<Vec<SubjectMemoryScopeDispatchOutbox>, String> {
    sqlx::query_as::<_, SubjectMemoryScopeDispatchOutbox>(
        "WITH candidates AS (SELECT tenant_id,source_id,id FROM engine_subject_memory_scopes \
         WHERE subject_memory_dispatch_pending ORDER BY subject_memory_dispatch_requested_at,updated_at \
         LIMIT $1 FOR UPDATE SKIP LOCKED) UPDATE engine_subject_memory_scopes s SET \
         subject_memory_dispatch_published_version=GREATEST(s.subject_memory_dispatch_published_version,s.subject_memory_dispatch_version), \
         subject_memory_dispatch_published_at=now(),subject_memory_dispatch_last_error=NULL, \
         subject_memory_dispatch_pending=false FROM candidates c WHERE s.tenant_id=c.tenant_id \
         AND s.source_id=c.source_id AND s.id=c.id \
         RETURNING s.id,s.tenant_id,s.source_id,s.scope_key,s.subject_memory_dispatch_version, \
         s.subject_memory_dispatch_published_version,s.subject_memory_dispatch_consumed_version, \
         s.subject_memory_dispatch_pending",
    )
    .bind(limit.clamp(1, 10_000))
    .fetch_all(db)
    .await
    .map_err(|error| error.to_string())
}

pub async fn defer_subject_memory_dispatch(
    db: &Db,
    event: &SubjectMemoryScopeDispatchOutbox,
) -> Result<bool, String> {
    sqlx::query(
        "UPDATE engine_subject_memory_scopes SET subject_memory_dispatch_pending=true \
         WHERE tenant_id=$1 AND source_id=$2 AND scope_key=$3 AND subject_memory_dispatch_version>=$4",
    )
    .bind(&event.tenant_id)
    .bind(&event.source_id)
    .bind(&event.scope_key)
    .bind(event.subject_memory_dispatch_version)
    .execute(db)
    .await
    .map(|result| result.rows_affected() > 0)
    .map_err(|error| error.to_string())
}

pub async fn recover_stale_subject_memory_dispatches(
    db: &Db,
    stale_before: &str,
    limit: i64,
) -> Result<u64, String> {
    let result = sqlx::query(
        "WITH candidates AS (SELECT tenant_id,source_id,id FROM engine_subject_memory_scopes \
         WHERE NOT subject_memory_dispatch_pending \
         AND subject_memory_dispatch_consumed_version<subject_memory_dispatch_version \
         AND subject_memory_dispatch_published_at<=$1 \
         AND COALESCE(subject_memory_dispatch_dead_letter_version,-1)<subject_memory_dispatch_version \
         ORDER BY subject_memory_dispatch_published_at,updated_at LIMIT $2 FOR UPDATE SKIP LOCKED) \
         UPDATE engine_subject_memory_scopes s SET subject_memory_dispatch_pending=true, \
         subject_memory_dispatch_last_error=NULL FROM candidates c WHERE s.tenant_id=c.tenant_id \
         AND s.source_id=c.source_id AND s.id=c.id",
    )
    .bind(crate::repositories::postgres::timestamp(stale_before)?)
    .bind(limit.clamp(1, 10_000))
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(result.rows_affected())
}

pub async fn mark_subject_memory_dispatch_consumed(
    db: &Db,
    event: &SubjectMemoryScopeDispatchOutbox,
) -> Result<bool, String> {
    sqlx::query(
        "UPDATE engine_subject_memory_scopes SET subject_memory_dispatch_consumed_version= \
         GREATEST(subject_memory_dispatch_consumed_version,$4),subject_memory_dispatch_consumed_at=now(), \
         subject_memory_dispatch_last_error=NULL WHERE tenant_id=$1 AND source_id=$2 AND scope_key=$3 \
         AND subject_memory_dispatch_version>=$4",
    ).bind(&event.tenant_id).bind(&event.source_id).bind(&event.scope_key)
      .bind(event.subject_memory_dispatch_version).execute(db).await
      .map(|result| result.rows_affected() > 0).map_err(|error| error.to_string())
}

pub async fn mark_subject_memory_dispatch_failed(
    db: &Db,
    event: &SubjectMemoryScopeDispatchOutbox,
    error: &str,
) -> Result<bool, String> {
    update_dispatch_error(db, event, error, false).await
}

pub async fn rearm_subject_memory_dispatch(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
) -> Result<Option<SubjectMemoryScopeDispatchOutbox>, String> {
    let existing = get_subject_memory_dispatch_state(db, tenant_id, source_id, scope_key).await?;
    let Some(existing) = existing else {
        return Ok(None);
    };
    if existing.subject_memory_dispatch_consumed_version < existing.subject_memory_dispatch_version
    {
        return Ok(Some(existing));
    }
    sqlx::query_as::<_, SubjectMemoryScopeDispatchOutbox>(
        "UPDATE engine_subject_memory_scopes SET subject_memory_dispatch_version=subject_memory_dispatch_version+1, \
         subject_memory_dispatch_requested_at=now(),subject_memory_dispatch_last_error=NULL, \
         subject_memory_dispatch_pending=true WHERE tenant_id=$1 AND source_id=$2 AND scope_key=$3 \
         AND status='active' AND subject_memory_dispatch_consumed_version>=subject_memory_dispatch_version \
         AND COALESCE(subject_memory_dispatch_dead_letter_version,-1)<subject_memory_dispatch_version \
         RETURNING id,tenant_id,source_id,scope_key,subject_memory_dispatch_version, \
         subject_memory_dispatch_published_version,subject_memory_dispatch_consumed_version, \
         subject_memory_dispatch_pending",
    ).bind(tenant_id).bind(source_id).bind(scope_key).fetch_optional(db).await.map_err(|error| error.to_string())
}

pub async fn replay_dead_lettered_subject_memory_dispatch(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
    dead_letter_version: i64,
) -> Result<Option<SubjectMemoryScopeDispatchOutbox>, String> {
    sqlx::query_as::<_, SubjectMemoryScopeDispatchOutbox>(
        "UPDATE engine_subject_memory_scopes SET subject_memory_dispatch_version=subject_memory_dispatch_version+1, \
         subject_memory_dispatch_requested_at=now(),subject_memory_dispatch_last_error=NULL, \
         subject_memory_dispatch_pending=true,subject_memory_dispatch_dead_letter_version=NULL, \
         subject_memory_dispatch_dead_lettered_at=NULL,subject_memory_dispatch_last_failed_at=NULL \
         WHERE tenant_id=$1 AND source_id=$2 AND scope_key=$3 AND status='active' \
         AND subject_memory_dispatch_version=$4 AND subject_memory_dispatch_dead_letter_version=$4 \
         AND subject_memory_dispatch_consumed_version>=$4 AND NOT subject_memory_dispatch_pending \
         RETURNING id,tenant_id,source_id,scope_key,subject_memory_dispatch_version, \
         subject_memory_dispatch_published_version,subject_memory_dispatch_consumed_version, \
         subject_memory_dispatch_pending",
    ).bind(tenant_id).bind(source_id).bind(scope_key).bind(dead_letter_version)
      .fetch_optional(db).await.map_err(|error| error.to_string())
}

async fn load_dispatch(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
    pending_only: bool,
) -> Result<Option<SubjectMemoryScopeDispatchOutbox>, String> {
    sqlx::query_as::<_, SubjectMemoryScopeDispatchOutbox>(
        "SELECT id,tenant_id,source_id,scope_key,subject_memory_dispatch_version, \
         subject_memory_dispatch_published_version,subject_memory_dispatch_consumed_version, \
         subject_memory_dispatch_pending FROM engine_subject_memory_scopes \
         WHERE tenant_id=$1 AND source_id=$2 AND scope_key=$3 \
         AND (NOT $4 OR subject_memory_dispatch_pending)",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(scope_key)
    .bind(pending_only)
    .fetch_optional(db)
    .await
    .map_err(|error| error.to_string())
}

async fn update_dispatch_error(
    db: &Db,
    event: &SubjectMemoryScopeDispatchOutbox,
    error: &str,
    dead_letter: bool,
) -> Result<bool, String> {
    let query = if dead_letter {
        "UPDATE engine_subject_memory_scopes SET subject_memory_dispatch_consumed_version= \
         GREATEST(subject_memory_dispatch_consumed_version,$4),subject_memory_dispatch_dead_letter_version=$4, \
         subject_memory_dispatch_dead_lettered_at=now(),subject_memory_dispatch_last_error=$5 \
         WHERE tenant_id=$1 AND source_id=$2 AND scope_key=$3 AND subject_memory_dispatch_version>=$4"
    } else {
        "UPDATE engine_subject_memory_scopes SET subject_memory_dispatch_last_error=$5, \
         subject_memory_dispatch_last_failed_at=now() WHERE tenant_id=$1 AND source_id=$2 \
         AND scope_key=$3 AND subject_memory_dispatch_version>=$4"
    };
    sqlx::query(query)
        .bind(&event.tenant_id)
        .bind(&event.source_id)
        .bind(&event.scope_key)
        .bind(event.subject_memory_dispatch_version)
        .bind(error)
        .execute(db)
        .await
        .map(|result| result.rows_affected() > 0)
        .map_err(|error| error.to_string())
}

fn decode_many(
    rows: Vec<Json<serde_json::Value>>,
) -> Result<Vec<EngineSubjectMemoryScope>, String> {
    rows.into_iter().map(decode).collect()
}

fn normalized(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod identity_tests {
    use super::{is_canonical_agent_subject_id, thread_identity_scope_requests};
    use crate::models::EngineThread;

    fn thread(subject_id: &str, labels: Vec<&str>) -> EngineThread {
        EngineThread {
            id: "thread-1".into(),
            tenant_id: "tenant-1".into(),
            source_id: "chatos".into(),
            subject_id: subject_id.into(),
            thread_type: "agent_task".into(),
            external_thread_id: None,
            title: None,
            labels: Some(labels.into_iter().map(ToOwned::to_owned).collect()),
            metadata: None,
            status: "active".into(),
            summary_status: "idle".into(),
            summary_job_run_id: None,
            summary_locked_at: None,
            summary_lock_expires_at: None,
            pending_record_count: 0,
            pending_summary_tokens: 0,
            created_at: "2026-11-04T00:00:00Z".into(),
            updated_at: "2026-11-04T00:00:00Z".into(),
            archived_at: None,
        }
    }

    #[test]
    fn v2_thread_registers_agent_and_project_scopes() {
        let requests = thread_identity_scope_requests(&thread(
            "agent:a-1",
            vec!["memory-schema:v2", "agent:a-1", "project:p-1", "room:r-1"],
        ))
        .expect("identity scopes");

        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].1.memory_type, "agent_memory");
        assert_eq!(requests[0].1.subject_id, "agent:a-1");
        assert_eq!(requests[1].1.memory_type, "project_summary");
        assert_eq!(requests[1].1.subject_id, "project:p-1");
        assert!(requests
            .iter()
            .all(|(_, request)| !request.subject_id.starts_with("room:")));
    }

    #[test]
    fn v2_thread_rejects_missing_or_conflicting_agent_identity() {
        assert!(thread_identity_scope_requests(&thread(
            "agent:a-1",
            vec!["memory-schema:v2", "project:p-1"],
        ))
        .is_err());
        assert!(thread_identity_scope_requests(&thread(
            "agent:a-2",
            vec!["memory-schema:v2", "agent:a-1"],
        ))
        .is_err());
    }

    #[test]
    fn runtime_profile_keys_are_not_agent_identities() {
        for subject in [
            "agent:main-chat",
            "agent:main_chat",
            "agent:task-execution",
            "agent:task_execution",
        ] {
            assert!(!is_canonical_agent_subject_id(subject));
            assert!(thread_identity_scope_requests(&thread(
                subject,
                vec!["memory-schema:v2", subject],
            ))
            .is_err());
        }
        assert!(is_canonical_agent_subject_id("agent:jiguli"));
        assert!(is_canonical_agent_subject_id("agent:agent-123"));
    }
}
