// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Project-scoped local MCP execution owned by the native clients.
//!
//! Only stdio MCP servers whose executable and environment have already been
//! resolved by the native Host can enter this boundary. There is deliberately
//! no remote HTTP transport, service authentication, queue transport, builtin
//! server catalog, or server-side tool routing here.

mod schema;
mod stdio;

use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use async_trait::async_trait;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

pub use stdio::StdioMcpExecutor;

#[derive(Clone, PartialEq, Eq)]
pub struct LocalMcpServerConfig {
    pub name: String,
    pub executable: PathBuf,
    pub arguments: Vec<String>,
    pub working_directory: PathBuf,
    pub environment: BTreeMap<String, String>,
}

impl std::fmt::Debug for LocalMcpServerConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalMcpServerConfig")
            .field("name", &self.name)
            .field("executable", &self.executable)
            .field("arguments", &self.arguments)
            .field("working_directory", &self.working_directory)
            .field("environment_names", &self.environment.keys())
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LocalMcpToolCall {
    pub tool_call_id: String,
    pub tool_name: String,
    pub arguments: Value,
    pub run_id: String,
    pub turn_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LocalMcpToolResult {
    pub content: String,
    pub structured_result: Option<Value>,
    pub is_error: bool,
    pub fatal_error: bool,
}

#[async_trait]
pub trait LocalMcpExecutor: Send + Sync {
    fn available_tools(&self) -> Vec<Value>;

    async fn execute_tool(
        &self,
        call: LocalMcpToolCall,
        cancellation: CancellationToken,
    ) -> Result<LocalMcpToolResult, String>;
}

pub type SharedLocalMcpExecutor = Arc<dyn LocalMcpExecutor>;
