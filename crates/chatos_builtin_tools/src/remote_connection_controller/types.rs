// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

pub const DEFAULT_COMMAND_TIMEOUT_SECONDS: u64 = 20;
pub const MAX_COMMAND_TIMEOUT_SECONDS: u64 = 120;
pub const DEFAULT_MAX_OUTPUT_CHARS: usize = 20_000;
pub const DEFAULT_MAX_READ_FILE_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone)]
pub struct RemoteConnectionControllerOptions {
    pub server_name: String,
    pub user_id: Option<String>,
    pub default_remote_connection_id: Option<String>,
    pub command_timeout_seconds: u64,
    pub max_command_timeout_seconds: u64,
    pub max_output_chars: usize,
    pub max_read_file_bytes: usize,
    pub store: RemoteConnectionControllerStoreRef,
}

#[derive(Debug, Clone)]
pub struct RemoteConnectionControllerContext {
    pub server_name: String,
    pub user_id: Option<String>,
    pub default_remote_connection_id: Option<String>,
    pub command_timeout_seconds: u64,
    pub max_command_timeout_seconds: u64,
    pub max_output_chars: usize,
    pub max_read_file_bytes: usize,
}

#[async_trait]
pub trait RemoteConnectionControllerStore: Send + Sync {
    async fn list_connections(
        &self,
        context: RemoteConnectionControllerContext,
    ) -> Result<Value, String>;

    async fn test_connection(
        &self,
        context: RemoteConnectionControllerContext,
        connection_id: Option<String>,
    ) -> Result<Value, String>;

    async fn run_command(
        &self,
        context: RemoteConnectionControllerContext,
        connection_id: Option<String>,
        command: String,
        timeout_seconds: Option<u64>,
        allow_dangerous: bool,
        max_output_chars: Option<usize>,
    ) -> Result<Value, String>;

    async fn list_directory(
        &self,
        context: RemoteConnectionControllerContext,
        connection_id: Option<String>,
        path: Option<String>,
        limit: Option<usize>,
    ) -> Result<Value, String>;

    async fn read_file(
        &self,
        context: RemoteConnectionControllerContext,
        connection_id: Option<String>,
        path: String,
        max_bytes: Option<usize>,
    ) -> Result<Value, String>;

    async fn download_file(
        &self,
        context: RemoteConnectionControllerContext,
        connection_id: Option<String>,
        path: String,
        encoding: String,
        max_bytes: Option<usize>,
    ) -> Result<Value, String>;

    async fn upload_file(
        &self,
        context: RemoteConnectionControllerContext,
        connection_id: Option<String>,
        path: String,
        content: String,
        encoding: String,
        create_parent_dirs: bool,
        overwrite: bool,
    ) -> Result<Value, String>;
}

#[derive(Clone)]
pub struct RemoteConnectionControllerStoreRef(Arc<dyn RemoteConnectionControllerStore>);

impl RemoteConnectionControllerStoreRef {
    pub fn new(store: Arc<dyn RemoteConnectionControllerStore>) -> Self {
        Self(store)
    }

    pub(super) fn inner(&self) -> Arc<dyn RemoteConnectionControllerStore> {
        self.0.clone()
    }
}

impl std::fmt::Debug for RemoteConnectionControllerStoreRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteConnectionControllerStoreRef")
            .finish_non_exhaustive()
    }
}
