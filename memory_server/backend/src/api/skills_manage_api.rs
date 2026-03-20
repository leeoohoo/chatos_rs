use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::services::skills::{
    import_skills_from_git as import_skills_from_git_service,
    install_skill_plugins as install_skill_plugins_service, list_all_plugin_sources,
    normalize_plugin_source,
};

use super::{require_auth, resolve_scope_user_id, SharedState};

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

pub(super) async fn import_skills_from_git(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<ImportSkillsFromGitRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let repository = req.repository.trim().to_string();
    if repository.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "repository is required"})),
        );
    }

    let scope_user_id = resolve_scope_user_id(&auth, req.user_id);
    let branch = req
        .branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    let outcome = match import_skills_from_git_service(
        state.as_ref(),
        scope_user_id.as_str(),
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
            )
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
        match install_skill_plugins_service(
            state.as_ref(),
            scope_user_id.as_str(),
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
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<InstallSkillPluginsRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let scope_user_id = resolve_scope_user_id(&auth, req.user_id);
    let install_all = req.install_all.unwrap_or(false);
    let source = req
        .source
        .as_deref()
        .map(normalize_plugin_source)
        .filter(|value| !value.is_empty());

    let target_sources = if install_all {
        match list_all_plugin_sources(state.as_ref(), scope_user_id.as_str()).await {
            Ok(items) => items,
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "load plugins failed", "detail": err})),
                )
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

    match install_skill_plugins_service(state.as_ref(), scope_user_id.as_str(), &target_sources).await
    {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "install plugins failed", "detail": err})),
        ),
    }
}
