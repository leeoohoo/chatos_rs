use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures_util::{stream, Stream, TryStreamExt};
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::FindOptions;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::convert::Infallible;
use std::time::Duration;
use crate::jobs;
use crate::repositories::{contacts, projects, sessions};
use crate::services::memory_engine_client;

use super::{
    ensure_admin, ensure_session_access, require_auth, resolve_scope_user_id, SharedState,
};

#[derive(Debug, Deserialize)]
pub(super) struct RunJobRequest {
    user_id: Option<String>,
    session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RunScopedReviewRepairRequest {
    user_id: Option<String>,
    project_id: Option<String>,
    contact_id: Option<String>,
    agent_id: Option<String>,
}

pub(super) async fn run_summary_once(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<RunJobRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let scope_user_id = resolve_scope_user_id(&auth, req.user_id);
    let result = if let Some(session_id) = req.session_id.as_deref() {
        if let Err(err) = ensure_session_access(state.as_ref(), &auth, session_id).await {
            return err;
        }
        memory_engine_client::run_thread_summary(&state.config, scope_user_id.as_str(), session_id)
            .await
            .map(|r| json!({
                "session_id": session_id,
                "engine": true,
                "backend": "memory_engine",
                "result": r
            }))
    } else {
        memory_engine_client::run_pending_summaries_once(
            &state.config,
            Some(scope_user_id.as_str()),
            None,
        )
            .await
            .map(|r| json!({
                "engine": true,
                "backend": "memory_engine",
                "result": r
            }))
    };

    match result {
        Ok(data) => (StatusCode::OK, Json(json!({"ok": true, "data": data}))),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "ok": false,
                "backend": "memory_engine",
                "error": err
            })),
        ),
    }
}

pub(super) async fn run_review_repair_once(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<RunScopedReviewRepairRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let scope_user_id = resolve_scope_user_id(&auth, req.user_id);
    let project_id = req
        .project_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "0".to_string());
    let contact_id = req
        .contact_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let agent_id = req
        .agent_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    if contact_id.is_none() && agent_id.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "ok": false,
                "error": "contact_id 或 agent_id 至少要提供一个"
            })),
        );
    }

    match jobs::review_repair::run_once_for_scope(
        &state.config,
        scope_user_id.as_str(),
        project_id.as_str(),
        contact_id.as_deref(),
        agent_id.as_deref(),
    )
    .await
    {
        Ok(data) => (
            StatusCode::OK,
            Json(json!({
                "ok": true,
                "backend": "memory_engine",
                "data": data
            })),
        ),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "ok": false,
                "backend": "memory_engine",
                "error": err
            })),
        ),
    }
}

pub(super) async fn get_review_repair_status(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(req): Query<RunScopedReviewRepairRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let scope_user_id = resolve_scope_user_id(&auth, req.user_id);

    let project_id = req
        .project_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "0".to_string());
    let contact_id = req
        .contact_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let agent_id = req
        .agent_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    if contact_id.is_none() && agent_id.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "ok": false,
                "error": "contact_id 或 agent_id 至少要提供一个"
            })),
        );
    }

    match jobs::review_repair::get_status_for_scope(
        &state.config,
        scope_user_id.as_str(),
        project_id.as_str(),
        contact_id.as_deref(),
        agent_id.as_deref(),
    )
    .await
    {
        Ok(data) => (
            StatusCode::OK,
            Json(json!({
                "ok": true,
                "backend": "memory_engine",
                "data": data
            })),
        ),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "ok": false,
                "backend": "memory_engine",
                "error": err
            })),
        ),
    }
}

pub(super) async fn run_rollup_once(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<RunJobRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let scope_user_id = resolve_scope_user_id(&auth, req.user_id);
    let rollup_cfg = match memory_engine_client::get_global_rollup_job_config(
        &state.config,
        scope_user_id.as_str(),
    )
    .await
    {
        Ok(cfg) => cfg,
        Err(err) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({
                    "ok": false,
                    "backend": "memory_engine",
                    "error": err
                })),
            )
        }
    };

    match memory_engine_client::run_pending_rollups_once(
        &state.config,
        scope_user_id.as_str(),
        &rollup_cfg,
    )
    .await
    {
        Ok(data) => (
            StatusCode::OK,
            Json(json!({
                "ok": true,
                "backend": "memory_engine",
                "data": data
            })),
        ),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "ok": false,
                "backend": "memory_engine",
                "error": err
            })),
        ),
    }
}

pub(super) async fn run_agent_memory_once(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<RunJobRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let scope_user_id = resolve_scope_user_id(&auth, req.user_id);
    match memory_engine_client::run_subject_memory_scopes_once(
        &state.config,
        Some(scope_user_id.as_str()),
        Some("memory_server"),
        Some(50),
    )
    .await
    {
        Ok(data) => (
            StatusCode::OK,
            Json(json!({
                "ok": true,
                "backend": "memory_engine",
                "data": {
                    "processed_agents": data.processed_scopes,
                    "summarized_agents": data.generated_scopes,
                    "generated_recalls": data.generated_memories,
                    "marked_source_summaries": data.marked_source_summaries,
                    "marked_source_recalls": data.marked_source_memories,
                    "failed_agents": data.failed_scopes
                }
            })),
        ),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "ok": false,
                "backend": "memory_engine",
                "error": err
            })),
        ),
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct JobRunsQuery {
    job_type: Option<String>,
    session_id: Option<String>,
    status: Option<String>,
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct JobRunsStreamQuery {
    job_type: Option<String>,
    session_id: Option<String>,
    status: Option<String>,
    limit: Option<i64>,
}

fn normalize_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
}

fn metadata_string(metadata: Option<&serde_json::Value>, path: &[&str]) -> Option<String> {
    let mut cursor = metadata?;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    normalize_text(cursor.as_str())
}

fn bson_text(value: Option<&Bson>) -> Option<String> {
    match value {
        Some(Bson::String(v)) => normalize_text(Some(v.as_str())),
        Some(Bson::Int32(v)) => Some(v.to_string()),
        Some(Bson::Int64(v)) => Some(v.to_string()),
        Some(Bson::Double(v)) => Some(v.to_string()),
        Some(Bson::Boolean(v)) => Some(v.to_string()),
        _ => None,
    }
}

fn bson_json_value(value: Option<&Bson>) -> Option<Value> {
    match value {
        Some(Bson::Document(doc)) => {
            mongodb::bson::from_bson::<Value>(Bson::Document(doc.clone())).ok()
        }
        Some(Bson::String(raw)) => serde_json::from_str::<Value>(raw)
            .ok()
            .or_else(|| Some(Value::String(raw.clone()))),
        _ => None,
    }
}

fn compact_error_text(raw: &str, max_chars: usize) -> String {
    let text = raw.trim();
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    format!("{}...", text.chars().take(max_chars).collect::<String>())
}

fn combine_detail(base: &str, extra: Option<String>) -> Option<String> {
    match extra {
        Some(v) => Some(format!("{base}+{v}")),
        None => Some(base.to_string()),
    }
}

fn contact_id_from_metadata(metadata: Option<&serde_json::Value>) -> Option<String> {
    metadata_string(metadata, &["contact", "contact_id"])
        .or_else(|| metadata_string(metadata, &["contact", "contactId"]))
        .or_else(|| metadata_string(metadata, &["ui_contact", "contact_id"]))
        .or_else(|| metadata_string(metadata, &["ui_contact", "contactId"]))
        .or_else(|| metadata_string(metadata, &["chat_runtime", "contact_id"]))
        .or_else(|| metadata_string(metadata, &["chat_runtime", "contactId"]))
}

fn agent_id_from_metadata(metadata: Option<&serde_json::Value>) -> Option<String> {
    metadata_string(metadata, &["contact", "agent_id"])
        .or_else(|| metadata_string(metadata, &["contact", "agentId"]))
        .or_else(|| metadata_string(metadata, &["ui_contact", "agent_id"]))
        .or_else(|| metadata_string(metadata, &["ui_contact", "agentId"]))
        .or_else(|| metadata_string(metadata, &["ui_chat_selection", "selected_agent_id"]))
        .or_else(|| metadata_string(metadata, &["ui_chat_selection", "selectedAgentId"]))
        .or_else(|| metadata_string(metadata, &["chat_runtime", "contact_agent_id"]))
        .or_else(|| metadata_string(metadata, &["chat_runtime", "contactAgentId"]))
}

fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

fn metadata_i64(metadata: Option<&serde_json::Value>, key: &str) -> Option<i64> {
    metadata.and_then(|value| value.get(key)).and_then(|value| value.as_i64())
}

fn metadata_string_value(metadata: Option<&serde_json::Value>, key: &str) -> Option<String> {
    metadata
        .and_then(|value| value.get(key))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn engine_job_run_session_id(item: &memory_engine_client::EngineJobRun) -> Option<String> {
    item.thread_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn compat_job_type(item: &memory_engine_client::EngineJobRun) -> String {
    metadata_string_value(item.metadata.as_ref(), "compat_job_type").unwrap_or_else(|| {
        match item.job_type.as_str() {
            "summary" => {
                if item.thread_id.is_some() {
                    "summary_l0".to_string()
                } else {
                    "summary_batch".to_string()
                }
            }
            "rollup" => {
                if item.thread_id.is_some() {
                    "summary_rollup".to_string()
                } else {
                    "summary_rollup_batch".to_string()
                }
            }
            "subject_memory" => "agent_memory".to_string(),
            "thread_repair" => {
                if item.thread_id.is_some() {
                    "summary_review_repair".to_string()
                } else {
                    "summary_review_repair_batch".to_string()
                }
            }
            other => other.to_string(),
        }
    })
}

fn compat_trigger_type(item: &memory_engine_client::EngineJobRun) -> String {
    metadata_string_value(item.metadata.as_ref(), "compat_trigger_type")
        .unwrap_or_else(|| item.trigger_type.clone())
}

fn engine_job_run_to_compat_row(item: &memory_engine_client::EngineJobRun) -> serde_json::Value {
    let session_id = engine_job_run_session_id(item);
    let metadata = item.metadata.as_ref();

    let pending_before_count = metadata_i64(metadata, "pending_before_count");
    let selected_count = metadata_i64(metadata, "selected_count")
        .or(Some(item.processed_count));
    let marked_count = metadata_i64(metadata, "marked_count")
        .or(Some(item.success_count));
    let pending_after_count = metadata_i64(metadata, "pending_after_count");

    json!({
        "id": item.id,
        "job_type": compat_job_type(item),
        "session_id": session_id,
        "status": item.status,
        "trigger_type": compat_trigger_type(item),
        "input_count": item.input_count,
        "output_count": item.output_count,
        "pending_before_count": pending_before_count,
        "selected_count": selected_count,
        "marked_count": marked_count,
        "pending_after_count": pending_after_count,
        "error_message": item.error_message,
        "started_at": item.started_at,
        "finished_at": item.finished_at,
    })
}

fn looks_like_short_session_prefix(raw: &str) -> bool {
    let value = raw.trim();
    let len = value.chars().count();
    if !(6..36).contains(&len) {
        return false;
    }
    value
        .chars()
        .all(|ch| ch.is_ascii_hexdigit() || ch == '-' || ch == '_')
}

async fn find_session_ids_by_prefix(
    state: &SharedState,
    prefix: &str,
    limit: i64,
) -> Result<Vec<String>, String> {
    let options = FindOptions::builder()
        .projection(doc! {"_id": 0, "id": 1})
        .limit(Some(limit.max(1).min(10)))
        .build();
    let cursor = state
        .pool
        .collection::<mongodb::bson::Document>("sessions")
        .find(doc! {"id": {"$regex": format!("^{}", prefix)}})
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;
    let docs: Vec<mongodb::bson::Document> =
        cursor.try_collect().await.map_err(|e| e.to_string())?;
    Ok(docs
        .into_iter()
        .filter_map(|doc| doc.get_str("id").ok().map(|value| value.to_string()))
        .collect())
}

#[derive(Debug, Clone)]
struct SessionLookupSession {
    user_id: String,
    project_id: Option<String>,
    title: Option<String>,
    metadata: Option<Value>,
}

async fn get_session_lookup_session_by_id(
    state: &SharedState,
    session_id: &str,
) -> Result<(Option<SessionLookupSession>, Option<String>), String> {
    match sessions::get_session_by_id(&state.pool, session_id).await {
        Ok(Some(session)) => Ok((
            Some(SessionLookupSession {
                user_id: session.user_id,
                project_id: session.project_id,
                title: session.title,
                metadata: session.metadata,
            }),
            None,
        )),
        Ok(None) => Ok((None, None)),
        Err(err) => {
            let raw_doc = state
                .pool
                .collection::<Document>("sessions")
                .find_one(doc! {"id": session_id})
                .await
                .map_err(|raw_err| {
                    format!("typed lookup failed: {}; raw lookup failed: {raw_err}", err)
                })?;

            if let Some(doc) = raw_doc {
                let fallback = SessionLookupSession {
                    user_id: bson_text(doc.get("user_id")).unwrap_or_default(),
                    project_id: bson_text(doc.get("project_id")),
                    title: bson_text(doc.get("title")),
                    metadata: bson_json_value(doc.get("metadata")),
                };
                let detail = format!(
                    "typed_decode_fallback:{}",
                    compact_error_text(err.as_str(), 120)
                );
                Ok((Some(fallback), Some(detail)))
            } else {
                Ok((None, None))
            }
        }
    }
}

#[derive(Debug, Clone)]
struct SessionLookupResult {
    session: Option<SessionLookupSession>,
    match_mode: &'static str,
    detail: Option<String>,
    effective_session_id: Option<String>,
    raw_len: usize,
    trimmed_len: usize,
}

async fn resolve_session_for_job_run(
    state: &SharedState,
    session_id_raw: &str,
) -> Result<SessionLookupResult, String> {
    let raw_len = session_id_raw.chars().count();
    let trimmed = session_id_raw.trim();
    let trimmed_len = trimmed.chars().count();

    let (session_exact, exact_fallback_detail) =
        get_session_lookup_session_by_id(state, session_id_raw).await?;
    if let Some(session) = session_exact {
        return Ok(SessionLookupResult {
            effective_session_id: Some(session_id_raw.to_string()),
            session: Some(session),
            match_mode: "exact",
            detail: exact_fallback_detail,
            raw_len,
            trimmed_len,
        });
    }

    if !trimmed.is_empty() && trimmed != session_id_raw {
        let (session_trimmed, trimmed_fallback_detail) =
            get_session_lookup_session_by_id(state, trimmed).await?;
        if let Some(session) = session_trimmed {
            return Ok(SessionLookupResult {
                effective_session_id: Some(trimmed.to_string()),
                session: Some(session),
                match_mode: "trimmed",
                detail: combine_detail("exact_miss_trimmed_hit", trimmed_fallback_detail),
                raw_len,
                trimmed_len,
            });
        }
    }

    if !trimmed.is_empty() {
        let lower = trimmed.to_ascii_lowercase();
        if lower != trimmed {
            let (session_lower, lower_fallback_detail) =
                get_session_lookup_session_by_id(state, lower.as_str()).await?;
            if let Some(session) = session_lower {
                return Ok(SessionLookupResult {
                    effective_session_id: Some(lower),
                    session: Some(session),
                    match_mode: "lowercase",
                    detail: combine_detail("exact_miss_lowercase_hit", lower_fallback_detail),
                    raw_len,
                    trimmed_len,
                });
            }
        }

        let upper = trimmed.to_ascii_uppercase();
        if upper != trimmed {
            let (session_upper, upper_fallback_detail) =
                get_session_lookup_session_by_id(state, upper.as_str()).await?;
            if let Some(session) = session_upper {
                return Ok(SessionLookupResult {
                    effective_session_id: Some(upper),
                    session: Some(session),
                    match_mode: "uppercase",
                    detail: combine_detail("exact_miss_uppercase_hit", upper_fallback_detail),
                    raw_len,
                    trimmed_len,
                });
            }
        }
    }

    if looks_like_short_session_prefix(session_id_raw) {
        let prefix = trimmed;
        let candidates = find_session_ids_by_prefix(state, prefix, 3).await?;
        if candidates.len() == 1 {
            let candidate_id = candidates[0].clone();
            let (session_prefix, prefix_fallback_detail) =
                get_session_lookup_session_by_id(state, candidate_id.as_str()).await?;
            if let Some(session) = session_prefix {
                return Ok(SessionLookupResult {
                    effective_session_id: Some(candidate_id.clone()),
                    session: Some(session),
                    match_mode: "prefix_unique",
                    detail: combine_detail(
                        format!("exact_miss_unique_prefix_hit:{}", short_id(&candidate_id))
                            .as_str(),
                        prefix_fallback_detail,
                    ),
                    raw_len,
                    trimmed_len,
                });
            }
        } else if candidates.len() > 1 {
            return Ok(SessionLookupResult {
                effective_session_id: None,
                session: None,
                match_mode: "prefix_ambiguous",
                detail: Some(format!(
                    "exact_miss_prefix_ambiguous:{} matches",
                    candidates.len()
                )),
                raw_len,
                trimmed_len,
            });
        }
    }

    Ok(SessionLookupResult {
        session: None,
        match_mode: "not_found",
        detail: Some("exact_miss_no_variant_hit".to_string()),
        effective_session_id: None,
        raw_len,
        trimmed_len,
    })
}

pub(super) async fn list_job_runs(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<JobRunsQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_admin(&auth) {
        return err;
    }

    match memory_engine_client::list_engine_job_runs(
        &state.config,
        q.job_type.as_deref(),
        q.session_id.as_deref(),
        q.status.as_deref(),
        None,
        Some("memory_server"),
        q.limit.unwrap_or(100),
    )
    .await
    {
        Ok(items) => {
            let mut session_labels: HashMap<String, Value> = HashMap::new();
            let mut project_name_cache: HashMap<String, Option<String>> = HashMap::new();
            let mut agent_name_cache: HashMap<String, Option<String>> = HashMap::new();

            for item in &items {
                let Some(session_id) = engine_job_run_session_id(item) else {
                    continue;
                };
                if session_labels.contains_key(session_id.as_str()) {
                    continue;
                }

                let label_value = match resolve_session_for_job_run(&state, session_id.as_str()).await {
                    Ok(SessionLookupResult {
                        session: Some(session),
                        match_mode,
                        detail,
                        effective_session_id,
                        raw_len,
                        trimmed_len,
                    }) => {
                        let display_session_id =
                            effective_session_id.as_deref().unwrap_or(session_id.as_str());
                        let project_id = normalize_text(session.project_id.as_deref())
                            .unwrap_or_else(|| "0".to_string());
                        let project_cache_key = format!("{}|{}", session.user_id, project_id);
                        let project_name = if let Some(cached) =
                            project_name_cache.get(&project_cache_key)
                        {
                            cached.clone()
                        } else {
                            let resolved = match projects::get_project_by_user_and_project_id(
                                &state.pool,
                                session.user_id.as_str(),
                                project_id.as_str(),
                            )
                            .await
                            {
                                Ok(Some(project)) => normalize_text(Some(project.name.as_str())),
                                _ => None,
                            };
                            project_name_cache.insert(project_cache_key, resolved.clone());
                            resolved
                        };

                        let agent_id = agent_id_from_metadata(session.metadata.as_ref())
                            .unwrap_or_else(|| "-".to_string());
                        let agent_cache_key = format!("{}|{}", session.user_id, agent_id);
                        let agent_name =
                            if let Some(cached) = agent_name_cache.get(&agent_cache_key) {
                                cached.clone()
                            } else {
                                let resolved = if agent_id == "-" {
                                    None
                                } else {
                                    match contacts::get_contact_by_user_and_agent(
                                        &state.pool,
                                        session.user_id.as_str(),
                                        agent_id.as_str(),
                                    )
                                    .await
                                    {
                                        Ok(Some(contact)) => {
                                            normalize_text(contact.agent_name_snapshot.as_deref())
                                        }
                                        _ => None,
                                    }
                                };
                                agent_name_cache.insert(agent_cache_key, resolved.clone());
                                resolved
                            };

                        let project_label = match project_name {
                            Some(name) => format!("{} ({})", name, project_id),
                            None => {
                                if project_id == "0" {
                                    "未绑定项目(0)".to_string()
                                } else {
                                    project_id.clone()
                                }
                            }
                        };
                        let contact_label = contact_id_from_metadata(session.metadata.as_ref())
                            .unwrap_or_else(|| "-".to_string());
                        let agent_label = match agent_name {
                            Some(name) => format!("{} ({})", name, agent_id),
                            None => agent_id.clone(),
                        };
                        let session_display = format!(
                            "联系人: {} | 项目: {} | 智能体: {} | 会话: {}",
                            contact_label,
                            project_label,
                            agent_label,
                            short_id(display_session_id)
                        );
                        json!({
                            "session_contact_label": contact_label,
                            "session_project_label": project_label,
                            "session_agent_label": agent_label,
                            "session_display": session_display,
                            "session_resolve_status": "found",
                            "session_resolve_detail": detail,
                            "session_resolve_match_mode": match_mode,
                            "session_id_effective": effective_session_id,
                            "session_id_raw_len": raw_len as i64,
                            "session_id_trimmed_len": trimmed_len as i64,
                            "session_user_id": session.user_id,
                            "session_title": session.title,
                        })
                    }
                    Ok(SessionLookupResult {
                        session: None,
                        match_mode,
                        detail,
                        effective_session_id,
                        raw_len,
                        trimmed_len,
                    }) => json!({
                        "session_contact_label": Value::Null,
                        "session_project_label": Value::Null,
                        "session_agent_label": Value::Null,
                        "session_display": format!("会话不存在: {}", short_id(session_id.as_str())),
                        "session_resolve_status": "missing_session",
                        "session_resolve_detail": detail,
                        "session_resolve_match_mode": match_mode,
                        "session_id_effective": effective_session_id,
                        "session_id_raw_len": raw_len as i64,
                        "session_id_trimmed_len": trimmed_len as i64,
                        "session_user_id": Value::Null,
                        "session_title": Value::Null,
                    }),
                    Err(err) => json!({
                        "session_contact_label": Value::Null,
                        "session_project_label": Value::Null,
                        "session_agent_label": Value::Null,
                        "session_display": format!("会话查询失败: {}", short_id(session_id.as_str())),
                        "session_resolve_status": "lookup_error",
                        "session_resolve_detail": err,
                        "session_resolve_match_mode": "lookup_error",
                        "session_id_effective": Value::Null,
                        "session_id_raw_len": session_id.chars().count() as i64,
                        "session_id_trimmed_len": session_id.trim().chars().count() as i64,
                        "session_user_id": Value::Null,
                        "session_title": Value::Null,
                    }),
                };

                session_labels.insert(session_id, label_value);
            }

            let mut out = Vec::with_capacity(items.len());
            for item in items {
                let mut row = engine_job_run_to_compat_row(&item);
                if let Some(session_id) = row.get("session_id").and_then(|v| v.as_str()) {
                    if let Some(extra) = session_labels.get(session_id) {
                        if let Some(dst) = row.as_object_mut() {
                            if let Some(extra_obj) = extra.as_object() {
                                for (k, v) in extra_obj {
                                    dst.insert(k.clone(), v.clone());
                                }
                            }
                        }
                    }
                }
                out.push(row);
            }

            (StatusCode::OK, Json(json!({"items": out})))
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list job runs failed", "detail": err})),
        ),
    }
}

pub(super) async fn stream_job_runs(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<JobRunsStreamQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<Value>)> {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return Err(err),
    };
    if let Err(err) = ensure_admin(&auth) {
        return Err(err);
    }

    let initial_items = match memory_engine_client::list_engine_job_runs(
        &state.config,
        q.job_type.as_deref(),
        q.session_id.as_deref(),
        q.status.as_deref(),
        None,
        Some("memory_server"),
        q.limit.unwrap_or(500),
    )
    .await
    {
        Ok(items) => items
            .into_iter()
            .map(|item| engine_job_run_to_compat_row(&item))
            .collect::<Vec<_>>(),
        Err(err) => {
            return Err((
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "list job runs failed", "detail": err})),
            ));
        }
    };

    let initial_event = Event::default().event("snapshot").json_data(json!({
        "items": initial_items,
    })).unwrap_or_else(|_| Event::default().event("snapshot").data("{\"items\":[]}"));

    let filters = q;
    let event_stream = stream::unfold(
        (Some(initial_event), filters, state.config.clone()),
        |(initial, filters, config)| async move {
            if let Some(event) = initial {
                return Some((Ok(event), (None, filters, config)));
            }

            tokio::time::sleep(Duration::from_secs(5)).await;
            let items = match memory_engine_client::list_engine_job_runs(
                &config,
                filters.job_type.as_deref(),
                filters.session_id.as_deref(),
                filters.status.as_deref(),
                None,
                Some("memory_server"),
                filters.limit.unwrap_or(500),
            )
            .await
            {
                Ok(items) => items
                    .into_iter()
                    .map(|item| engine_job_run_to_compat_row(&item))
                    .collect::<Vec<_>>(),
                Err(_) => {
                    let resync = Event::default().event("resync").data("{}");
                    return Some((Ok(resync), (None, filters, config)));
                }
            };

            let event = Event::default()
                .event("snapshot")
                .json_data(json!({ "items": items }))
                .unwrap_or_else(|_| Event::default().event("snapshot").data("{\"items\":[]}"));
            Some((Ok(event), (None, filters, config)))
        },
    );

    Ok(Sse::new(event_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keepalive"),
    ))
}

pub(super) async fn job_stats(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_admin(&auth) {
        return err;
    }

    match memory_engine_client::get_engine_job_run_stats(
        &state.config,
        None,
        None,
        Some("memory_server"),
        24,
    )
    .await
    {
        Ok(stats) => (StatusCode::OK, Json(json!({"stats": stats}))),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": "job stats failed", "detail": err})),
        ),
    }
}
