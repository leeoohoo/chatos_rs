use std::process::Stdio;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use uuid::Uuid;

use crate::services::mcp_loader::McpStdioServer;

pub async fn list_tools_http(url: &str) -> Result<Vec<Value>, String> {
    let response = jsonrpc_http_call(url, "tools/list", json!({})).await?;
    extract_tools(&response)
}

pub async fn list_tools_stdio(cfg: &McpStdioServer) -> Result<Vec<Value>, String> {
    let response = jsonrpc_stdio_call(cfg, "tools/list", json!({}), None).await?;
    extract_tools(&response)
}

pub fn extract_tools(response: &Value) -> Result<Vec<Value>, String> {
    if let Some(arr) = response.get("tools").and_then(|value| value.as_array()) {
        return Ok(arr.clone());
    }

    if let Some(arr) = response
        .get("result")
        .and_then(|result| result.get("tools"))
        .and_then(|value| value.as_array())
    {
        return Ok(arr.clone());
    }

    Err("tools not found in response".to_string())
}

pub async fn jsonrpc_http_call(url: &str, method: &str, params: Value) -> Result<Value, String> {
    let id = Uuid::new_v4().to_string();
    let payload = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
    let response = reqwest::Client::new()
        .post(url)
        .json(&payload)
        .send()
        .await
        .map_err(|err| err.to_string())?;

    let value: Value = response.json().await.map_err(|err| err.to_string())?;
    if value.get("error").is_some() {
        return Err(value.to_string());
    }

    Ok(value.get("result").cloned().unwrap_or(value))
}

pub async fn jsonrpc_stdio_call(
    cfg: &McpStdioServer,
    method: &str,
    params: Value,
    session_id: Option<&str>,
) -> Result<Value, String> {
    let id = Uuid::new_v4().to_string();
    let payload = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});

    let mut cmd = tokio::process::Command::new(&cfg.command);
    if let Some(args) = &cfg.args {
        cmd.args(args);
    }
    if let Some(env) = &cfg.env {
        cmd.envs(env);
    }
    if let Some(cwd) = &cfg.cwd {
        cmd.current_dir(cwd);
    }

    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|err| err.to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        let data = payload.to_string() + "\n";
        stdin
            .write_all(data.as_bytes())
            .await
            .map_err(|err| err.to_string())?;
    }

    let stdout = child.stdout.take().ok_or("missing stdout")?;
    let mut reader = BufReader::new(stdout).lines();

    loop {
        if session_id
            .map(crate::utils::abort_registry::is_aborted)
            .unwrap_or(false)
        {
            return Err("aborted".to_string());
        }

        match reader.next_line().await {
            Ok(Some(line)) => {
                if line.trim().is_empty() {
                    continue;
                }

                if let Ok(value) = serde_json::from_str::<Value>(&line) {
                    if value.get("id").and_then(|value| value.as_str()) == Some(id.as_str()) {
                        if value.get("error").is_some() {
                            return Err(value.to_string());
                        }
                        return Ok(value.get("result").cloned().unwrap_or(value));
                    }
                }
            }
            Ok(None) => break,
            Err(err) => return Err(err.to_string()),
        }
    }

    Err("no response from stdio server".to_string())
}
