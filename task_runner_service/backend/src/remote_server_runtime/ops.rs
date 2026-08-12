// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use chatos_mcp::{RemoteConnectionControllerContext, RemoteConnectionControllerStore};
use serde_json::{json, Value};
use tokio::time::Duration;

use super::connector::{remote_sftp_request, run_remote_command, test_remote_server_connectivity};
use super::store_helpers::{
    persist_test_result, resolve_enabled_server, server_owned_by, touch_server,
};
use super::support::{
    command_danger_reason, normalize_remote_path, resolve_connection_id, truncate_text,
    ConnectionSummary,
};
use super::TaskRunnerRemoteConnectionStore;

#[async_trait]
impl RemoteConnectionControllerStore for TaskRunnerRemoteConnectionStore {
    async fn list_connections(
        &self,
        context: RemoteConnectionControllerContext,
    ) -> Result<Value, String> {
        let owner_user_id = owner_user_id(&context)?;
        let mut list = self
            .store
            .list_remote_servers()
            .await?
            .into_iter()
            .filter(|item| {
                item.enabled
                    && has_local_connector_binding(item)
                    && server_owned_by(item, owner_user_id)
            })
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
        let owner_user_id = owner_user_id(&context)?;
        let server = resolve_enabled_server(self, &connection_id, owner_user_id).await?;
        let response = match test_remote_server_connectivity(
            &self.config,
            owner_user_id,
            &server,
            Some(server.id.clone()),
        )
        .await
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
        let owner_user_id = owner_user_id(&context)?;
        let server = resolve_enabled_server(self, &connection_id, owner_user_id).await?;
        let timeout = timeout_seconds
            .unwrap_or(context.command_timeout_seconds)
            .clamp(1, context.max_command_timeout_seconds);
        let output_limit = max_output_chars
            .unwrap_or(context.max_output_chars)
            .clamp(1, context.max_output_chars.max(1));

        let output = run_remote_command(
            &self.config,
            owner_user_id,
            &server,
            command.as_str(),
            Duration::from_secs(timeout),
        )
        .await?;
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
        let owner_user_id = owner_user_id(&context)?;
        let server = resolve_enabled_server(self, &connection_id, owner_user_id).await?;
        let normalized_path = normalize_remote_path(
            path.as_deref()
                .filter(|value| !value.trim().is_empty())
                .or(server.default_remote_path.as_deref())
                .unwrap_or("."),
        );
        let entry_limit = limit.unwrap_or(200).clamp(1, 1000);
        let response = remote_sftp_request(
            &self.config,
            owner_user_id,
            &server,
            "list",
            json!({ "path": normalized_path }),
            Duration::from_secs(context.command_timeout_seconds),
        )
        .await?;
        let mut entries = response
            .get("entries")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let truncated = entries.len() > entry_limit;
        if truncated {
            entries.truncate(entry_limit);
        }
        touch_server(self, &server.id).await?;

        Ok(json!({
            "connection_id": server.id,
            "path": response.get("path").cloned().unwrap_or_else(|| json!(normalized_path)),
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
        let owner_user_id = owner_user_id(&context)?;
        let server = resolve_enabled_server(self, &connection_id, owner_user_id).await?;
        let normalized_path = normalize_remote_path(path.as_str());
        let read_limit = max_bytes
            .unwrap_or(context.max_read_file_bytes)
            .clamp(1, context.max_read_file_bytes.max(1));
        let response = remote_sftp_request(
            &self.config,
            owner_user_id,
            &server,
            "read_file",
            json!({ "remote_path": normalized_path, "max_bytes": read_limit }),
            Duration::from_secs(context.command_timeout_seconds),
        )
        .await?;
        let bytes = decode_sftp_content(&response)?;
        let content = String::from_utf8(bytes).map_err(|_| {
            "远程文件不是有效 UTF-8 文本；请使用 download_file encoding=\"base64\"".to_string()
        })?;
        touch_server(self, &server.id).await?;

        Ok(json!({
            "connection_id": server.id,
            "path": normalized_path,
            "max_bytes": read_limit,
            "source_size_bytes": response.get("source_size").cloned().unwrap_or(Value::Null),
            "truncated": response.get("truncated").cloned().unwrap_or(json!(false)),
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
        let owner_user_id = owner_user_id(&context)?;
        let server = resolve_enabled_server(self, &connection_id, owner_user_id).await?;
        let normalized_path = normalize_remote_path(path.as_str());
        let transfer_limit = max_bytes
            .unwrap_or(context.max_read_file_bytes)
            .clamp(1, context.max_read_file_bytes.max(1));
        let result = remote_sftp_request(
            &self.config,
            owner_user_id,
            &server,
            "read_file",
            json!({ "remote_path": normalized_path, "max_bytes": transfer_limit }),
            Duration::from_secs(context.command_timeout_seconds),
        )
        .await?;
        let bytes = decode_sftp_content(&result)?;
        let content_size_bytes = bytes.len();
        let content = match encoding.as_str() {
            "base64" => BASE64_STANDARD.encode(bytes.as_slice()),
            "text" => String::from_utf8(bytes).map_err(|_| {
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
            "source_size_bytes": result.get("source_size").cloned().unwrap_or(Value::Null),
            "content_size_bytes": content_size_bytes,
            "truncated": result.get("truncated").cloned().unwrap_or(json!(false)),
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
        let owner_user_id = owner_user_id(&context)?;
        let server = resolve_enabled_server(self, &connection_id, owner_user_id).await?;
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
        let result = remote_sftp_request(
            &self.config,
            owner_user_id,
            &server,
            "write_file",
            json!({
                "remote_path": normalized_path,
                "content_base64": BASE64_STANDARD.encode(bytes),
                "create_parent_dirs": create_parent_dirs,
                "overwrite": overwrite,
            }),
            Duration::from_secs(context.command_timeout_seconds),
        )
        .await?;
        let bytes_written = result
            .get("bytes_written")
            .and_then(Value::as_u64)
            .ok_or_else(|| "Local Connector SFTP response is missing bytes_written".to_string())?;
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

fn owner_user_id(context: &RemoteConnectionControllerContext) -> Result<&str, String> {
    context
        .user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "remote connection controller is missing owner user context".to_string())
}

fn decode_sftp_content(response: &Value) -> Result<Vec<u8>, String> {
    BASE64_STANDARD
        .decode(
            response
                .get("content_base64")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    "Local Connector SFTP response is missing content_base64".to_string()
                })?,
        )
        .map_err(|error| format!("decode Local Connector SFTP content failed: {error}"))
}

fn has_local_connector_binding(server: &crate::models::RemoteServerRecord) -> bool {
    server
        .local_connector_device_id
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        && server
            .local_connector_workspace_id
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
}
