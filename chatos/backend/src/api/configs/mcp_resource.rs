// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::Path;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader};
use uuid::Uuid;

use crate::core::auth::AuthUser;
use crate::core::mcp_args::{parse_args_json_array, parse_env};
use crate::core::mcp_config_access::{ensure_owned_mcp_config, map_mcp_config_access_error};
use crate::repositories::mcp_configs as mcp_repo;
use crate::services::builtin_mcp::{get_builtin_mcp_config, is_builtin_mcp_id};

use super::{authorize_mcp_cwd_or_default, ResourceByCommandRequest};

const MCP_RESOURCE_STDIO_TIMEOUT: Duration = Duration::from_secs(15);
const MCP_RESOURCE_RESPONSE_LINE_LIMIT_BYTES: usize = 2 * 1024 * 1024;
const MCP_RESOURCE_TEXT_LIMIT_BYTES: usize = 1 * 1024 * 1024;

pub(super) async fn get_mcp_resource_config(
    auth: AuthUser,
    Path(config_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    if is_builtin_mcp_id(&config_id) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "内置 MCP 不支持资源配置读取"})),
        );
    }
    if let Err(err) = ensure_owned_mcp_config(&config_id, &auth).await {
        return map_mcp_config_access_error(err);
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
            );
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
    let cwd = match authorize_mcp_cwd_or_default(&auth, cfg.cwd.clone()).await {
        Ok(path) => path,
        Err(err) => return err,
    };
    match read_mcp_resource_config(
        &cfg.command,
        &args,
        &env,
        cwd.as_deref(),
        Some(&auth.user_id),
    )
    .await
    {
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
    auth: AuthUser,
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
            );
        }
    };
    let args = parse_args_json_array(&req.args);
    let env = parse_env(&req.env);
    let alias = req.alias.unwrap_or_else(|| "mcp_server".to_string());
    let cwd = match authorize_mcp_cwd_or_default(&auth, req.cwd).await {
        Ok(path) => path,
        Err(err) => return err,
    };
    match read_mcp_resource_config(&command, &args, &env, cwd.as_deref(), Some(&auth.user_id)).await
    {
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
    user_id: Option<&str>,
) -> Result<String, String> {
    tokio::time::timeout(
        MCP_RESOURCE_STDIO_TIMEOUT,
        read_mcp_resource_config_inner(command, args, env, cwd, user_id),
    )
    .await
    .map_err(|_| {
        format!(
            "stdio MCP resource read timed out after {}s",
            MCP_RESOURCE_STDIO_TIMEOUT.as_secs()
        )
    })?
}

async fn read_mcp_resource_config_inner(
    command: &str,
    args: &[String],
    env: &HashMap<String, String>,
    cwd: Option<&str>,
    _user_id: Option<&str>,
) -> Result<String, String> {
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
    cmd.kill_on_drop(true);

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
    let mut reader = BufReader::new(stdout);
    while let Some(line) =
        read_mcp_resource_line_limited(&mut reader, MCP_RESOURCE_RESPONSE_LINE_LIMIT_BYTES).await?
    {
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
                            ensure_mcp_resource_text_within_limit(text)?;
                            return Ok(text.to_string());
                        }
                        let text = first.to_string();
                        ensure_mcp_resource_text_within_limit(text.as_str())?;
                        return Ok(text);
                    }
                }
                let text = result.to_string();
                ensure_mcp_resource_text_within_limit(text.as_str())?;
                return Ok(text);
            }
        }
    }
    Err("no response from stdio server".to_string())
}

async fn read_mcp_resource_line_limited<R>(
    reader: &mut R,
    limit_bytes: usize,
) -> Result<Option<String>, String>
where
    R: AsyncBufRead + Unpin,
{
    let mut line = Vec::new();
    loop {
        let available = reader.fill_buf().await.map_err(|err| err.to_string())?;
        if available.is_empty() {
            if line.is_empty() {
                return Ok(None);
            }
            break;
        }

        let take_len = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|index| index + 1)
            .unwrap_or(available.len());
        let next_len = line.len().saturating_add(take_len);
        ensure_mcp_resource_line_within_limit(next_len, limit_bytes)?;
        line.extend_from_slice(&available[..take_len]);
        reader.consume(take_len);
        if line.last().copied() == Some(b'\n') {
            break;
        }
    }

    while matches!(line.last().copied(), Some(b'\n' | b'\r')) {
        line.pop();
    }
    String::from_utf8(line)
        .map(Some)
        .map_err(|err| format!("MCP resource response was not UTF-8: {err}"))
}

fn ensure_mcp_resource_line_within_limit(
    actual_bytes: usize,
    limit_bytes: usize,
) -> Result<(), String> {
    if actual_bytes > limit_bytes {
        return Err(format!(
            "MCP resource response line exceeded limit: {actual_bytes} bytes > {limit_bytes} bytes"
        ));
    }
    Ok(())
}

fn ensure_mcp_resource_text_within_limit(text: &str) -> Result<(), String> {
    let actual_bytes = text.len();
    if actual_bytes > MCP_RESOURCE_TEXT_LIMIT_BYTES {
        return Err(format!(
            "MCP resource config text exceeded limit: {actual_bytes} bytes > {MCP_RESOURCE_TEXT_LIMIT_BYTES} bytes"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_mcp_resource_line_within_limit, ensure_mcp_resource_text_within_limit,
        MCP_RESOURCE_TEXT_LIMIT_BYTES,
    };

    #[test]
    fn mcp_resource_line_limit_accepts_boundary_size() {
        assert!(ensure_mcp_resource_line_within_limit(1024, 1024).is_ok());
    }

    #[test]
    fn mcp_resource_line_limit_rejects_oversized_line() {
        let err = ensure_mcp_resource_line_within_limit(1025, 1024)
            .expect_err("oversized line should fail");

        assert!(err.contains("exceeded limit"));
        assert!(err.contains("1025 bytes > 1024 bytes"));
    }

    #[test]
    fn mcp_resource_text_limit_rejects_oversized_text() {
        let text = "x".repeat(MCP_RESOURCE_TEXT_LIMIT_BYTES + 1);
        let err = ensure_mcp_resource_text_within_limit(text.as_str())
            .expect_err("oversized text should fail");

        assert!(err.contains("exceeded limit"));
    }
}
