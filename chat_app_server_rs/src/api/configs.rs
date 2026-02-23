use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::builtin::sub_agent_router::{
    get_mcp_permissions_settings as get_sub_agent_router_mcp_permissions,
    import_agents_from_json as import_sub_agent_router_agents,
    import_from_git as import_sub_agent_router_from_git,
    import_skills_from_json as import_sub_agent_router_skills,
    install_plugins_from_marketplace as install_sub_agent_router_plugins,
    save_mcp_permissions_settings as save_sub_agent_router_mcp_permissions,
    summarize_settings as summarize_sub_agent_router_settings,
};
use crate::models::ai_model_config::AiModelConfig;
use crate::models::mcp_config::McpConfig;
use crate::models::system_context::SystemContext;
use crate::repositories::ai_model_configs as ai_repo;
use crate::repositories::mcp_configs as mcp_repo;
use crate::repositories::system_contexts as ctx_repo;
use crate::services::builtin_mcp::{
    builtin_display_name, get_builtin_mcp_config, is_builtin_mcp_id, list_builtin_mcp_configs,
    SUB_AGENT_ROUTER_MCP_ID,
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
            get(get_mcp_resource_config),
        )
        .route(
            "/api/mcp-configs/:config_id/builtin/settings",
            get(get_builtin_mcp_settings),
        )
        .route(
            "/api/mcp-configs/:config_id/builtin/mcp-permissions",
            get(get_builtin_mcp_permissions).post(update_builtin_mcp_permissions),
        )
        .route(
            "/api/mcp-configs/:config_id/builtin/import-agents",
            post(import_builtin_agents),
        )
        .route(
            "/api/mcp-configs/:config_id/builtin/import-skills",
            post(import_builtin_skills),
        )
        .route(
            "/api/mcp-configs/:config_id/builtin/import-git",
            post(import_builtin_from_git),
        )
        .route(
            "/api/mcp-configs/:config_id/builtin/install-plugin",
            post(install_builtin_plugin),
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

async fn get_builtin_mcp_settings(Path(config_id): Path<String>) -> (StatusCode, Json<Value>) {
    if config_id != SUB_AGENT_ROUTER_MCP_ID {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "该内置 MCP 暂不支持设置"})),
        );
    }
    match summarize_sub_agent_router_settings() {
        Ok(value) => (StatusCode::OK, Json(json!({"ok": true, "data": value}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("读取内置设置失败: {}", err)})),
        ),
    }
}

async fn get_builtin_mcp_permissions(Path(config_id): Path<String>) -> (StatusCode, Json<Value>) {
    if config_id != SUB_AGENT_ROUTER_MCP_ID {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "该内置 MCP 暂不支持 MCP 权限设置"})),
        );
    }

    let options = match list_sub_agent_router_mcp_options().await {
        Ok(items) => items,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("读取 MCP 列表失败: {}", err)})),
            )
        }
    };

    match get_sub_agent_router_mcp_permissions() {
        Ok(state) => {
            let payload = build_sub_agent_router_mcp_permissions_payload(state, options);
            (StatusCode::OK, Json(json!({"ok": true, "data": payload})))
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("读取 MCP 权限设置失败: {}", err)})),
        ),
    }
}

async fn update_builtin_mcp_permissions(
    Path(config_id): Path<String>,
    Json(req): Json<BuiltinMcpPermissionsRequest>,
) -> (StatusCode, Json<Value>) {
    if config_id != SUB_AGENT_ROUTER_MCP_ID {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Builtin MCP does not support MCP permission settings"})),
        );
    }

    let options = match list_sub_agent_router_mcp_options().await {
        Ok(items) => items,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("Failed to load MCP options: {}", err)})),
            )
        }
    };

    let mut option_prefix_map = HashMap::new();
    let mut option_id_set = HashSet::new();
    for item in &options {
        let id = item
            .get("id")
            .and_then(|v| v.as_str())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        let prefix = item
            .get("tool_prefix")
            .and_then(|v| v.as_str())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        if let Some(id) = id {
            option_id_set.insert(id.clone());
            if let Some(prefix) = prefix {
                option_prefix_map.insert(id, prefix);
            }
        }
    }

    let requested_ids = req.enabled_mcp_ids.unwrap_or_default();
    let normalized_requested_ids = normalize_string_list(requested_ids);

    let enabled_mcp_ids = normalized_requested_ids
        .iter()
        .filter(|id| option_id_set.contains(*id))
        .cloned()
        .collect::<Vec<_>>();

    let enabled_tool_prefixes = normalize_string_list(
        enabled_mcp_ids
            .iter()
            .filter_map(|id| option_prefix_map.get(id).cloned())
            .collect(),
    );

    let previous_selected_system_context_id = get_sub_agent_router_mcp_permissions()
        .ok()
        .and_then(|state| {
            state
                .get("selected_system_context_id")
                .and_then(|v| v.as_str())
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        });

    let selected_system_context_id_for_save = if req.selected_system_context_id.is_some() {
        req.selected_system_context_id
            .as_deref()
            .map(str::trim)
            .map(|v| v.to_string())
    } else {
        previous_selected_system_context_id
    };

    let selected_system_context_id_for_validation = selected_system_context_id_for_save
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());

    if let Some(context_id) = selected_system_context_id_for_validation {
        match ctx_repo::get_system_context_by_id(context_id).await {
            Ok(Some(_)) => {}
            Ok(None) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "Selected system prompt not found"})),
                )
            }
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": format!("Failed to validate selected system prompt: {}", err)})),
                )
            }
        }
    }

    match save_sub_agent_router_mcp_permissions(
        &enabled_mcp_ids,
        &enabled_tool_prefixes,
        selected_system_context_id_for_save.as_deref(),
    ) {
        Ok(state) => {
            let payload = build_sub_agent_router_mcp_permissions_payload(state, options);
            (StatusCode::OK, Json(json!({"ok": true, "data": payload})))
        }
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("Failed to save MCP permissions: {}", err)})),
        ),
    }
}

async fn import_builtin_agents(
    Path(config_id): Path<String>,
    Json(req): Json<BuiltinImportRequest>,
) -> (StatusCode, Json<Value>) {
    if config_id != SUB_AGENT_ROUTER_MCP_ID {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "该内置 MCP 暂不支持 agents 导入"})),
        );
    }

    let content = req
        .content
        .as_deref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let Some(content) = content else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "content 不能为空"})),
        );
    };

    match import_sub_agent_router_agents(content.as_str()) {
        Ok(value) => (StatusCode::OK, Json(json!({"ok": true, "data": value}))),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("导入 agents 失败: {}", err)})),
        ),
    }
}

async fn import_builtin_skills(
    Path(config_id): Path<String>,
    Json(req): Json<BuiltinImportRequest>,
) -> (StatusCode, Json<Value>) {
    if config_id != SUB_AGENT_ROUTER_MCP_ID {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "该内置 MCP 暂不支持 skills 导入"})),
        );
    }

    let content = req
        .content
        .as_deref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let Some(content) = content else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "content 不能为空"})),
        );
    };

    match import_sub_agent_router_skills(content.as_str()) {
        Ok(value) => (StatusCode::OK, Json(json!({"ok": true, "data": value}))),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("导入 skills 失败: {}", err)})),
        ),
    }
}

async fn import_builtin_from_git(
    Path(config_id): Path<String>,
    Json(req): Json<BuiltinGitImportRequest>,
) -> (StatusCode, Json<Value>) {
    if config_id != SUB_AGENT_ROUTER_MCP_ID {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "该内置 MCP 暂不支持 git 导入"})),
        );
    }

    let repository = req
        .repository
        .as_deref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let Some(repository) = repository else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "repository 不能为空"})),
        );
    };

    let branch = req
        .branch
        .as_deref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let agents_path = req
        .agents_path
        .as_deref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let skills_path = req
        .skills_path
        .as_deref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    match import_sub_agent_router_from_git(
        repository.as_str(),
        branch.as_deref(),
        agents_path.as_deref(),
        skills_path.as_deref(),
    ) {
        Ok(value) => (StatusCode::OK, Json(json!({"ok": true, "data": value}))),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("从 git 导入失败: {}", err)})),
        ),
    }
}

async fn install_builtin_plugin(
    Path(config_id): Path<String>,
    Json(req): Json<BuiltinPluginInstallRequest>,
) -> (StatusCode, Json<Value>) {
    if config_id != SUB_AGENT_ROUTER_MCP_ID {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "该内置 MCP 暂不支持 plugin 安装"})),
        );
    }

    let source = req
        .source
        .as_deref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let install_all = req.install_all.unwrap_or(false);

    if !install_all && source.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "source 不能为空（或设置 install_all=true）"})),
        );
    }

    match install_sub_agent_router_plugins(source.as_deref(), install_all) {
        Ok(value) => (StatusCode::OK, Json(json!({"ok": true, "data": value}))),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("安装 plugin 失败: {}", err)})),
        ),
    }
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
        created_at: crate::core::time::now_rfc3339(),
        updated_at: crate::core::time::now_rfc3339(),
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
        created_at: crate::core::time::now_rfc3339(),
        updated_at: crate::core::time::now_rfc3339(),
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

fn normalize_string_list(values: Vec<String>) -> Vec<String> {
    let mut out = values
        .into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    out.sort();
    out.dedup();
    out
}

fn parse_string_array_value(value: Option<&Value>) -> Vec<String> {
    let Some(items) = value.and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    normalize_string_list(
        items
            .iter()
            .filter_map(|item| item.as_str().map(ToString::to_string))
            .collect(),
    )
}

fn mcp_tool_prefix(cfg: &McpConfig) -> String {
    let server_name = cfg.name.trim();
    let server_name = if server_name.is_empty() {
        "mcp_server"
    } else {
        server_name
    };
    let id_prefix = &cfg.id[..8.min(cfg.id.len())];
    format!("{}_{}", server_name, id_prefix)
}

fn mcp_option_value(cfg: &McpConfig, builtin: bool) -> Value {
    let display_name = if builtin {
        builtin_display_name(&cfg.id)
            .map(ToString::to_string)
            .unwrap_or_else(|| cfg.name.clone())
    } else {
        cfg.name.clone()
    };

    json!({
        "id": cfg.id.clone(),
        "name": cfg.name.clone(),
        "display_name": display_name,
        "builtin": builtin,
        "readonly": builtin,
        "config_enabled": cfg.enabled,
        "type": cfg.r#type.clone(),
        "command": cfg.command.clone(),
        "tool_prefix": mcp_tool_prefix(cfg)
    })
}

async fn list_sub_agent_router_mcp_options() -> Result<Vec<Value>, String> {
    let custom_configs = mcp_repo::list_mcp_configs(None).await?;

    let mut seen_ids = HashSet::new();
    let mut options = Vec::new();

    for cfg in list_builtin_mcp_configs() {
        if cfg.id == SUB_AGENT_ROUTER_MCP_ID {
            continue;
        }
        if seen_ids.insert(cfg.id.clone()) {
            options.push(mcp_option_value(&cfg, true));
        }
    }

    for cfg in custom_configs {
        if cfg.id == SUB_AGENT_ROUTER_MCP_ID {
            continue;
        }
        if seen_ids.insert(cfg.id.clone()) {
            options.push(mcp_option_value(&cfg, false));
        }
    }

    options.sort_by(|left, right| {
        let left_builtin = left
            .get("builtin")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let right_builtin = right
            .get("builtin")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if left_builtin != right_builtin {
            return right_builtin.cmp(&left_builtin);
        }

        let left_name = left
            .get("display_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();
        let right_name = right
            .get("display_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();
        left_name.cmp(&right_name)
    });

    Ok(options)
}

fn build_sub_agent_router_mcp_permissions_payload(state: Value, options: Vec<Value>) -> Value {
    let configured = state
        .get("configured")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let saved_ids = parse_string_array_value(state.get("enabled_mcp_ids"));
    let saved_prefixes = parse_string_array_value(state.get("enabled_tool_prefixes"));

    let mut option_ids = Vec::new();
    let mut option_prefix_map: HashMap<String, String> = HashMap::new();
    let mut option_items = Vec::with_capacity(options.len());

    for item in options {
        let id = item
            .get("id")
            .and_then(|v| v.as_str())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        if let Some(id) = id {
            if !option_ids.contains(&id) {
                option_ids.push(id.clone());
            }
            if let Some(prefix) = item
                .get("tool_prefix")
                .and_then(|v| v.as_str())
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
            {
                option_prefix_map.insert(id, prefix);
            }
        }
        option_items.push(item);
    }

    let option_id_set = option_ids.iter().cloned().collect::<HashSet<_>>();
    let unknown_saved_ids = saved_ids
        .iter()
        .filter(|id| !option_id_set.contains(*id))
        .cloned()
        .collect::<Vec<_>>();

    let effective_enabled_ids = if configured {
        saved_ids
            .iter()
            .filter(|id| option_id_set.contains(*id))
            .cloned()
            .collect::<Vec<_>>()
    } else {
        option_ids.clone()
    };
    let enabled_set = effective_enabled_ids
        .iter()
        .cloned()
        .collect::<HashSet<String>>();

    let effective_prefixes = normalize_string_list(
        effective_enabled_ids
            .iter()
            .filter_map(|id| option_prefix_map.get(id).cloned())
            .collect(),
    );

    let options_with_enabled = option_items
        .into_iter()
        .map(|mut item| {
            let enabled = item
                .get("id")
                .and_then(|v| v.as_str())
                .map(|id| enabled_set.contains(id))
                .unwrap_or(false);
            if let Some(map) = item.as_object_mut() {
                map.insert("enabled".to_string(), json!(enabled));
            }
            item
        })
        .collect::<Vec<_>>();

    let selected_system_context_id = state
        .get("selected_system_context_id")
        .and_then(|v| v.as_str())
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string());

    json!({
        "configured": configured,
        "updated_at": state.get("updated_at").cloned().unwrap_or(Value::Null),
        "path": state.get("path").cloned().unwrap_or(Value::Null),
        "enabled_mcp_ids": effective_enabled_ids,
        "saved_enabled_mcp_ids": saved_ids,
        "unknown_mcp_ids": unknown_saved_ids,
        "enabled_tool_prefixes": effective_prefixes,
        "saved_enabled_tool_prefixes": saved_prefixes,
        "selected_system_context_id": selected_system_context_id,
        "options": options_with_enabled
    })
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
