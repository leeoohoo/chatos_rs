use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::models::ai_model_config::AiModelConfig;
use crate::models::mcp_config::McpConfig;
use crate::models::system_context::SystemContext;
use crate::repositories::ai_model_configs as ai_repo;
use crate::repositories::mcp_configs as mcp_repo;
use crate::repositories::system_contexts as ctx_repo;
use crate::services::builtin_mcp::{
    builtin_display_name, get_builtin_mcp_config, is_builtin_mcp_id, list_builtin_mcp_configs,
};

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
struct SystemContextRequest {
    name: Option<String>,
    content: Option<String>,
    user_id: Option<String>,
    is_active: Option<bool>,
    app_ids: Option<Vec<String>>,
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
            "/api/mcp-configs/:config_id",
            put(update_mcp_config).delete(delete_mcp_config),
        )
        .route(
            "/api/mcp-configs/:config_id/resource/config",
            get(get_mcp_resource_config),
        )
        .route(
            "/api/mcp-configs/resource/config",
            post(post_mcp_resource_config),
        )
        .route(
            "/api/ai-model-configs",
            get(list_ai_model_configs).post(create_ai_model_config),
        )
        .route(
            "/api/ai-model-configs/:config_id",
            put(update_ai_model_config).delete(delete_ai_model_config),
        )
        .route(
            "/api/system-contexts",
            get(list_system_contexts).post(create_system_context),
        )
        .route(
            "/api/system-contexts/:context_id",
            put(update_system_context).delete(delete_system_context),
        )
        .route(
            "/api/system-contexts/:context_id/activate",
            post(activate_system_context),
        )
        .route("/api/system-context/active", get(get_active_system_context))
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
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
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

async fn get_mcp_resource_config(Path(config_id): Path<String>) -> (StatusCode, Json<Value>) {
    if is_builtin_mcp_id(&config_id) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "内置 MCP 不支持资源配置读取"})),
        );
    }
    let cfg = if is_builtin_mcp_id(&config_id) {
        get_builtin_mcp_config(&config_id)
    } else {
        mcp_repo::get_mcp_config_by_id(&config_id)
            .await
            .unwrap_or(None)
    };
    let cfg = match cfg {
        Some(c) => c,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "MCP配置不存在"})),
            )
        }
    };
    if cfg.r#type != "stdio" {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "仅支持stdio类型的MCP配置读取资源"})),
        );
    }
    if cfg.command.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "MCP配置缺少可执行命令"})),
        );
    }
    let args = parse_args(&cfg.args);
    let env = parse_env(&cfg.env);
    let cwd = cfg.cwd.clone();
    match read_mcp_resource_config(&cfg.command, &args, &env, cwd.as_deref()).await {
        Ok(text) => {
            let data =
                serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({ "raw": text }));
            (
                StatusCode::OK,
                Json(json!({ "success": true, "config": data, "alias": cfg.name })),
            )
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("读取MCP配置资源失败: {}", err) })),
        ),
    }
}

async fn post_mcp_resource_config(
    Json(req): Json<ResourceByCommandRequest>,
) -> (StatusCode, Json<Value>) {
    if req.r#type.as_deref() != Some("stdio") {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "仅支持stdio类型的MCP配置读取资源"})),
        );
    }
    let command = match req.command {
        Some(c) => c,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "缺少可执行命令"})),
            )
        }
    };
    let args = parse_args(&req.args);
    let env = parse_env(&req.env);
    let alias = req.alias.unwrap_or_else(|| "mcp_server".to_string());
    match read_mcp_resource_config(&command, &args, &env, req.cwd.as_deref()).await {
        Ok(text) => {
            let data =
                serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({ "raw": text }));
            (
                StatusCode::OK,
                Json(json!({ "success": true, "config": data, "alias": alias })),
            )
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("读取MCP配置资源失败: {}", err) })),
        ),
    }
}

async fn list_ai_model_configs(Query(query): Query<UserQuery>) -> (StatusCode, Json<Value>) {
    let configs = match ai_repo::list_ai_model_configs(query.user_id).await {
        Ok(list) => list,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "获取AI模型配置失败", "detail": err})),
            )
        }
    };
    let out = configs
        .into_iter()
        .map(|cfg| {
            json!({
                "id": cfg.id,
                "name": cfg.name,
                "provider": cfg.provider,
                "model": cfg.model,
                "thinking_level": cfg.thinking_level,
                "api_key": cfg.api_key,
                "base_url": cfg.base_url,
                "user_id": cfg.user_id,
                "enabled": cfg.enabled,
                "supports_images": cfg.supports_images,
                "supports_reasoning": cfg.supports_reasoning,
                "supports_responses": cfg.supports_responses,
                "created_at": cfg.created_at,
                "updated_at": cfg.updated_at
            })
        })
        .collect();
    (StatusCode::OK, Json(Value::Array(out)))
}

async fn create_ai_model_config(
    Json(req): Json<AiModelConfigRequest>,
) -> (StatusCode, Json<Value>) {
    let Some(name) = req.name else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "创建AI模型配置失败"})),
        );
    };
    let Some(model) = req.model else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "创建AI模型配置失败"})),
        );
    };
    let id = Uuid::new_v4().to_string();
    let provider = match normalize_provider_input(req.provider.clone()) {
        Ok(p) => p,
        Err(err) => return (StatusCode::BAD_REQUEST, Json(json!({"error": err}))),
    };
    let thinking_level = match normalize_thinking_level_input(&provider, req.thinking_level.clone())
    {
        Ok(v) => v,
        Err(err) => return (StatusCode::BAD_REQUEST, Json(json!({"error": err}))),
    };
    let cfg = AiModelConfig {
        id: id.clone(),
        name,
        provider,
        model,
        thinking_level,
        api_key: req.api_key,
        base_url: req.base_url,
        user_id: req.user_id,
        enabled: req.enabled.unwrap_or(true),
        supports_images: req.supports_images.unwrap_or(false),
        supports_reasoning: req.supports_reasoning.unwrap_or(false),
        supports_responses: req.supports_responses.unwrap_or(false),
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    if let Err(err) = ai_repo::create_ai_model_config(&cfg).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "创建AI模型配置失败", "detail": err})),
        );
    }
    (StatusCode::CREATED, Json(ai_model_config_value(&cfg)))
}

async fn update_ai_model_config(
    Path(config_id): Path<String>,
    Json(req): Json<AiModelConfigRequest>,
) -> (StatusCode, Json<Value>) {
    let existing = match ai_repo::get_ai_model_config_by_id(&config_id).await {
        Ok(cfg) => cfg,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "更新AI模型配置失败", "detail": err})),
            )
        }
    };
    if existing.is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "AI模型配置不存在"})),
        );
    }
    let mut cfg = existing.unwrap();
    if let Some(v) = req.name {
        cfg.name = v;
    }
    let provider = if let Some(v) = req.provider {
        match normalize_provider_input(Some(v)) {
            Ok(p) => p,
            Err(err) => return (StatusCode::BAD_REQUEST, Json(json!({"error": err}))),
        }
    } else {
        normalize_provider_input(Some(cfg.provider.clone())).unwrap_or_else(|_| "gpt".to_string())
    };
    cfg.provider = provider.clone();
    if let Some(v) = req.model {
        cfg.model = v;
    }
    if provider != "gpt" {
        cfg.thinking_level = None;
    } else if let Some(v) = req.thinking_level {
        cfg.thinking_level = Some(v);
    }
    if let Some(v) = req.api_key {
        cfg.api_key = Some(v);
    }
    if let Some(v) = req.base_url {
        cfg.base_url = Some(v);
    }
    if let Some(v) = req.enabled {
        cfg.enabled = v;
    }
    if let Some(v) = req.supports_images {
        cfg.supports_images = v;
    }
    if let Some(v) = req.supports_reasoning {
        cfg.supports_reasoning = v;
    }
    if let Some(v) = req.supports_responses {
        cfg.supports_responses = v;
    }
    match normalize_thinking_level_input(&provider, cfg.thinking_level.clone()) {
        Ok(v) => cfg.thinking_level = v,
        Err(err) => return (StatusCode::BAD_REQUEST, Json(json!({"error": err}))),
    }
    if let Err(err) = ai_repo::update_ai_model_config(&config_id, &cfg).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "更新AI模型配置失败", "detail": err})),
        );
    }
    let updated = match ai_repo::get_ai_model_config_by_id(&config_id).await {
        Ok(Some(cfg)) => cfg,
        Ok(None) => return (StatusCode::OK, Json(Value::Null)),
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "更新AI模型配置失败", "detail": err})),
            )
        }
    };
    (StatusCode::OK, Json(ai_model_config_value(&updated)))
}

async fn delete_ai_model_config(Path(config_id): Path<String>) -> (StatusCode, Json<Value>) {
    let existing = match ai_repo::get_ai_model_config_by_id(&config_id).await {
        Ok(cfg) => cfg,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "删除AI模型配置失败", "detail": err})),
            )
        }
    };
    if existing.is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "AI模型配置不存在"})),
        );
    }
    if let Err(err) = ai_repo::delete_ai_model_config(&config_id).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "删除AI模型配置失败", "detail": err})),
        );
    }
    (
        StatusCode::OK,
        Json(json!({ "message": "AI模型配置删除成功" })),
    )
}

async fn list_system_contexts(Query(query): Query<UserQuery>) -> (StatusCode, Json<Value>) {
    let user_id = match query.user_id {
        Some(u) => u,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "user_id 为必填参数"})),
            )
        }
    };
    let contexts = match ctx_repo::list_system_contexts(&user_id).await {
        Ok(list) => list,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "获取系统上下文失败", "detail": err})),
            )
        }
    };
    let mut out = Vec::new();
    for ctx in contexts {
        let app_ids = match ctx_repo::get_app_ids_for_system_context(&ctx.id).await {
            Ok(ids) => ids,
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "获取系统上下文失败", "detail": err})),
                )
            }
        };
        out.push(json!({
            "id": ctx.id,
            "name": ctx.name,
            "content": ctx.content,
            "user_id": ctx.user_id,
            "is_active": ctx.is_active,
            "created_at": ctx.created_at,
            "updated_at": ctx.updated_at,
            "app_ids": app_ids
        }));
    }
    (StatusCode::OK, Json(Value::Array(out)))
}

async fn get_active_system_context(Query(query): Query<UserQuery>) -> (StatusCode, Json<Value>) {
    let user_id = match query.user_id {
        Some(u) => u,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "user_id 为必填参数"})),
            )
        }
    };
    let ctx = match ctx_repo::get_active_system_context(&user_id).await {
        Ok(ctx) => ctx,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "获取活跃系统上下文失败", "detail": err})),
            )
        }
    };
    if let Some(ctx) = ctx {
        let app_ids = match ctx_repo::get_app_ids_for_system_context(&ctx.id).await {
            Ok(ids) => ids,
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "获取活跃系统上下文失败", "detail": err})),
                )
            }
        };
        return (
            StatusCode::OK,
            Json(json!({
                "content": ctx.content.clone().unwrap_or_default(),
                "context": {
                    "id": ctx.id,
                    "name": ctx.name,
                    "content": ctx.content,
                    "user_id": ctx.user_id,
                    "is_active": ctx.is_active,
                    "created_at": ctx.created_at,
                    "updated_at": ctx.updated_at,
                    "app_ids": app_ids
                }
            })),
        );
    }
    (
        StatusCode::OK,
        Json(json!({ "content": "", "context": Value::Null })),
    )
}

async fn create_system_context(Json(req): Json<SystemContextRequest>) -> (StatusCode, Json<Value>) {
    let Some(name) = req.name else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "创建系统上下文失败"})),
        );
    };
    let Some(user_id) = req.user_id else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "创建系统上下文失败"})),
        );
    };
    let id = Uuid::new_v4().to_string();
    let ctx = SystemContext {
        id: id.clone(),
        name,
        content: req.content,
        user_id,
        is_active: req.is_active.unwrap_or(false),
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    if let Err(err) = ctx_repo::create_system_context(&ctx).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "创建系统上下文失败", "detail": err})),
        );
    }
    if let Some(app_ids) = req.app_ids.clone() {
        if let Err(err) = ctx_repo::set_app_ids_for_system_context(&id, &app_ids).await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "创建系统上下文失败", "detail": err})),
            );
        }
    }
    let ctx = match ctx_repo::get_system_context_by_id(&id).await {
        Ok(Some(ctx)) => ctx,
        Ok(None) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "创建系统上下文失败"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "创建系统上下文失败", "detail": err})),
            )
        }
    };
    let app_ids = match ctx_repo::get_app_ids_for_system_context(&id).await {
        Ok(ids) => ids,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "创建系统上下文失败", "detail": err})),
            )
        }
    };
    let obj = system_context_value(&ctx, Some(app_ids));
    (StatusCode::CREATED, Json(obj))
}

async fn update_system_context(
    Path(context_id): Path<String>,
    Json(req): Json<SystemContextRequest>,
) -> (StatusCode, Json<Value>) {
    let existing = match ctx_repo::get_system_context_by_id(&context_id).await {
        Ok(ctx) => ctx,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "更新系统上下文失败", "detail": err})),
            )
        }
    };
    let Some(mut ctx) = existing else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "更新系统上下文失败"})),
        );
    };
    if let Some(v) = req.name {
        ctx.name = v;
    }
    if let Some(v) = req.content {
        ctx.content = Some(v);
    }
    if let Some(v) = req.is_active {
        ctx.is_active = v;
    }
    if let Err(err) = ctx_repo::update_system_context(&context_id, &ctx).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "更新系统上下文失败", "detail": err})),
        );
    }
    if let Some(app_ids) = req.app_ids {
        if let Err(err) = ctx_repo::set_app_ids_for_system_context(&context_id, &app_ids).await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "更新系统上下文失败", "detail": err})),
            );
        }
    }
    let ctx = match ctx_repo::get_system_context_by_id(&context_id).await {
        Ok(Some(ctx)) => ctx,
        Ok(None) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "更新系统上下文失败"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "更新系统上下文失败", "detail": err})),
            )
        }
    };
    let app_ids = match ctx_repo::get_app_ids_for_system_context(&context_id).await {
        Ok(ids) => ids,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "更新系统上下文失败", "detail": err})),
            )
        }
    };
    let obj = system_context_value(&ctx, Some(app_ids));
    (StatusCode::OK, Json(obj))
}

async fn delete_system_context(Path(context_id): Path<String>) -> (StatusCode, Json<Value>) {
    if let Err(err) = ctx_repo::delete_system_context(&context_id).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "删除系统上下文失败", "detail": err})),
        );
    }
    (
        StatusCode::OK,
        Json(json!({ "message": "系统上下文删除成功" })),
    )
}

#[derive(Debug, Deserialize)]
struct ActivateContextRequest {
    user_id: Option<String>,
}

async fn activate_system_context(
    Path(context_id): Path<String>,
    Json(req): Json<ActivateContextRequest>,
) -> (StatusCode, Json<Value>) {
    let user_id = match req.user_id {
        Some(u) => u,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "激活系统上下文失败"})),
            )
        }
    };
    if let Err(err) = ctx_repo::activate_system_context(&context_id, &user_id).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "激活系统上下文失败", "detail": err})),
        );
    }
    let list = match ctx_repo::list_system_contexts(&user_id).await {
        Ok(list) => list,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "激活系统上下文失败", "detail": err})),
            )
        }
    };
    let activated = list.into_iter().find(|c| c.id == context_id);
    let out = activated
        .map(|ctx| system_context_value(&ctx, None))
        .unwrap_or(Value::Null);
    (StatusCode::OK, Json(out))
}

fn parse_args(args: &Option<Value>) -> Vec<String> {
    match args {
        Some(Value::String(s)) => {
            if let Ok(v) = serde_json::from_str::<Vec<Value>>(s) {
                return v
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            return Vec::new();
        }
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

fn value_to_env_string(v: &Value) -> Option<String> {
    if v.is_null() {
        return None;
    }
    if let Some(s) = v.as_str() {
        return Some(s.to_string());
    }
    Some(v.to_string())
}

fn parse_env(env: &Option<Value>) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    match env {
        Some(Value::String(s)) => {
            if let Ok(v) = serde_json::from_str::<Value>(s) {
                if let Value::Object(obj) = v {
                    for (k, v) in obj {
                        if let Some(val) = value_to_env_string(&v) {
                            map.insert(k, val);
                        }
                    }
                }
            }
        }
        Some(Value::Object(obj)) => {
            for (k, v) in obj {
                if let Some(val) = value_to_env_string(v) {
                    map.insert(k.clone(), val);
                }
            }
        }
        _ => {}
    }
    map
}

async fn read_mcp_resource_config(
    command: &str,
    args: &[String],
    env: &std::collections::HashMap<String, String>,
    cwd: Option<&str>,
) -> Result<String, String> {
    use std::process::Stdio;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let mut cmd = tokio::process::Command::new(command);
    if !args.is_empty() {
        cmd.args(args);
    }
    if !env.is_empty() {
        cmd.envs(env);
    }
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let id = Uuid::new_v4().to_string();
    let payload = json!({"jsonrpc":"2.0","id": id, "method":"resources/read", "params": { "uri": "config://server" }});
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all((payload.to_string() + "\n").as_bytes())
            .await
            .map_err(|e| e.to_string())?;
    }
    let stdout = child.stdout.take().ok_or("missing stdout")?;
    let mut reader = BufReader::new(stdout).lines();
    while let Some(line) = reader.next_line().await.map_err(|e| e.to_string())? {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<Value>(&line) {
            if v.get("id").and_then(|v| v.as_str()) == Some(&id) {
                if v.get("error").is_some() {
                    return Err(v.to_string());
                }
                let result = v.get("result").cloned().unwrap_or(v);
                if let Some(contents) = result.get("contents").and_then(|v| v.as_array()) {
                    if let Some(first) = contents.first() {
                        if let Some(text) = first.get("text").and_then(|v| v.as_str()) {
                            return Ok(text.to_string());
                        }
                        return Ok(first.to_string());
                    }
                }
                return Ok(result.to_string());
            }
        }
    }
    Err("no response from stdio server".to_string())
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

fn ai_model_config_value(cfg: &AiModelConfig) -> Value {
    json!({
        "id": cfg.id.clone(),
        "name": cfg.name.clone(),
        "provider": cfg.provider.clone(),
        "model": cfg.model.clone(),
        "thinking_level": cfg.thinking_level.clone(),
        "api_key": cfg.api_key.clone(),
        "base_url": cfg.base_url.clone(),
        "user_id": cfg.user_id.clone(),
        "enabled": cfg.enabled,
        "supports_images": cfg.supports_images,
        "supports_reasoning": cfg.supports_reasoning,
        "supports_responses": cfg.supports_responses,
        "created_at": cfg.created_at.clone(),
        "updated_at": cfg.updated_at.clone()
    })
}

fn normalize_provider_input(provider: Option<String>) -> Result<String, String> {
    let raw = provider.unwrap_or_else(|| "gpt".to_string());
    let p = raw.trim().to_lowercase();
    let p = if p == "openai" { "gpt".to_string() } else { p };
    match p.as_str() {
        "gpt" | "deepseek" | "kimik2" => Ok(p),
        _ => Err("provider 仅支持 gpt / deepseek / kimik2".to_string()),
    }
}

fn normalize_thinking_level_input(
    provider: &str,
    level: Option<String>,
) -> Result<Option<String>, String> {
    let level = level
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    if level.is_none() {
        return Ok(None);
    }
    let provider = normalize_provider_input(Some(provider.to_string()))?;
    if provider != "gpt" {
        return Err("只有 gpt 支持思考等级".to_string());
    }
    let lvl = level.unwrap().to_lowercase();
    let allowed = ["none", "minimal", "low", "medium", "high", "xhigh"];
    if !allowed.contains(&lvl.as_str()) {
        return Err("思考等级仅支持 none/minimal/low/medium/high/xhigh".to_string());
    }
    Ok(Some(lvl))
}

fn system_context_value(ctx: &SystemContext, app_ids: Option<Vec<String>>) -> Value {
    let mut obj = json!({
        "id": ctx.id.clone(),
        "name": ctx.name.clone(),
        "content": ctx.content.clone(),
        "user_id": ctx.user_id.clone(),
        "is_active": ctx.is_active,
        "created_at": ctx.created_at.clone(),
        "updated_at": ctx.updated_at.clone()
    });
    if let Some(ids) = app_ids {
        if let Some(map) = obj.as_object_mut() {
            map.insert("app_ids".to_string(), json!(ids));
        }
    }
    obj
}
