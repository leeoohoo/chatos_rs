use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::repositories::skills as skills_repo;

use super::{require_auth, resolve_scope_user_id, resolve_visible_user_ids, SharedState};

#[derive(Debug, Deserialize)]
pub(super) struct ListSkillsQuery {
    user_id: Option<String>,
    plugin_source: Option<String>,
    query: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ListSkillPluginsQuery {
    user_id: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

pub(super) async fn list_skills(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<ListSkillsQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let scope_user_id = resolve_scope_user_id(&auth, q.user_id);
    let visible_user_ids = resolve_visible_user_ids(scope_user_id.as_str());
    let limit = q.limit.unwrap_or(200);
    let offset = q.offset.unwrap_or(0);
    let plugin_source = q
        .plugin_source
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let query = q
        .query
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    match skills_repo::list_skills(
        &state.pool,
        visible_user_ids.as_slice(),
        plugin_source,
        query,
        limit,
        offset,
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list skills failed", "detail": err})),
        ),
    }
}

pub(super) async fn list_skill_plugins(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<ListSkillPluginsQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let scope_user_id = resolve_scope_user_id(&auth, q.user_id);
    let limit = q.limit.unwrap_or(200);
    let offset = q.offset.unwrap_or(0);

    match skills_repo::list_plugins(&state.pool, scope_user_id.as_str(), limit, offset).await {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list skill plugins failed", "detail": err})),
        ),
    }
}
