// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::cmp::Ordering;

use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use chatos_mcp::{RemoteConnectionControllerContext, RemoteConnectionControllerStore};
use serde_json::{json, Value};
use tokio::time::Duration;

use super::ssh::{
    download_sftp_file, run_ssh_command, test_remote_server_connectivity, upload_sftp_file,
};
use super::store_helpers::{persist_test_result, resolve_enabled_server, touch_server};
use super::support::{
    command_danger_reason, normalize_remote_path, parse_directory_entries, resolve_connection_id,
    shell_quote, split_file_output, truncate_text, ConnectionSummary,
};
use super::TaskRunnerRemoteConnectionStore;

#[async_trait]
impl RemoteConnectionControllerStore for TaskRunnerRemoteConnectionStore {
    async fn list_connections(
        &self,
        context: RemoteConnectionControllerContext,
    ) -> Result<Value, String> {
        let mut list = self
            .store
            .list_remote_servers()
            .await?
            .into_iter()
            .filter(|item| item.enabled)
            .collect::<Vec<_>>();
        if let Some(default_connection_id) = context
            .default_remote_connection_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            list.retain(|item| item.id == default_connection_id);
        }
        list.sort_by_key(|entry| entry.name.to_lowercase());
        let connections = list
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
            .collect::<Vec<_>>();

        Ok(json!({
            "count": connections.len(),
            "connections": connections,
        }))
    }

    async fn test_connection(
        &self,
        context: RemoteConnectionControllerContext,
        connection_id: Option<String>,
    ) -> Result<Value, String> {
        let connection_id = resolve_connection_id(&context, connection_id)?;
        let server = resolve_enabled_server(self, &connection_id).await?;
        let response = match test_remote_server_connectivity(&server, Some(server.id.clone())).await
        {
            Ok(response) => {
                persist_test_result(self, &server.id, true, response.remote_host.clone()).await?;
                response
            }
            Err(err) => {
                persist_test_result(self, &server.id, false, Some(err.clone())).await?;
                return Err(err);
            }
        };
        touch_server(self, &server.id).await?;

        Ok(json!({
            "connection_id": server.id,
            "name": server.name,
            "host": server.host,
            "port": server.port,
            "username": server.username,
            "result": {
                "success": response.ok,
                "remote_host": response.remote_host,
                "connected_at": response.tested_at,
            },
        }))
    }

    async fn run_command(
        &self,
        context: RemoteConnectionControllerContext,
        connection_id: Option<String>,
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

        let connection_id = resolve_connection_id(&context, connection_id)?;
        let server = resolve_enabled_server(self, &connection_id).await?;
        let timeout = timeout_seconds
            .unwrap_or(context.command_timeout_seconds)
            .clamp(1, context.max_command_timeout_seconds);
        let output_limit = max_output_chars
            .unwrap_or(context.max_output_chars)
            .clamp(128, context.max_output_chars.max(128));

        let output =
            run_ssh_command(&server, command.as_str(), Duration::from_secs(timeout)).await?;
        let (output_text, truncated) = truncate_text(output.as_str(), output_limit);
        touch_server(self, &server.id).await?;

        Ok(json!({
            "connection_id": server.id,
            "name": server.name,
            "host": server.host,
            "port": server.port,
            "username": server.username,
            "command": command,
            "timeout_seconds": timeout,
            "output_chars": output_text.chars().count(),
            "output_truncated": truncated,
            "output": output_text,
        }))
    }

    async fn list_directory(
        &self,
        context: RemoteConnectionControllerContext,
        connection_id: Option<String>,
        path: Option<String>,
        limit: Option<usize>,
    ) -> Result<Value, String> {
        let connection_id = resolve_connection_id(&context, connection_id)?;
        let server = resolve_enabled_server(self, &connection_id).await?;
        let normalized_path = normalize_remote_path(
            path.as_deref()
                .filter(|value| !value.trim().is_empty())
                .or(server.default_remote_path.as_deref())
                .unwrap_or("."),
        );
        let entry_limit = limit.unwrap_or(200).clamp(1, 1000);
        let script = format!(
            "set -e; P={quoted}; if [ ! -d \"$P\" ]; then printf '__TASK_RUNNER_DIR_NOT_FOUND__\\n'; else cd \"$P\"; find . -mindepth 1 -maxdepth 1 -printf '%P\\t%y\\t%s\\t%T@\\n'; fi",
            quoted = shell_quote(normalized_path.as_str()),
        );
        let output = run_ssh_command(
            &server,
            script.as_str(),
            Duration::from_secs(context.command_timeout_seconds),
        )
        .await?;
        if output.contains("__TASK_RUNNER_DIR_NOT_FOUND__") {
            return Err(format!("远程目录不存在: {normalized_path}"));
        }

        let mut entries = parse_directory_entries(normalized_path.as_str(), output.as_str());
        entries.sort_by(|left, right| match (left.is_dir, right.is_dir) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
        });
        let truncated = entries.len() > entry_limit;
        if truncated {
            entries.truncate(entry_limit);
        }
        touch_server(self, &server.id).await?;

        Ok(json!({
            "connection_id": server.id,
            "path": normalized_path,
            "count": entries.len(),
            "entries_truncated": truncated,
            "entries": entries,
        }))
    }

    async fn read_file(
        &self,
        context: RemoteConnectionControllerContext,
        connection_id: Option<String>,
        path: String,
        max_bytes: Option<usize>,
    ) -> Result<Value, String> {
        let connection_id = resolve_connection_id(&context, connection_id)?;
        let server = resolve_enabled_server(self, &connection_id).await?;
        let normalized_path = normalize_remote_path(path.as_str());
        let read_limit = max_bytes
            .unwrap_or(context.max_read_file_bytes)
            .clamp(1, context.max_read_file_bytes.max(1));
        let script = format!(
            "set -e; P={quoted}; if [ ! -f \"$P\" ]; then printf '__TASK_RUNNER_FILE_NOT_FOUND__\\n'; else SZ=$(wc -c < \"$P\" 2>/dev/null || echo 0); head -c {limit} \"$P\"; if [ \"$SZ\" -gt {limit} ]; then printf '\\n__TASK_RUNNER_FILE_TRUNCATED__ size=%s limit={limit}\\n' \"$SZ\"; fi; fi",
            quoted = shell_quote(normalized_path.as_str()),
            limit = read_limit,
        );
        let output = run_ssh_command(
            &server,
            script.as_str(),
            Duration::from_secs(context.command_timeout_seconds),
        )
        .await?;
        if output.contains("__TASK_RUNNER_FILE_NOT_FOUND__") {
            return Err(format!("远程文件不存在: {normalized_path}"));
        }
        let (content, truncated, source_size) = split_file_output(output);
        touch_server(self, &server.id).await?;

        Ok(json!({
            "connection_id": server.id,
            "path": normalized_path,
            "max_bytes": read_limit,
            "source_size_bytes": source_size,
            "truncated": truncated,
            "content": content,
        }))
    }

    async fn download_file(
        &self,
        context: RemoteConnectionControllerContext,
        connection_id: Option<String>,
        path: String,
        encoding: String,
        max_bytes: Option<usize>,
    ) -> Result<Value, String> {
        let connection_id = resolve_connection_id(&context, connection_id)?;
        let server = resolve_enabled_server(self, &connection_id).await?;
        let normalized_path = normalize_remote_path(path.as_str());
        let transfer_limit = max_bytes
            .unwrap_or(context.max_read_file_bytes)
            .clamp(1, context.max_read_file_bytes.max(1));
        let result = download_sftp_file(
            &server,
            normalized_path.as_str(),
            transfer_limit,
            Duration::from_secs(context.command_timeout_seconds),
        )
        .await?;
        let content_size_bytes = result.content.len();
        let content = match encoding.as_str() {
            "base64" => BASE64_STANDARD.encode(result.content.as_slice()),
            "text" => String::from_utf8(result.content).map_err(|_| {
                "远程文件不是有效 UTF-8 文本；请使用 encoding=\"base64\" 重新下载".to_string()
            })?,
            _ => return Err("encoding must be one of: text, base64".to_string()),
        };
        touch_server(self, &server.id).await?;

        Ok(json!({
            "connection_id": server.id,
            "path": normalized_path,
            "encoding": encoding,
            "max_bytes": transfer_limit,
            "source_size_bytes": result.source_size,
            "content_size_bytes": content_size_bytes,
            "truncated": result.truncated,
            "content": content,
        }))
    }

    async fn upload_file(
        &self,
        context: RemoteConnectionControllerContext,
        connection_id: Option<String>,
        path: String,
        content: String,
        encoding: String,
        create_parent_dirs: bool,
        overwrite: bool,
    ) -> Result<Value, String> {
        let connection_id = resolve_connection_id(&context, connection_id)?;
        let server = resolve_enabled_server(self, &connection_id).await?;
        let normalized_path = normalize_remote_path(path.as_str());
        let bytes = match encoding.as_str() {
            "base64" => BASE64_STANDARD
                .decode(content.as_bytes())
                .map_err(|err| format!("content 不是有效 base64: {err}"))?,
            "text" => content.into_bytes(),
            _ => return Err("encoding must be one of: text, base64".to_string()),
        };
        let max_upload_bytes = context.max_read_file_bytes.max(1);
        if bytes.len() > max_upload_bytes {
            return Err(format!(
                "上传内容超过限制: {} bytes > {} bytes",
                bytes.len(),
                max_upload_bytes
            ));
        }
        let bytes_written = upload_sftp_file(
            &server,
            normalized_path.as_str(),
            bytes,
            create_parent_dirs,
            overwrite,
            Duration::from_secs(context.command_timeout_seconds),
        )
        .await?;
        touch_server(self, &server.id).await?;

        Ok(json!({
            "connection_id": server.id,
            "path": normalized_path,
            "encoding": encoding,
            "bytes_written": bytes_written,
            "create_parent_dirs": create_parent_dirs,
            "overwrite": overwrite,
        }))
    }
}
