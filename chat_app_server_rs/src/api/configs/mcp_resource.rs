use axum::extract::Path;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;

use crate::core::mcp_args::{parse_args_json_array, parse_env};
use crate::repositories::mcp_configs as mcp_repo;
use crate::services::builtin_mcp::{get_builtin_mcp_config, is_builtin_mcp_id};

use super::ResourceByCommandRequest;

pub(super) async fn get_mcp_resource_config(
    Path(config_id): Path<String>,
) -> (StatusCode, Json<Value>) {
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
    let args = parse_args_json_array(&cfg.args);
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

pub(super) async fn post_mcp_resource_config(
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
    let args = parse_args_json_array(&req.args);
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

async fn read_mcp_resource_config(
    command: &str,
    args: &[String],
    env: &HashMap<String, String>,
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
