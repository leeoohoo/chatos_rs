// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::core::auth::AuthUser;
use crate::core::mcp_config_access::{ensure_owned_mcp_config, map_mcp_config_access_error};
use crate::core::user_scope::resolve_user_id;
use crate::core::user_visible_path::display_path;
use crate::models::mcp_config::McpConfig;
use crate::repositories::mcp_configs as mcp_repo;
use crate::services::builtin_mcp::{
    builtin_display_name, is_builtin_mcp_id, list_builtin_mcp_configs,
};

use super::fs::policy::{FsPathPolicy, FsPolicyError};

mod ai_model;
mod mcp_resource;

#[derive(Debug, Deserialize)]
struct UserQuery {
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct McpConfigRequest {
    name: Option<String>,
    command: Option<String>,
    r#type: Option<String>,
    args: Option<Value>,
    env: Option<Value>,
    cwd: Option<String>,
    user_id: Option<String>,
    enabled: Option<bool>,
    app_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Clone)]
struct AiModelConfigRequest {
    id: Option<String>,
    name: Option<String>,
    provider: Option<String>,
    prompt_vendor: Option<String>,
    model: Option<String>,
    thinking_level: Option<String>,
    task_usage_scenario: Option<String>,
    task_thinking_level: Option<String>,
    temperature: Option<f64>,
    clear_temperature: Option<bool>,
    max_output_tokens: Option<i64>,
    clear_max_output_tokens: Option<bool>,
    api_key: Option<String>,
    clear_api_key: Option<bool>,
    base_url: Option<String>,
    enabled: Option<bool>,
    supports_images: Option<bool>,
    supports_reasoning: Option<bool>,
    supports_responses: Option<bool>,
}

#[derive(Debug, Deserialize, Clone)]
struct AiModelSettingsRequest {
    user_id: Option<String>,
    memory_summary_model_config_id: Option<Option<String>>,
    memory_summary_thinking_level: Option<Option<String>>,
    project_management_agent_model_config_id: Option<Option<String>>,
    project_management_agent_thinking_level: Option<Option<String>>,
    environment_initialization_model_config_id: Option<Option<String>>,
    environment_initialization_thinking_level: Option<Option<String>>,
}

#[derive(Debug, Deserialize)]
struct ResourceByCommandRequest {
    r#type: Option<String>,
    command: Option<String>,
    args: Option<Value>,
    env: Option<Value>,
    cwd: Option<String>,
    alias: Option<String>,
}

pub fn router() -> Router {
    Router::new()
        .route(
            "/api/mcp-configs",
            get(list_mcp_configs).post(create_mcp_config),
        )
        .route(
            "/api/mcp-configs/{config_id}",
            put(update_mcp_config).delete(delete_mcp_config),
        )
        .route(
            "/api/mcp-configs/{config_id}/resource/config",
            get(mcp_resource::get_mcp_resource_config),
        )
        .route(
            "/api/mcp-configs/resource/config",
            post(mcp_resource::post_mcp_resource_config),
        )
        .route(
            "/api/ai-model-configs",
            get(ai_model::list_ai_model_configs).post(ai_model::create_ai_model_config),
        )
        .route(
            "/api/ai-model-providers",
            get(ai_model::list_ai_model_providers).post(ai_model::create_ai_model_provider),
        )
        .route(
            "/api/ai-model-providers/{provider_id}",
            get(ai_model::get_ai_model_provider)
                .put(ai_model::update_ai_model_provider)
                .delete(ai_model::delete_ai_model_provider),
        )
        .route(
            "/api/ai-model-providers/{provider_id}/refresh",
            post(ai_model::refresh_ai_model_provider),
        )
        .route(
            "/api/ai-model-settings",
            get(ai_model::get_ai_model_settings).put(ai_model::put_ai_model_settings),
        )
        .route(
            "/api/ai-model-configs/{config_id}/models",
            get(ai_model::list_ai_provider_models),
        )
        .route(
            "/api/ai-model-configs/{config_id}",
            get(ai_model::get_ai_model_config)
                .put(ai_model::update_ai_model_config)
                .delete(ai_model::delete_ai_model_config),
        )
        .route(
            "/api/ai-model-configs/{config_id}/refresh",
            post(ai_model::refresh_ai_model_config),
        )
}

async fn list_mcp_configs(
    auth: AuthUser,
    Query(query): Query<UserQuery>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(query.user_id, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };
    let configs = match mcp_repo::list_mcp_configs(Some(user_id)).await {
        Ok(list) => list,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "获取MCP配置失败", "detail": err})),
            );
        }
    };
    let mut out = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();
    for cfg in configs {
        seen_ids.insert(cfg.id.clone());
        let app_ids = match mcp_repo::get_app_ids_for_mcp_config(&cfg.id).await {
            Ok(ids) => ids,
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "获取MCP配置失败", "detail": err})),
                );
            }
        };
        let display_cwd = cfg.cwd.as_ref().map(|cwd| display_path(cwd.as_str()));
        out.push(json!({
            "id": cfg.id,
            "name": cfg.name,
            "command": cfg.command,
            "type": cfg.r#type,
            "args": cfg.args,
            "env": cfg.env,
            "cwd": display_cwd.clone(),
            "display_cwd": display_cwd,
            "user_id": cfg.user_id,
            "enabled": cfg.enabled,
            "created_at": cfg.created_at,
            "updated_at": cfg.updated_at,
            "app_ids": app_ids
        }));
    }
    for cfg in list_builtin_mcp_configs() {
        if seen_ids.contains(&cfg.id) {
            continue;
        }
        let display_name = builtin_display_name(&cfg.id).unwrap_or(&cfg.name);
        let mut obj = mcp_config_value(&cfg);
        if let Some(map) = obj.as_object_mut() {
            map.insert("readonly".to_string(), json!(true));
            map.insert("builtin".to_string(), json!(true));
            map.insert("display_name".to_string(), json!(display_name));
            map.insert("app_ids".to_string(), json!([] as [String; 0]));
        }
        out.push(obj);
    }
    (StatusCode::OK, Json(Value::Array(out)))
}

fn fs_policy_error_tuple(err: FsPolicyError) -> (StatusCode, Json<Value>) {
    (
        err.status_code(),
        Json(serde_json::json!({ "error": err.message() })),
    )
}

pub(super) async fn authorize_optional_mcp_cwd(
    auth: &AuthUser,
    raw: Option<String>,
) -> Result<Option<String>, (StatusCode, Json<Value>)> {
    let Some(raw) = raw.map(|value| value.trim().to_string()) else {
        return Ok(None);
    };
    if raw.is_empty() {
        return Ok(None);
    }
    let policy = FsPathPolicy::for_user(auth)
        .await
        .map_err(fs_policy_error_tuple)?;
    let authorized = policy
        .authorize_existing_dir(
            raw.as_str(),
            "MCP 工作目录不存在或不是目录",
            "MCP 工作目录不存在或不是目录",
        )
        .map_err(fs_policy_error_tuple)?;
    policy
        .require_write(&authorized)
        .map_err(fs_policy_error_tuple)?;
    Ok(Some(authorized.path.to_string_lossy().to_string()))
}

pub(super) async fn default_mcp_cwd(
    auth: &AuthUser,
) -> Result<Option<String>, (StatusCode, Json<Value>)> {
    let policy = FsPathPolicy::for_user(auth)
        .await
        .map_err(fs_policy_error_tuple)?;
    Ok(policy
        .default_public_dir()
        .or_else(|| policy.default_workspace_dir())
        .map(|path| path.to_string_lossy().to_string()))
}

pub(super) async fn authorize_mcp_cwd_or_default(
    auth: &AuthUser,
    raw: Option<String>,
) -> Result<Option<String>, (StatusCode, Json<Value>)> {
    match authorize_optional_mcp_cwd(auth, raw).await? {
        Some(path) => Ok(Some(path)),
        None => default_mcp_cwd(auth).await,
    }
}

async fn create_mcp_config(
    auth: AuthUser,
    Json(req): Json<McpConfigRequest>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(req.user_id.clone(), &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };
    let Some(name) = req.name else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "创建MCP配置失败"})),
        );
    };
    let Some(command) = req.command else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "创建MCP配置失败"})),
        );
    };
    let mcp_type = req.r#type.unwrap_or_else(|| "stdio".to_string());
    let cwd = if mcp_type == "stdio" {
        match authorize_optional_mcp_cwd(&auth, req.cwd).await {
            Ok(path) => path,
            Err(err) => return err,
        }
    } else {
        None
    };
    let id = Uuid::new_v4().to_string();
    let cfg = McpConfig {
        id: id.clone(),
        name,
        command,
        r#type: mcp_type,
        args: req.args,
        env: req.env,
        cwd,
        user_id: Some(user_id),
        enabled: req.enabled.unwrap_or(true),
        created_at: crate::core::time::now_rfc3339(),
        updated_at: crate::core::time::now_rfc3339(),
    };
    if let Err(err) = mcp_repo::create_mcp_config(&cfg).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "创建MCP配置失败", "detail": err})),
        );
    }
    if let Some(app_ids) = req.app_ids.clone() {
        if let Err(err) = mcp_repo::set_app_ids_for_mcp_config(&id, &app_ids).await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "创建MCP配置失败", "detail": err})),
            );
        }
    }
    let saved_cfg = match mcp_repo::get_mcp_config_by_id(&id).await {
        Ok(Some(cfg)) => cfg,
        Ok(None) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "创建MCP配置失败"})),
            );
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "创建MCP配置失败", "detail": err})),
            );
        }
    };
    let app_ids = match mcp_repo::get_app_ids_for_mcp_config(&id).await {
        Ok(ids) => ids,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "创建MCP配置失败", "detail": err})),
            );
        }
    };
    let mut obj = mcp_config_value(&saved_cfg);
    if let Some(map) = obj.as_object_mut() {
        map.insert("app_ids".to_string(), json!(app_ids));
    }
    (StatusCode::CREATED, Json(obj))
}

async fn update_mcp_config(
    auth: AuthUser,
    Path(config_id): Path<String>,
    Json(req): Json<McpConfigRequest>,
) -> (StatusCode, Json<Value>) {
    if is_builtin_mcp_id(&config_id) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "内置 MCP 配置不可编辑"})),
        );
    }
    let mut cfg = match ensure_owned_mcp_config(&config_id, &auth).await {
        Ok(cfg) => cfg,
        Err(err) => return map_mcp_config_access_error(err),
    };
    let mut update_requested = false;
    if let Some(v) = req.name {
        cfg.name = v;
        update_requested = true;
    }
    if let Some(v) = req.command {
        cfg.command = v;
        update_requested = true;
    }
    if let Some(v) = req.r#type {
        cfg.r#type = v;
        update_requested = true;
    }
    if let Some(v) = req.args {
        cfg.args = Some(v);
        update_requested = true;
    }
    if let Some(v) = req.env {
        cfg.env = Some(v);
        update_requested = true;
    }
    if let Some(v) = req.cwd {
        match authorize_optional_mcp_cwd(&auth, Some(v)).await {
            Ok(path) => cfg.cwd = path,
            Err(err) => return err,
        }
        update_requested = true;
    }
    if cfg.r#type != "stdio" {
        cfg.cwd = None;
    }
    if let Some(v) = req.enabled {
        cfg.enabled = v;
        update_requested = true;
    }
    if update_requested {
        if let Err(err) = mcp_repo::update_mcp_config(&config_id, &cfg).await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "更新MCP配置失败", "detail": err})),
            );
        }
    }
    if let Some(app_ids) = req.app_ids {
        if let Err(err) = mcp_repo::set_app_ids_for_mcp_config(&config_id, &app_ids).await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "更新MCP配置失败", "detail": err})),
            );
        }
    }
    let cfg = match mcp_repo::get_mcp_config_by_id(&config_id).await {
        Ok(Some(cfg)) => cfg,
        Ok(None) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "更新MCP配置失败"})),
            );
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "更新MCP配置失败", "detail": err})),
            );
        }
    };
    let app_ids = match mcp_repo::get_app_ids_for_mcp_config(&config_id).await {
        Ok(ids) => ids,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "更新MCP配置失败", "detail": err})),
            );
        }
    };
    let mut obj = mcp_config_value(&cfg);
    if let Some(map) = obj.as_object_mut() {
        map.insert("app_ids".to_string(), json!(app_ids));
    }
    (StatusCode::OK, Json(obj))
}

async fn delete_mcp_config(
    auth: AuthUser,
    Path(config_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    if is_builtin_mcp_id(&config_id) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "内置 MCP 配置不可删除"})),
        );
    }
    if let Err(err) = ensure_owned_mcp_config(&config_id, &auth).await {
        return map_mcp_config_access_error(err);
    }
    if let Err(err) = mcp_repo::delete_mcp_config(&config_id).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "删除MCP配置失败", "detail": err})),
        );
    }
    (
        StatusCode::OK,
        Json(json!({ "message": "MCP配置删除成功", "id": config_id })),
    )
}

fn mcp_config_value(cfg: &McpConfig) -> Value {
    json!({
        "id": cfg.id.clone(),
        "name": cfg.name.clone(),
        "command": cfg.command.clone(),
        "type": cfg.r#type.clone(),
        "args": cfg.args.clone(),
        "env": cfg.env.clone(),
        "cwd": cfg.cwd.as_ref().map(|cwd| display_path(cwd.as_str())),
        "display_cwd": cfg.cwd.as_ref().map(|cwd| display_path(cwd.as_str())),
        "user_id": cfg.user_id.clone(),
        "enabled": cfg.enabled,
        "created_at": cfg.created_at.clone(),
        "updated_at": cfg.updated_at.clone()
    })
}
