// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use serde_json::Value;

use crate::tool_registry::ToolRegistry;

mod args;
#[cfg(test)]
mod tests;
mod tools;
mod types;

pub use types::{
    RemoteConnectionControllerContext, RemoteConnectionControllerOptions,
    RemoteConnectionControllerStore, RemoteConnectionControllerStoreRef,
    DEFAULT_COMMAND_TIMEOUT_SECONDS, DEFAULT_MAX_OUTPUT_CHARS, DEFAULT_MAX_READ_FILE_BYTES,
    MAX_COMMAND_TIMEOUT_SECONDS,
};

type ToolHandler = Arc<dyn Fn(Value) -> Result<Value, String> + Send + Sync>;

#[derive(Clone)]
pub struct RemoteConnectionControllerService {
    registry: ToolRegistry<ToolHandler>,
}

impl RemoteConnectionControllerService {
    pub fn new(opts: RemoteConnectionControllerOptions) -> Result<Self, String> {
        let mut service = Self {
            registry: ToolRegistry::new(),
        };
        let bound = RemoteConnectionControllerContext {
            server_name: opts.server_name,
            user_id: opts
                .user_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned),
            default_remote_connection_id: opts
                .default_remote_connection_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned),
            command_timeout_seconds: opts
                .command_timeout_seconds
                .clamp(1, MAX_COMMAND_TIMEOUT_SECONDS)
                .max(DEFAULT_COMMAND_TIMEOUT_SECONDS),
            max_command_timeout_seconds: opts
                .max_command_timeout_seconds
                .max(MAX_COMMAND_TIMEOUT_SECONDS),
            max_output_chars: opts.max_output_chars.max(DEFAULT_MAX_OUTPUT_CHARS),
            max_read_file_bytes: opts.max_read_file_bytes.max(DEFAULT_MAX_READ_FILE_BYTES),
        };

        if bound.user_id.is_none() {
            let reason = "remote_connection_controller 缺少 user_id 上下文".to_string();
            service.registry.register_unavailable_tools(
                [
                    "list_connections",
                    "test_connection",
                    "run_command",
                    "list_directory",
                    "read_file",
                    "download_file",
                    "upload_file",
                ],
                reason,
            );
            return Ok(service);
        }

        let require_connection_id = bound.default_remote_connection_id.is_none();
        service.register_list_connections(bound.clone(), opts.store.clone());
        service.register_test_connection(bound.clone(), opts.store.clone(), require_connection_id);
        service.register_run_command(bound.clone(), opts.store.clone(), require_connection_id);
        service.register_list_directory(bound.clone(), opts.store.clone(), require_connection_id);
        service.register_read_file(bound.clone(), opts.store.clone(), require_connection_id);
        service.register_download_file(bound.clone(), opts.store.clone(), require_connection_id);
        service.register_upload_file(bound, opts.store, require_connection_id);
        Ok(service)
    }

    pub fn list_tools(&self) -> Vec<Value> {
        self.registry.list_tools()
    }

    pub fn call_tool(&self, name: &str, args: Value) -> Result<Value, String> {
        let tool = self
            .registry
            .get(name)
            .ok_or_else(|| format!("Tool not found: {name}"))?;
        (tool.handler)(args)
    }

    pub fn unavailable_tools(&self) -> Vec<(String, String)> {
        self.registry.unavailable_tools()
    }

    pub(super) fn register_tool(
        &mut self,
        name: &str,
        description: &str,
        input_schema: Value,
        handler: ToolHandler,
    ) {
        self.registry
            .register_tool(name, description, input_schema, handler);
    }
}
