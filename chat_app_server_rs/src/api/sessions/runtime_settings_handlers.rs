// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::{extract::Path, http::StatusCode, Json};
use serde_json::Value;

use crate::api::fs::policy::{FsPathPolicy, FsPolicyError};
use crate::core::auth::AuthUser;
use crate::core::session_access::{ensure_owned_session, map_session_access_error};
use crate::models::session::Session;
use crate::models::session_runtime_settings::SessionRuntimeSettings;
use crate::repositories::session_runtime_settings;

use super::contracts::UpdateSessionRuntimeSettingsRequest;

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

fn normalize_id_list(values: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() || out.iter().any(|item: &String| item == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
}

fn fs_policy_error_tuple(err: FsPolicyError) -> (StatusCode, Json<Value>) {
    (
        err.status_code(),
        Json(serde_json::json!({ "error": err.message() })),
    )
}

async fn authorize_optional_workspace_root(
    auth: &AuthUser,
    raw: Option<String>,
) -> Result<Option<String>, (StatusCode, Json<Value>)> {
    let Some(raw) = normalize_optional_text(raw) else {
        return Ok(None);
    };
    let policy = FsPathPolicy::for_user(auth)
        .await
        .map_err(fs_policy_error_tuple)?;
    let authorized = policy
        .authorize_existing_dir(
            raw.as_str(),
            "工作空间目录不存在或不是目录",
            "工作空间目录不存在或不是目录",
        )
        .map_err(fs_policy_error_tuple)?;
    policy
        .require_write(&authorized)
        .map_err(fs_policy_error_tuple)?;
    Ok(Some(authorized.path.to_string_lossy().to_string()))
}

async fn load_or_default(
    session: &Session,
    auth: &AuthUser,
) -> Result<SessionRuntimeSettings, String> {
    let existing = session_runtime_settings::get_session_runtime_settings(
        session.id.as_str(),
        auth.user_id.as_str(),
    )
    .await?;
    Ok(existing
        .unwrap_or_else(|| SessionRuntimeSettings::new(session.id.clone(), auth.user_id.clone())))
}

pub(super) async fn get_session_runtime_settings(
    auth: AuthUser,
    Path(conversation_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let session = match ensure_owned_session(&conversation_id, &auth).await {
        Ok(session) => session,
        Err(err) => return map_session_access_error(err),
    };

    match load_or_default(&session, &auth).await {
        Ok(settings) => (
            StatusCode::OK,
            Json(serde_json::to_value(settings).unwrap_or(Value::Null)),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}

pub(super) async fn update_session_runtime_settings(
    auth: AuthUser,
    Path(conversation_id): Path<String>,
    Json(req): Json<UpdateSessionRuntimeSettingsRequest>,
) -> (StatusCode, Json<Value>) {
    let session = match ensure_owned_session(&conversation_id, &auth).await {
        Ok(session) => session,
        Err(err) => return map_session_access_error(err),
    };

    let mut next = match load_or_default(&session, &auth).await {
        Ok(settings) => settings,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": err})),
            );
        }
    };
    next.session_id = conversation_id;
    next.user_id = auth.user_id.clone();

    if let Some(value) = req.selected_model_id {
        next.selected_model_id = normalize_optional_text(value);
    }
    if let Some(value) = req.selected_model_name {
        next.selected_model_name = normalize_optional_text(value);
    }
    if let Some(value) = req.selected_thinking_level {
        next.selected_thinking_level = normalize_optional_text(value);
    }
    if let Some(value) = req.remote_connection_id {
        next.remote_connection_id = normalize_optional_text(value);
    }
    if let Some(value) = req.workspace_root {
        next.workspace_root = match authorize_optional_workspace_root(&auth, value).await {
            Ok(path) => path,
            Err(err) => return err,
        };
    }
    if let Some(value) = req.reasoning_enabled {
        next.reasoning_enabled = value;
    }
    if let Some(value) = req.plan_mode_enabled {
        next.plan_mode_enabled = value;
    }
    if let Some(value) = req.mcp_enabled {
        next.mcp_enabled = value;
    }
    if let Some(value) = req.enabled_mcp_ids {
        next.enabled_mcp_ids = normalize_id_list(value);
    }
    if let Some(value) = req.auto_create_task {
        next.auto_create_task = value;
    }

    match session_runtime_settings::upsert_session_runtime_settings(&next).await {
        Ok(saved) => (
            StatusCode::OK,
            Json(serde_json::to_value(saved).unwrap_or(Value::Null)),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}
