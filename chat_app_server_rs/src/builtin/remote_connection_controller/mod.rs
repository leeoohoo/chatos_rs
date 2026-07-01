// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod actions;
mod context;

use async_trait::async_trait;
use serde_json::Value;

use chatos_builtin_tools::{RemoteConnectionControllerContext, RemoteConnectionControllerStore};

use self::actions::{
    list_connections_with_context, list_directory_with_context, read_file_with_context,
    run_command_with_context, test_connection_with_context,
};

#[derive(Clone)]
pub(super) struct BoundContext {
    pub(super) user_id: Option<String>,
    pub(super) default_remote_connection_id: Option<String>,
    pub(super) command_timeout_seconds: u64,
    pub(super) max_command_timeout_seconds: u64,
    pub(super) max_output_chars: usize,
    pub(super) max_read_file_bytes: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ChatosRemoteConnectionControllerStore;

#[async_trait]
impl RemoteConnectionControllerStore for ChatosRemoteConnectionControllerStore {
    async fn list_connections(
        &self,
        context: RemoteConnectionControllerContext,
    ) -> Result<Value, String> {
        list_connections_with_context(bound_context(context)).await
    }

    async fn test_connection(
        &self,
        context: RemoteConnectionControllerContext,
        connection_id: Option<String>,
    ) -> Result<Value, String> {
        test_connection_with_context(bound_context(context), connection_id).await
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
        run_command_with_context(
            bound_context(context),
            connection_id,
            command,
            timeout_seconds,
            allow_dangerous,
            max_output_chars,
        )
        .await
    }

    async fn list_directory(
        &self,
        context: RemoteConnectionControllerContext,
        connection_id: Option<String>,
        path: Option<String>,
        limit: Option<usize>,
    ) -> Result<Value, String> {
        list_directory_with_context(bound_context(context), connection_id, path, limit).await
    }

    async fn read_file(
        &self,
        context: RemoteConnectionControllerContext,
        connection_id: Option<String>,
        path: String,
        max_bytes: Option<usize>,
    ) -> Result<Value, String> {
        read_file_with_context(bound_context(context), connection_id, path, max_bytes).await
    }
}

fn bound_context(context: RemoteConnectionControllerContext) -> BoundContext {
    BoundContext {
        user_id: context.user_id,
        default_remote_connection_id: context.default_remote_connection_id,
        command_timeout_seconds: context.command_timeout_seconds,
        max_command_timeout_seconds: context.max_command_timeout_seconds,
        max_output_chars: context.max_output_chars,
        max_read_file_bytes: context.max_read_file_bytes,
    }
}
