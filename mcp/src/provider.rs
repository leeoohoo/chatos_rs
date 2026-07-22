// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use chatos_mcp_runtime::{BuiltinToolProvider, McpBuiltinServer, McpHttpServer};
use chatos_plugin_management_sdk::SystemMcpKey;

use crate::SystemMcpHost;

#[derive(Debug, Clone, Default)]
pub struct SystemMcpResolveContext {
    pub workspace_dir: Option<String>,
    pub owner_user_id: Option<String>,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub headers: BTreeMap<String, String>,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Clone)]
pub enum ResolvedSystemMcpBackend {
    Embedded {
        server: McpBuiltinServer,
        provider: Option<Arc<dyn BuiltinToolProvider>>,
    },
    Http(McpHttpServer),
    Unavailable(String),
}

#[async_trait]
pub trait SystemMcpHostAdapter: Send + Sync {
    fn host(&self) -> SystemMcpHost;

    async fn resolve(
        &self,
        key: SystemMcpKey,
        context: &SystemMcpResolveContext,
    ) -> Result<ResolvedSystemMcpBackend, String>;
}
