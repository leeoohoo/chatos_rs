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
    pub(super) fn register_test_connection(
        &mut self,
        bound: RemoteConnectionControllerContext,
        store: RemoteConnectionControllerStoreRef,
    ) {
        self.register_tool(
            "test_connection",
            "Test SSH connectivity for the selected remote connection.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            async_text_tool_handler(move |_args| {
                let ctx = bound.clone();
                let store = store.inner();
                Ok(async move { store.test_connection(ctx).await })
            }),
        );
    }

    pub(super) fn register_run_command(
        &mut self,
        bound: RemoteConnectionControllerContext,
        store: RemoteConnectionControllerStoreRef,
    ) {
        let max_output_chars_limit = bound.max_output_chars;
        self.register_tool(
            "run_command",
            "Run one SSH command on the selected remote host. Dangerous commands require allow_dangerous=true.",
            json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" },
                    "timeout_seconds": { "type": "integer", "minimum": 1, "maximum": 120 },
                    "allow_dangerous": { "type": "boolean" },
                    "max_output_chars": { "type": "integer", "minimum": 1, "maximum": max_output_chars_limit }
                },
                "required": ["command"],
                "additionalProperties": false
            }),
            async_text_tool_handler(move |args| {
                let command = required_trimmed_string(&args, "command")?;
                let timeout_seconds = optional_u64(&args, "timeout_seconds");
                let allow_dangerous = optional_bool(&args, "allow_dangerous");
                let max_output_chars = optional_usize(&args, "max_output_chars");
                let ctx = bound.clone();
                let store = store.inner();
                Ok(async move {
                    store
                        .run_command(ctx, command, timeout_seconds, allow_dangerous, max_output_chars)
                        .await
                })
            }),
        );
    }

    pub(super) fn register_list_directory(
        &mut self,
        bound: RemoteConnectionControllerContext,
        store: RemoteConnectionControllerStoreRef,
    ) {
        self.register_tool(
            "list_directory",
            "List entries under a directory on the selected remote host.",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 1000 }
                },
                "additionalProperties": false
            }),
            async_text_tool_handler(move |args| {
                let path = optional_trimmed_string(&args, "path");
                let limit = optional_usize(&args, "limit");
                let ctx = bound.clone();
                let store = store.inner();
                Ok(async move { store.list_directory(ctx, path, limit).await })
            }),
        );
    }

    pub(super) fn register_read_file(
        &mut self,
        bound: RemoteConnectionControllerContext,
        store: RemoteConnectionControllerStoreRef,
    ) {
        let server_name = bound.server_name.clone();
        let description = format!(
            "Read UTF-8 text file content on selected SSH server {}.",
            server_name
        );
        self.register_tool(
            "read_file",
            &description,
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "max_bytes": { "type": "integer", "minimum": 1, "maximum": 262144 }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
            async_text_tool_handler(move |args| {
                let path = required_trimmed_string(&args, "path")?;
                let max_bytes = optional_usize(&args, "max_bytes");
                let ctx = bound.clone();
                let store = store.inner();
                Ok(async move { store.read_file(ctx, path, max_bytes).await })
            }),
        );
    }

    pub(super) fn register_download_file(
        &mut self,
        bound: RemoteConnectionControllerContext,
        store: RemoteConnectionControllerStoreRef,
    ) {
        let server_name = bound.server_name.clone();
        let description = format!(
            "Download file content from selected SSH server {}. Use encoding=base64 for binary files.",
            server_name
        );
        self.register_tool(
            "download_file",
            &description,
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "encoding": { "type": "string", "enum": ["text", "base64"] },
                    "max_bytes": { "type": "integer", "minimum": 1, "maximum": 262144 }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
            async_text_tool_handler(move |args| {
                let path = required_trimmed_string(&args, "path")?;
                let encoding = optional_encoding(&args, "encoding", "text")?;
                let max_bytes = optional_usize(&args, "max_bytes");
                let ctx = bound.clone();
                let store = store.inner();
                Ok(async move { store.download_file(ctx, path, encoding, max_bytes).await })
            }),
        );
    }

    pub(super) fn register_upload_file(
        &mut self,
        bound: RemoteConnectionControllerContext,
        store: RemoteConnectionControllerStoreRef,
    ) {
        let server_name = bound.server_name.clone();
        let description = format!(
            "Upload content to a file on selected SSH server {}. Use encoding=base64 for binary content.",
            server_name
        );
        self.register_tool(
            "upload_file",
            &description,
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" },
                    "encoding": { "type": "string", "enum": ["text", "base64"] },
                    "create_parent_dirs": { "type": "boolean" },
                    "overwrite": { "type": "boolean" }
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
            async_text_tool_handler(move |args| {
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
                        .upload_file(ctx, path, content, encoding, create_parent_dirs, overwrite)
                        .await
                })
            }),
        );
    }
}
