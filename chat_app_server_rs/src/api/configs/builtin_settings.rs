use axum::extract::Path;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

use crate::builtin::sub_agent_router::{
    get_mcp_permissions_settings as get_sub_agent_router_mcp_permissions,
    import_agents_from_json as import_sub_agent_router_agents,
    import_from_git as import_sub_agent_router_from_git,
    import_skills_from_json as import_sub_agent_router_skills,
    install_plugins_from_marketplace as install_sub_agent_router_plugins,
    save_mcp_permissions_settings as save_sub_agent_router_mcp_permissions,
    summarize_settings as summarize_sub_agent_router_settings,
};
use crate::models::mcp_config::McpConfig;
use crate::repositories::mcp_configs as mcp_repo;
use crate::repositories::system_contexts as ctx_repo;
use crate::services::builtin_mcp::{
    builtin_display_name, list_builtin_mcp_configs, SUB_AGENT_ROUTER_MCP_ID,
};

use super::{
    BuiltinGitImportRequest, BuiltinImportRequest, BuiltinMcpPermissionsRequest,
    BuiltinPluginInstallRequest,
};

pub(super) async fn get_builtin_mcp_settings(
    Path(config_id): Path<String>,
) -> (StatusCode, Json<Value>) {
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

pub(super) async fn get_builtin_mcp_permissions(
    Path(config_id): Path<String>,
) -> (StatusCode, Json<Value>) {
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

pub(super) async fn update_builtin_mcp_permissions(
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

    let previous_selected_system_context_id =
        get_sub_agent_router_mcp_permissions()
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
                    Json(
                        json!({"error": format!("Failed to validate selected system prompt: {}", err)}),
                    ),
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

pub(super) async fn import_builtin_agents(
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

pub(super) async fn import_builtin_skills(
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

pub(super) async fn import_builtin_from_git(
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

pub(super) async fn install_builtin_plugin(
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
