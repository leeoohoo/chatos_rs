use std::collections::HashMap;
use std::process::Stdio;
use std::sync::OnceLock;

use bytes::BytesMut;
use futures::StreamExt;
use serde_json::{json, Value};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader};
use uuid::Uuid;

use crate::services::mcp_loader::McpStdioServer;

static MCP_HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

const MCP_HTTP_RESPONSE_LIMIT_BYTES: usize = 4 * 1024 * 1024;
const MCP_HTTP_ERROR_BODY_PREVIEW_BYTES: usize = 16 * 1024;
const MCP_STDIO_RESPONSE_LINE_LIMIT_BYTES: usize = 4 * 1024 * 1024;

pub async fn jsonrpc_http_call(
    url: &str,
    headers: Option<&HashMap<String, String>>,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    let id = Uuid::new_v4().to_string();
    let payload = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
    let client = mcp_http_client();
    let mut request = client.post(url).json(&payload);
    if let Some(headers) = headers {
        for (key, value) in headers {
            request = request.header(key.as_str(), value.as_str());
        }
    }
    let response = request.send().await.map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body = read_mcp_http_body_limited(response, MCP_HTTP_ERROR_BODY_PREVIEW_BYTES)
            .await
            .map(|bytes| String::from_utf8_lossy(bytes.as_ref()).into_owned())
            .unwrap_or_default();
        return Err(format!("MCP HTTP request failed: {status} {body}"));
    }

    let body = read_mcp_http_body_limited(response, MCP_HTTP_RESPONSE_LIMIT_BYTES).await?;
    let value: Value = serde_json::from_slice(body.as_ref()).map_err(|err| err.to_string())?;
    if value.get("error").is_some() {
        return Err(value.to_string());
    }

    Ok(value.get("result").cloned().unwrap_or(value))
}

fn mcp_http_client() -> &'static reqwest::Client {
    MCP_HTTP_CLIENT.get_or_init(reqwest::Client::new)
}

async fn read_mcp_http_body_limited(
    response: reqwest::Response,
    limit_bytes: usize,
) -> Result<bytes::Bytes, String> {
    if let Some(content_length) = response.content_length() {
        ensure_mcp_http_body_within_limit(content_length as usize, limit_bytes)?;
    }

    let mut body = BytesMut::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| err.to_string())?;
        let next_len = body.len().saturating_add(chunk.len());
        ensure_mcp_http_body_within_limit(next_len, limit_bytes)?;
        body.extend_from_slice(chunk.as_ref());
    }
    Ok(body.freeze())
}

fn ensure_mcp_http_body_within_limit(
    actual_bytes: usize,
    limit_bytes: usize,
) -> Result<(), String> {
    if actual_bytes > limit_bytes {
        return Err(format!(
            "MCP HTTP response exceeded limit: {actual_bytes} bytes > {limit_bytes} bytes"
        ));
    }
    Ok(())
}

pub async fn jsonrpc_stdio_call(
    cfg: &McpStdioServer,
    method: &str,
    params: Value,
    conversation_id: Option<&str>,
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
    cmd.kill_on_drop(true);

    let mut child = cmd.spawn().map_err(|err| err.to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        let data = payload.to_string() + "\n";
        stdin
            .write_all(data.as_bytes())
            .await
            .map_err(|err| err.to_string())?;
    }

    let stdout = child.stdout.take().ok_or("missing stdout")?;
    let mut reader = BufReader::new(stdout);

    loop {
        if conversation_id
            .map(crate::utils::abort_registry::is_aborted)
            .unwrap_or(false)
        {
            return Err("aborted".to_string());
        }

        match read_mcp_stdio_line_limited(&mut reader, MCP_STDIO_RESPONSE_LINE_LIMIT_BYTES).await {
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
            Err(err) => return Err(err),
        }
    }

    Err("no response from stdio server".to_string())
}

async fn read_mcp_stdio_line_limited<R>(
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
        ensure_mcp_stdio_line_within_limit(next_len, limit_bytes)?;
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
        .map_err(|err| format!("MCP stdio response was not UTF-8: {err}"))
}

fn ensure_mcp_stdio_line_within_limit(
    actual_bytes: usize,
    limit_bytes: usize,
) -> Result<(), String> {
    if actual_bytes > limit_bytes {
        return Err(format!(
            "MCP stdio response line exceeded limit: {actual_bytes} bytes > {limit_bytes} bytes"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ensure_mcp_http_body_within_limit, ensure_mcp_stdio_line_within_limit};

    #[test]
    fn mcp_http_body_limit_accepts_boundary_size() {
        assert!(ensure_mcp_http_body_within_limit(1024, 1024).is_ok());
    }

    #[test]
    fn mcp_http_body_limit_rejects_oversized_body() {
        let err =
            ensure_mcp_http_body_within_limit(1025, 1024).expect_err("oversized body should fail");

        assert!(err.contains("exceeded limit"));
        assert!(err.contains("1025 bytes > 1024 bytes"));
    }

    #[test]
    fn mcp_stdio_line_limit_accepts_boundary_size() {
        assert!(ensure_mcp_stdio_line_within_limit(1024, 1024).is_ok());
    }

    #[test]
    fn mcp_stdio_line_limit_rejects_oversized_line() {
        let err =
            ensure_mcp_stdio_line_within_limit(1025, 1024).expect_err("oversized line should fail");

        assert!(err.contains("exceeded limit"));
        assert!(err.contains("1025 bytes > 1024 bytes"));
    }
}
