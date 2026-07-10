// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::json;

use crate::tool_registry::async_text_tool_handler;

use super::args::{
    optional_bool, optional_bool_with_default, optional_encoding, optional_trimmed_string,
    optional_u64, optional_usize, required_string, required_trimmed_string,
};
use super::{
    RemoteConnectionControllerContext, RemoteConnectionControllerService,
    RemoteConnectionControllerStoreRef,
};

impl RemoteConnectionControllerService {
    pub(super) fn register_list_connections(
        &mut self,
        bound: RemoteConnectionControllerContext,
        store: RemoteConnectionControllerStoreRef,
    ) {
        self.register_tool(
            "list_connections",
            "List current user's available remote SSH/SFTP connections (sensitive fields are masked). Use this tool family for remote hosts, not local terminal execution. If no suitable connection or credentials are available, ask the user to choose/create/update a Task Runner remote-server connection before treating the remote task as complete.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            async_text_tool_handler(move |_args| {
                let ctx = bound.clone();
                let store = store.inner();
                Ok(async move { store.list_connections(ctx).await })
            }),
        );
    }

    pub(super) fn register_test_connection(
        &mut self,
        bound: RemoteConnectionControllerContext,
        store: RemoteConnectionControllerStoreRef,
        require_connection_id: bool,
    ) {
        let required = if require_connection_id {
            json!(["connection_id"])
        } else {
            json!([])
        };
        let description = if require_connection_id {
            "Test SSH connectivity for a remote connection. connection_id is required because no default connection is bound. If authentication fails or credentials are missing, ask the user for the needed connection/config update instead of downgrading to public probing."
        } else {
            "Test SSH connectivity for a remote connection. If connection_id is omitted, use default bound connection from chat runtime. If authentication fails or credentials are missing, ask the user for the needed connection/config update instead of downgrading to public probing."
        };
        self.register_tool(
            "test_connection",
            description,
            json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" }
                },
                "required": required,
                "additionalProperties": false
            }),
            async_text_tool_handler(move |args| {
                let connection_id = optional_trimmed_string(&args, "connection_id");
                let ctx = bound.clone();
                let store = store.inner();
                Ok(async move { store.test_connection(ctx, connection_id).await })
            }),
        );
    }

    pub(super) fn register_run_command(
        &mut self,
        bound: RemoteConnectionControllerContext,
        store: RemoteConnectionControllerStoreRef,
        require_connection_id: bool,
    ) {
        let required = if require_connection_id {
            json!(["connection_id", "command"])
        } else {
            json!(["command"])
        };
        let description = if require_connection_id {
            "Run one SSH command on a remote host. connection_id is required because no default connection is bound. Returns exit_code/stdout/stderr/truncated flags. Dangerous commands are blocked by default unless allow_dangerous=true. If remote authentication or connection details are missing, ask the user to supply/update them before claiming the remote inspection is complete."
        } else {
            "Run one SSH command on a remote host (preferred for all server-side checks/ops). Returns structured result including exit_code/stdout/stderr/truncated flags. Dangerous commands are blocked by default unless allow_dangerous=true. If remote authentication or connection details are missing, ask the user to supply/update them before claiming the remote inspection is complete."
        };
        self.register_tool(
            "run_command",
            description,
            json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "command": { "type": "string" },
                    "timeout_seconds": { "type": "integer", "minimum": 1, "maximum": 120 },
                    "allow_dangerous": { "type": "boolean" },
                    "max_output_chars": { "type": "integer", "minimum": 128, "maximum": 20000 }
                },
                "required": required,
                "additionalProperties": false
            }),
            async_text_tool_handler(move |args| {
                let connection_id = optional_trimmed_string(&args, "connection_id");
                let command = required_trimmed_string(&args, "command")?;
                let timeout_seconds = optional_u64(&args, "timeout_seconds");
                let allow_dangerous = optional_bool(&args, "allow_dangerous");
                let max_output_chars = optional_usize(&args, "max_output_chars");
                let ctx = bound.clone();
                let store = store.inner();
                Ok(async move {
                    store
                        .run_command(
                            ctx,
                            connection_id,
                            command,
                            timeout_seconds,
                            allow_dangerous,
                            max_output_chars,
                        )
                        .await
                })
            }),
        );
    }

    pub(super) fn register_list_directory(
        &mut self,
        bound: RemoteConnectionControllerContext,
        store: RemoteConnectionControllerStoreRef,
        require_connection_id: bool,
    ) {
        let required = if require_connection_id {
            json!(["connection_id"])
        } else {
            json!([])
        };
        let description = if require_connection_id {
            "List entries under a remote directory path. connection_id is required because no default connection is bound."
        } else {
            "List entries under a remote directory path on the bound SSH host."
        };
        self.register_tool(
            "list_directory",
            description,
            json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "path": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 1000 }
                },
                "required": required,
                "additionalProperties": false
            }),
            async_text_tool_handler(move |args| {
                let connection_id = optional_trimmed_string(&args, "connection_id");
                let path = optional_trimmed_string(&args, "path");
                let limit = optional_usize(&args, "limit");
                let ctx = bound.clone();
                let store = store.inner();
                Ok(async move { store.list_directory(ctx, connection_id, path, limit).await })
            }),
        );
    }

    pub(super) fn register_read_file(
        &mut self,
        bound: RemoteConnectionControllerContext,
        store: RemoteConnectionControllerStoreRef,
        require_connection_id: bool,
    ) {
        let server_name = bound.server_name.clone();
        let required = if require_connection_id {
            json!(["connection_id", "path"])
        } else {
            json!(["path"])
        };
        let description = if require_connection_id {
            format!(
                "Read remote file content (up to size limit) on SSH server {}. connection_id is required because no default connection is bound.",
                server_name
            )
        } else {
            format!(
                "Read remote file content (up to size limit) on bound SSH server {}.",
                server_name
            )
        };
        self.register_tool(
            "read_file",
            &description,
            json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "path": { "type": "string" },
                    "max_bytes": { "type": "integer", "minimum": 1, "maximum": 262144 }
                },
                "required": required,
                "additionalProperties": false
            }),
            async_text_tool_handler(move |args| {
                let connection_id = optional_trimmed_string(&args, "connection_id");
                let path = required_trimmed_string(&args, "path")?;
                let max_bytes = optional_usize(&args, "max_bytes");
                let ctx = bound.clone();
                let store = store.inner();
                Ok(async move { store.read_file(ctx, connection_id, path, max_bytes).await })
            }),
        );
    }

    pub(super) fn register_download_file(
        &mut self,
        bound: RemoteConnectionControllerContext,
        store: RemoteConnectionControllerStoreRef,
        require_connection_id: bool,
    ) {
        let server_name = bound.server_name.clone();
        let required = if require_connection_id {
            json!(["connection_id", "path"])
        } else {
            json!(["path"])
        };
        let description = if require_connection_id {
            format!(
                "Download a remote file through SFTP on SSH server {}. connection_id is required because no default connection is bound. Returns content as UTF-8 text by default; use encoding=base64 for binary files.",
                server_name
            )
        } else {
            format!(
                "Download a remote file through SFTP on bound SSH server {}. Returns content as UTF-8 text by default; use encoding=base64 for binary files.",
                server_name
            )
        };
        self.register_tool(
            "download_file",
            &description,
            json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "path": { "type": "string" },
                    "encoding": { "type": "string", "enum": ["text", "base64"] },
                    "max_bytes": { "type": "integer", "minimum": 1, "maximum": 262144 }
                },
                "required": required,
                "additionalProperties": false
            }),
            async_text_tool_handler(move |args| {
                let connection_id = optional_trimmed_string(&args, "connection_id");
                let path = required_trimmed_string(&args, "path")?;
                let encoding = optional_encoding(&args, "encoding", "text")?;
                let max_bytes = optional_usize(&args, "max_bytes");
                let ctx = bound.clone();
                let store = store.inner();
                Ok(async move {
                    store
                        .download_file(ctx, connection_id, path, encoding, max_bytes)
                        .await
                })
            }),
        );
    }

    pub(super) fn register_upload_file(
        &mut self,
        bound: RemoteConnectionControllerContext,
        store: RemoteConnectionControllerStoreRef,
        require_connection_id: bool,
    ) {
        let server_name = bound.server_name.clone();
        let required = if require_connection_id {
            json!(["connection_id", "path", "content"])
        } else {
            json!(["path", "content"])
        };
        let description = if require_connection_id {
            format!(
                "Upload content to a remote file through SFTP on SSH server {}. connection_id is required because no default connection is bound. Use encoding=base64 for binary content.",
                server_name
            )
        } else {
            format!(
                "Upload content to a remote file through SFTP on bound SSH server {}. Use encoding=base64 for binary content.",
                server_name
            )
        };
        self.register_tool(
            "upload_file",
            &description,
            json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "path": { "type": "string" },
                    "content": { "type": "string" },
                    "encoding": { "type": "string", "enum": ["text", "base64"] },
                    "create_parent_dirs": { "type": "boolean" },
                    "overwrite": { "type": "boolean" }
                },
                "required": required,
                "additionalProperties": false
            }),
            async_text_tool_handler(move |args| {
                let connection_id = optional_trimmed_string(&args, "connection_id");
                let path = required_trimmed_string(&args, "path")?;
                let content = required_string(&args, "content")?;
                let encoding = optional_encoding(&args, "encoding", "text")?;
                let create_parent_dirs =
                    optional_bool_with_default(&args, "create_parent_dirs", true);
                let overwrite = optional_bool_with_default(&args, "overwrite", true);
                let ctx = bound.clone();
                let store = store.inner();
                Ok(async move {
                    store
                        .upload_file(
                            ctx,
                            connection_id,
                            path,
                            content,
                            encoding,
                            create_parent_dirs,
                            overwrite,
                        )
                        .await
                })
            }),
        );
    }
}
