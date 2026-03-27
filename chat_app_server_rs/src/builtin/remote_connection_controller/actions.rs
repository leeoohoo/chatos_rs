use std::cmp::Ordering;

use serde::Serialize;
use serde_json::{json, Value};
use tokio::time::Duration;

use crate::api::remote_connections::{run_remote_connectivity_test, run_ssh_command};
use crate::models::remote_connection::{RemoteConnection, RemoteConnectionService};

use super::context::{
    command_danger_reason, join_remote_path, normalize_remote_path, required_user_id,
    resolve_connection_id, shell_quote, truncate_text,
};
use super::BoundContext;

#[derive(Debug, Serialize)]
struct ConnectionSummary {
    id: String,
    name: String,
    host: String,
    port: i64,
    username: String,
    auth_type: String,
    default_remote_path: Option<String>,
}

#[derive(Debug, Serialize)]
struct RemoteEntry {
    path: String,
    name: String,
    is_dir: bool,
    size: Option<u64>,
    modified_at: Option<String>,
}

pub(super) async fn list_connections_with_context(ctx: BoundContext) -> Result<Value, String> {
    let user_id = required_user_id(&ctx)?;
    let mut list = RemoteConnectionService::list(Some(user_id)).await?;
    list.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));

    let connections: Vec<ConnectionSummary> = list
        .into_iter()
        .map(|item| ConnectionSummary {
            id: item.id,
            name: item.name,
            host: item.host,
            port: item.port,
            username: item.username,
            auth_type: item.auth_type,
            default_remote_path: item.default_remote_path,
        })
        .collect();

    Ok(json!({
        "count": connections.len(),
        "connections": connections,
    }))
}

pub(super) async fn test_connection_with_context(
    ctx: BoundContext,
    explicit_connection_id: Option<String>,
) -> Result<Value, String> {
    let connection = resolve_owned_connection(&ctx, explicit_connection_id).await?;
    let result = run_remote_connectivity_test(&connection).await?;
    let _ = RemoteConnectionService::touch(&connection.id).await;

    Ok(json!({
        "connection_id": connection.id,
        "name": connection.name,
        "host": connection.host,
        "port": connection.port,
        "username": connection.username,
        "result": result,
    }))
}

pub(super) async fn run_command_with_context(
    ctx: BoundContext,
    explicit_connection_id: Option<String>,
    command: String,
    timeout_seconds: Option<u64>,
    allow_dangerous: bool,
    max_output_chars: Option<usize>,
) -> Result<Value, String> {
    if let Some(reason) = command_danger_reason(command.as_str()) {
        if !allow_dangerous {
            return Err(format!(
                "{reason}。如确实需要执行，请显式设置 allow_dangerous=true"
            ));
        }
    }

    let connection = resolve_owned_connection(&ctx, explicit_connection_id).await?;
    let timeout = timeout_seconds
        .unwrap_or(ctx.command_timeout_seconds)
        .clamp(1, ctx.max_command_timeout_seconds);
    let output_limit = max_output_chars
        .unwrap_or(ctx.max_output_chars)
        .clamp(128, ctx.max_output_chars.max(128));

    let output =
        run_ssh_command(&connection, command.as_str(), Duration::from_secs(timeout)).await?;
    let (output_text, truncated) = truncate_text(output.as_str(), output_limit);
    let _ = RemoteConnectionService::touch(&connection.id).await;

    Ok(json!({
        "connection_id": connection.id,
        "name": connection.name,
        "host": connection.host,
        "port": connection.port,
        "username": connection.username,
        "command": command,
        "timeout_seconds": timeout,
        "output_chars": output_text.chars().count(),
        "output_truncated": truncated,
        "output": output_text,
    }))
}

pub(super) async fn list_directory_with_context(
    ctx: BoundContext,
    explicit_connection_id: Option<String>,
    input_path: Option<String>,
    limit: Option<usize>,
) -> Result<Value, String> {
    let connection = resolve_owned_connection(&ctx, explicit_connection_id).await?;
    let path = normalize_remote_path(
        input_path
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .or(connection.default_remote_path.as_deref())
            .unwrap_or("."),
    );
    let entry_limit = limit.unwrap_or(200).clamp(1, 1000);

    let script = format!(
        "set -e; P={quoted}; if [ ! -d \"$P\" ]; then printf '__CHATOS_DIR_NOT_FOUND__\\n'; else cd \"$P\"; find . -mindepth 1 -maxdepth 1 -printf '%P\\t%y\\t%s\\t%T@\\n'; fi",
        quoted = shell_quote(path.as_str()),
    );
    let output = run_ssh_command(
        &connection,
        script.as_str(),
        Duration::from_secs(ctx.command_timeout_seconds),
    )
    .await?;
    if output.contains("__CHATOS_DIR_NOT_FOUND__") {
        return Err(format!("远程目录不存在: {path}"));
    }

    let mut entries = parse_directory_entries(path.as_str(), output.as_str());
    entries.sort_by(|left, right| match (left.is_dir, right.is_dir) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
    });
    let truncated = entries.len() > entry_limit;
    if truncated {
        entries.truncate(entry_limit);
    }

    let _ = RemoteConnectionService::touch(&connection.id).await;
    Ok(json!({
        "connection_id": connection.id,
        "path": path,
        "count": entries.len(),
        "entries_truncated": truncated,
        "entries": entries,
    }))
}

pub(super) async fn read_file_with_context(
    ctx: BoundContext,
    explicit_connection_id: Option<String>,
    path: String,
    max_bytes: Option<usize>,
) -> Result<Value, String> {
    let connection = resolve_owned_connection(&ctx, explicit_connection_id).await?;
    let normalized_path = normalize_remote_path(path.as_str());
    let read_limit = max_bytes
        .unwrap_or(ctx.max_read_file_bytes)
        .clamp(1, ctx.max_read_file_bytes.max(1));

    let script = format!(
        "set -e; P={quoted}; if [ ! -f \"$P\" ]; then printf '__CHATOS_FILE_NOT_FOUND__\\n'; else SZ=$(wc -c < \"$P\" 2>/dev/null || echo 0); head -c {limit} \"$P\"; if [ \"$SZ\" -gt {limit} ]; then printf '\\n__CHATOS_FILE_TRUNCATED__ size=%s limit={limit}\\n' \"$SZ\"; fi; fi",
        quoted = shell_quote(normalized_path.as_str()),
        limit = read_limit,
    );
    let output = run_ssh_command(
        &connection,
        script.as_str(),
        Duration::from_secs(ctx.command_timeout_seconds),
    )
    .await?;
    if output.contains("__CHATOS_FILE_NOT_FOUND__") {
        return Err(format!("远程文件不存在: {normalized_path}"));
    }

    let (content, truncated, source_size) = split_file_output(output);
    let _ = RemoteConnectionService::touch(&connection.id).await;

    Ok(json!({
        "connection_id": connection.id,
        "path": normalized_path,
        "max_bytes": read_limit,
        "source_size_bytes": source_size,
        "truncated": truncated,
        "content": content,
    }))
}

async fn resolve_owned_connection(
    ctx: &BoundContext,
    explicit_connection_id: Option<String>,
) -> Result<RemoteConnection, String> {
    let connection_id = resolve_connection_id(ctx, explicit_connection_id)?;
    let user_id = required_user_id(ctx)?;
    let connection = RemoteConnectionService::get_by_id(connection_id.as_str())
        .await?
        .ok_or_else(|| format!("远端连接不存在: {connection_id}"))?;

    if connection.user_id.as_deref() != Some(user_id.as_str()) {
        return Err("无权访问该远端连接".to_string());
    }
    Ok(connection)
}

fn parse_directory_entries(base_path: &str, output: &str) -> Vec<RemoteEntry> {
    let mut out = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.split('\t');
        let name = parts.next().unwrap_or("").trim().to_string();
        if name.is_empty() || name == "." || name == ".." {
            continue;
        }
        let kind = parts.next().unwrap_or("f");
        let size = parts.next().and_then(|value| value.parse::<u64>().ok());
        let modified_at = parts.next().map(|value| value.to_string());
        let is_dir = kind == "d";
        out.push(RemoteEntry {
            path: join_remote_path(base_path, name.as_str()),
            name,
            is_dir,
            size,
            modified_at,
        });
    }
    out
}

fn split_file_output(output: String) -> (String, bool, Option<u64>) {
    const MARKER: &str = "__CHATOS_FILE_TRUNCATED__";
    if let Some(index) = output.rfind(MARKER) {
        let mut content = output[..index].to_string();
        while content.ends_with('\n') || content.ends_with('\r') {
            content.pop();
        }
        let tail = output[index..].trim();
        let source_size = tail
            .split_whitespace()
            .find_map(|chunk| chunk.strip_prefix("size="))
            .and_then(|value| value.parse::<u64>().ok());
        return (content, true, source_size);
    }
    (output, false, None)
}
