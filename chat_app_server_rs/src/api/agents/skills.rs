// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::services::chatos_skills;

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
pub(super) struct ImportSkillsFromGitRequest {
    user_id: Option<String>,
    repository: String,
    branch: Option<String>,
    marketplace_path: Option<String>,
    plugins_path: Option<String>,
    auto_install: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct InstallSkillPluginsRequest {
    user_id: Option<String>,
    source: Option<String>,
    install_all: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct SkillDetailQuery {
    user_id: Option<String>,
    source: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ScopedUserQuery {
    user_id: Option<String>,
}

pub(super) async fn get_skill(
    auth: AuthUser,
    Path(skill_id): Path<String>,
    Query(query): Query<ScopedUserQuery>,
) -> (StatusCode, Json<Value>) {
    let user_id = match super::resolve_scope_user_id(query.user_id, &auth) {
        Ok(value) => value,
        Err(err) => return err,
    };
    match chatos_skills::get_skill(user_id.as_str(), skill_id.as_str()).await {
        Ok(Some(skill)) => (StatusCode::OK, Json(json!(skill))),
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

pub(super) async fn list_skills(
    auth: AuthUser,
    Query(query): Query<ListSkillsQuery>,
) -> (StatusCode, Json<Value>) {
    let user_id = match super::resolve_scope_user_id(query.user_id, &auth) {
        Ok(value) => value,
        Err(err) => return err,
    };

    match chatos_skills::list_skills(
        user_id.as_str(),
        query.plugin_source.as_deref(),
        query.query.as_deref(),
        query.limit,
        query.offset.unwrap_or(0),
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!(items))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list skills failed", "detail": err})),
        ),
    }
}

pub(super) async fn list_skill_plugins(
    auth: AuthUser,
    Query(query): Query<ListSkillPluginsQuery>,
) -> (StatusCode, Json<Value>) {
    let user_id = match super::resolve_scope_user_id(query.user_id, &auth) {
        Ok(value) => value,
        Err(err) => return err,
    };

    match chatos_skills::list_skill_plugins(
        user_id.as_str(),
        query.limit,
        query.offset.unwrap_or(0),
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!(items))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list skill plugins failed", "detail": err})),
        ),
    }
}

pub(super) async fn get_skill_plugin(
    auth: AuthUser,
    Query(query): Query<SkillDetailQuery>,
) -> (StatusCode, Json<Value>) {
    let source = query.source.unwrap_or_default();
    if source.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "source is required"})),
        );
    }

    let user_id = match super::resolve_scope_user_id(query.user_id, &auth) {
        Ok(value) => value,
        Err(err) => return err,
    };
    match chatos_skills::get_skill_plugin(user_id.as_str(), source.as_str()).await {
        Ok(Some(plugin)) => (StatusCode::OK, Json(json!(plugin))),
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

pub(super) async fn import_skills_from_git(
    auth: AuthUser,
    Json(req): Json<ImportSkillsFromGitRequest>,
) -> (StatusCode, Json<Value>) {
    let user_id = match super::resolve_scope_user_id(req.user_id.clone(), &auth) {
        Ok(value) => value,
        Err(err) => return err,
    };
    let repository = req.repository.trim().to_string();
    if repository.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "repository is required"})),
        );
    }
    let branch = req
        .branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    let outcome = match chatos_skills::import_skills_from_git(
        user_id.as_str(),
        repository.clone(),
        branch.clone(),
        req.marketplace_path,
        req.plugins_path,
    )
    .await
    {
        Ok(value) => value,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "import skills from git failed", "detail": err})),
            );
        }
    };

    if outcome.imported_sources.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "no plugin imported", "details": outcome.details})),
        );
    }

    let auto_install = req.auto_install.unwrap_or(false);
    let install_result = if auto_install {
        match chatos_skills::install_skill_plugins(
            user_id.as_str(),
            outcome.imported_sources.as_slice(),
        )
        .await
        {
            Ok(value) => Some(value),
            Err(err) => Some(json!({"ok": false, "error": err})),
        }
    } else {
        None
    };

    (
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "repository": outcome.repository,
            "branch": outcome.branch,
            "imported_sources": outcome.imported_sources,
            "details": outcome.details,
            "auto_install": auto_install,
            "install_result": install_result
        })),
    )
}

pub(super) async fn install_skill_plugins(
    auth: AuthUser,
    Json(req): Json<InstallSkillPluginsRequest>,
) -> (StatusCode, Json<Value>) {
    let user_id = match super::resolve_scope_user_id(req.user_id.clone(), &auth) {
        Ok(value) => value,
        Err(err) => return err,
    };
    let install_all = req.install_all.unwrap_or(false);
    let source = req
        .source
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    let target_sources = if install_all {
        match chatos_skills::list_all_plugin_sources(user_id.as_str()).await {
            Ok(items) => items,
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "load plugins failed", "detail": err})),
                );
            }
        }
    } else if let Some(value) = source {
        vec![value]
    } else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "source is required when install_all=false"})),
        );
    };

    match chatos_skills::install_skill_plugins(user_id.as_str(), &target_sources).await {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "install plugins failed", "detail": err})),
        ),
    }
}
