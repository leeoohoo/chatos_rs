use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::repositories::skills as skills_repo;
use crate::services::skills::{
    extract_plugin_content_async, normalize_plugin_source, resolve_plugin_root_from_cache,
    resolve_skill_state_root,
};

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

#[derive(Debug, Deserialize)]
pub(super) struct GetSkillPluginQuery {
    user_id: Option<String>,
    source: Option<String>,
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

pub(super) async fn get_skill(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(skill_id): Path<String>,
    Query(q): Query<ListSkillsQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let scope_user_id = resolve_scope_user_id(&auth, q.user_id);
    let visible_user_ids = resolve_visible_user_ids(scope_user_id.as_str());
    let skill_id = skill_id.trim();
    if skill_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "skill_id is required"})),
        );
    }

    match skills_repo::get_skill_by_id(&state.pool, visible_user_ids.as_slice(), skill_id).await {
        Ok(Some(item)) => (StatusCode::OK, Json(json!(item))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "skill not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "get skill failed", "detail": err})),
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
    let visible_user_ids = resolve_visible_user_ids(scope_user_id.as_str());
    let limit = q.limit.unwrap_or(200);
    let offset = q.offset.unwrap_or(0);

    match skills_repo::list_plugins_by_user_ids(
        &state.pool,
        visible_user_ids.as_slice(),
        limit,
        offset,
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list skill plugins failed", "detail": err})),
        ),
    }
}

pub(super) async fn get_skill_plugin(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<GetSkillPluginQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let scope_user_id = resolve_scope_user_id(&auth, q.user_id);
    let visible_user_ids = resolve_visible_user_ids(scope_user_id.as_str());
    let source = q
        .source
        .as_deref()
        .map(normalize_plugin_source)
        .filter(|item| !item.is_empty());
    let Some(source) = source else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "source is required"})),
        );
    };

    match skills_repo::get_plugin_by_source_for_user_ids(
        &state.pool,
        visible_user_ids.as_slice(),
        source.as_str(),
    )
    .await
    {
        Ok(Some(mut item)) => {
            let content_missing = item
                .content
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none();
            let commands_missing = item.commands.is_empty();
            if content_missing || commands_missing {
                let state_root = resolve_skill_state_root(item.user_id.as_str());
                let plugins_root = state_root.join("plugins");
                if let Some(plugin_root) = resolve_plugin_root_from_cache(
                    plugins_root.as_path(),
                    item.cache_path.as_deref(),
                    item.source.as_str(),
                ) {
                    if let Ok(extracted) = extract_plugin_content_async(plugin_root).await {
                        let mut changed = false;
                        if content_missing {
                            if let Some(content) = extracted
                                .content
                                .as_deref()
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                            {
                                item.content = Some(content.to_string());
                                changed = true;
                            }
                        }
                        if commands_missing && !extracted.commands.is_empty() {
                            item.commands = extracted.commands;
                            item.command_count = item.commands.len().min(i64::MAX as usize) as i64;
                            changed = true;
                        }
                        if changed {
                            if let Ok(saved) =
                                skills_repo::upsert_plugin(&state.pool, item.clone()).await
                            {
                                item = saved;
                            }
                        }
                    }
                }
            }
            (StatusCode::OK, Json(json!(item)))
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "plugin not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "get skill plugin failed", "detail": err})),
        ),
    }
}
