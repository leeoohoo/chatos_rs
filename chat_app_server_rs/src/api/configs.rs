use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::models::mcp_config::McpConfig;
use crate::repositories::mcp_configs as mcp_repo;
use crate::services::builtin_mcp::{
    builtin_display_name, is_builtin_mcp_id, list_builtin_mcp_configs, SUB_AGENT_ROUTER_MCP_ID,
};

mod ai_model;
mod builtin_settings;
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

#[derive(Debug, Deserialize)]
struct AiModelConfigRequest {
    name: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    thinking_level: Option<String>,
    api_key: Option<String>,
    base_url: Option<String>,
    user_id: Option<String>,
    enabled: Option<bool>,
    supports_images: Option<bool>,
    supports_reasoning: Option<bool>,
    supports_responses: Option<bool>,
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

#[derive(Debug, Deserialize)]
struct BuiltinImportRequest {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BuiltinGitImportRequest {
    repository: Option<String>,
    branch: Option<String>,
    agents_path: Option<String>,
    skills_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BuiltinPluginInstallRequest {
    source: Option<String>,
    install_all: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct BuiltinMcpPermissionsRequest {
    enabled_mcp_ids: Option<Vec<String>>,
    selected_system_context_id: Option<String>,
}

pub fn router() -> Router {
    Router::new()
        .route(
            "/api/mcp-configs",
            get(list_mcp_configs).post(create_mcp_config),
        )
        .route(
            "/api/mcp-configs/:config_id",
            put(update_mcp_config).delete(delete_mcp_config),
        )
        .route(
            "/api/mcp-configs/:config_id/resource/config",
            get(mcp_resource::get_mcp_resource_config),
        )
        .route(
            "/api/mcp-configs/:config_id/builtin/settings",
            get(builtin_settings::get_builtin_mcp_settings),
        )
        .route(
            "/api/mcp-configs/:config_id/builtin/mcp-permissions",
            get(builtin_settings::get_builtin_mcp_permissions)
                .post(builtin_settings::update_builtin_mcp_permissions),
        )
        .route(
            "/api/mcp-configs/:config_id/builtin/import-agents",
            post(builtin_settings::import_builtin_agents),
        )
        .route(
            "/api/mcp-configs/:config_id/builtin/import-skills",
            post(builtin_settings::import_builtin_skills),
        )
        .route(
            "/api/mcp-configs/:config_id/builtin/import-git",
            post(builtin_settings::import_builtin_from_git),
        )
        .route(
            "/api/mcp-configs/:config_id/builtin/install-plugin",
            post(builtin_settings::install_builtin_plugin),
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
            "/api/ai-model-configs/:config_id",
            put(ai_model::update_ai_model_config).delete(ai_model::delete_ai_model_config),
        )
}

async fn list_mcp_configs(Query(query): Query<UserQuery>) -> (StatusCode, Json<Value>) {
    let configs = match mcp_repo::list_mcp_configs(query.user_id).await {
        Ok(list) => list,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "获取MCP配置失败", "detail": err})),
            )
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
                )
            }
        };
        out.push(json!({
            "id": cfg.id,
            "name": cfg.name,
            "command": cfg.command,
            "type": cfg.r#type,
            "args": cfg.args,
            "env": cfg.env,
            "cwd": cfg.cwd,
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
            if cfg.id == SUB_AGENT_ROUTER_MCP_ID {
                map.insert("supports_settings".to_string(), json!(true));
                map.insert("builtin_kind".to_string(), json!("sub_agent_router"));
            }
        }
        out.push(obj);
    }
    (StatusCode::OK, Json(Value::Array(out)))
}

async fn create_mcp_config(Json(req): Json<McpConfigRequest>) -> (StatusCode, Json<Value>) {
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
    let id = Uuid::new_v4().to_string();
    let cfg = McpConfig {
        id: id.clone(),
        name,
        command,
        r#type: req.r#type.unwrap_or_else(|| "stdio".to_string()),
        args: req.args,
        env: req.env,
        cwd: req.cwd,
        user_id: req.user_id,
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
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "创建MCP配置失败", "detail": err})),
            )
        }
    };
    let app_ids = match mcp_repo::get_app_ids_for_mcp_config(&id).await {
        Ok(ids) => ids,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "创建MCP配置失败", "detail": err})),
            )
        }
    };
    let mut obj = mcp_config_value(&saved_cfg);
    if let Some(map) = obj.as_object_mut() {
        map.insert("app_ids".to_string(), json!(app_ids));
    }
    (StatusCode::CREATED, Json(obj))
}

async fn update_mcp_config(
    Path(config_id): Path<String>,
    Json(req): Json<McpConfigRequest>,
) -> (StatusCode, Json<Value>) {
    if is_builtin_mcp_id(&config_id) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "内置 MCP 配置不可编辑"})),
        );
    }
    let existing = match mcp_repo::get_mcp_config_by_id(&config_id).await {
        Ok(cfg) => cfg,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "更新MCP配置失败", "detail": err})),
            )
        }
    };
    if existing.is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "MCP配置不存在"})),
        );
    }
    let mut cfg = existing.unwrap();
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
        cfg.cwd = Some(v);
        update_requested = true;
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
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "更新MCP配置失败", "detail": err})),
            )
        }
    };
    let app_ids = match mcp_repo::get_app_ids_for_mcp_config(&config_id).await {
        Ok(ids) => ids,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "更新MCP配置失败", "detail": err})),
            )
        }
    };
    let mut obj = mcp_config_value(&cfg);
    if let Some(map) = obj.as_object_mut() {
        map.insert("app_ids".to_string(), json!(app_ids));
    }
    (StatusCode::OK, Json(obj))
}

async fn delete_mcp_config(Path(config_id): Path<String>) -> (StatusCode, Json<Value>) {
    if is_builtin_mcp_id(&config_id) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "内置 MCP 配置不可删除"})),
        );
    }
    let existing = match mcp_repo::get_mcp_config_by_id(&config_id).await {
        Ok(cfg) => cfg,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "删除MCP配置失败", "detail": err})),
            )
        }
    };
    if existing.is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "MCP配置不存在"})),
        );
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
        "cwd": cfg.cwd.clone(),
        "user_id": cfg.user_id.clone(),
        "enabled": cfg.enabled,
        "created_at": cfg.created_at.clone(),
        "updated_at": cfg.updated_at.clone()
    })
}
