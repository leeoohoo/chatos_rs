use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

use crate::services::memory_engine_client;

use super::{ensure_session_access, require_auth, SharedState};

#[derive(Debug, Deserialize)]
pub(super) struct ListSummariesQuery {
    level: Option<i64>,
    status: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

pub(super) async fn list_summaries(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Query(q): Query<ListSummariesQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_session_access(state.as_ref(), &auth, session_id.as_str()).await {
        return err;
    }

    let status_filter = q.status.as_deref().and_then(|status| {
        if status.eq_ignore_ascii_case("all") {
            None
        } else {
            Some(status)
        }
    });

    match memory_engine_client::list_summaries(
        &state.config,
        &state.pool,
        session_id.as_str(),
        q.level,
        status_filter,
        q.limit.unwrap_or(100),
        q.offset.unwrap_or(0),
    )
    .await {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": "list summaries failed", "detail": err})),
        ),
    }
}

pub(super) async fn summary_levels(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_session_access(state.as_ref(), &auth, session_id.as_str()).await {
        return err;
    }

    match memory_engine_client::list_all_summaries_by_session(
        &state.config,
        &state.pool,
        session_id.as_str(),
    )
    .await {
        Ok(items) => {
            let mut levels = BTreeMap::<i64, (i64, i64)>::new();
            for item in items {
                let entry = levels.entry(item.level).or_insert((0, 0));
                entry.0 += 1;
                if item.status == "pending" {
                    entry.1 += 1;
                }
            }

            let payload: Vec<Value> = levels
                .into_iter()
                .map(|(level, (total, pending))| {
                    json!({
                        "level": level,
                        "total": total,
                        "pending": pending,
                        "summarized": total.saturating_sub(pending),
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({"items": payload})))
        }
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": "list summary levels failed", "detail": err})),
        ),
    }
}

pub(super) async fn summary_graph(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_session_access(state.as_ref(), &auth, session_id.as_str()).await {
        return err;
    }

    match memory_engine_client::list_all_summaries_by_session(
        &state.config,
        &state.pool,
        session_id.as_str(),
    )
    .await {
        Ok(items) => {
            let nodes: Vec<Value> = items
                .iter()
                .map(|s| {
                    let excerpt: String = s.summary_text.chars().take(120).collect();
                    json!({
                        "id": s.id,
                        "level": s.level,
                        "status": s.status,
                        "rollup_summary_id": s.rollup_summary_id,
                        "created_at": s.created_at,
                        "summary_excerpt": excerpt,
                    })
                })
                .collect();

            let edges: Vec<Value> = items
                .iter()
                .filter_map(|s| {
                    s.rollup_summary_id.as_ref().map(|target| {
                        json!({
                            "from": s.id,
                            "to": target,
                        })
                    })
                })
                .collect();

            (
                StatusCode::OK,
                Json(json!({
                    "session_id": session_id,
                    "nodes": nodes,
                    "edges": edges
                })),
            )
        }
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": "summary graph failed", "detail": err})),
        ),
    }
}

pub(super) async fn delete_summary(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((session_id, summary_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_session_access(state.as_ref(), &auth, session_id.as_str()).await {
        return err;
    }

    match memory_engine_client::delete_summary(
        &state.config,
        &state.pool,
        session_id.as_str(),
        summary_id.as_str(),
    )
    .await {
        Ok(reset_messages) if reset_messages > 0 => (
            StatusCode::OK,
            Json(json!({"success": true, "reset_messages": reset_messages})),
        ),
        Ok(_) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "summary not found"})),
        ),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": "delete summary failed", "detail": err})),
        ),
    }
}
