// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::catalog::{contains_tool_name, sort_tools_by_name, tool_name};

#[derive(Debug, Clone, Default)]
pub struct McpRequestContext {
    pub metadata: BTreeMap<String, String>,
}

#[async_trait]
pub trait McpToolProvider: Send + Sync {
    fn server_name(&self) -> &str;

    fn list_tools(&self, context: &McpRequestContext) -> Vec<Value>;

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        context: McpRequestContext,
    ) -> Result<Value, String>;

    fn unavailable_tools(&self, _context: &McpRequestContext) -> Vec<(String, String)> {
        Vec::new()
    }
}

#[derive(Clone)]
pub struct CompositeToolProvider {
    server_name: String,
    providers: Vec<Arc<dyn McpToolProvider>>,
}

impl CompositeToolProvider {
    pub fn new(server_name: impl Into<String>, providers: Vec<Arc<dyn McpToolProvider>>) -> Self {
        Self {
            server_name: server_name.into(),
            providers,
        }
    }
}

#[async_trait]
impl McpToolProvider for CompositeToolProvider {
    fn server_name(&self) -> &str {
        self.server_name.as_str()
    }

    fn list_tools(&self, context: &McpRequestContext) -> Vec<Value> {
        let mut seen = HashSet::new();
        let mut tools = Vec::new();
        for provider in &self.providers {
            for tool in provider.list_tools(context) {
                let Some(name) = tool_name(&tool) else {
                    continue;
                };
                if seen.insert(name.to_string()) {
                    tools.push(tool);
                }
            }
        }
        sort_tools_by_name(tools)
    }

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        context: McpRequestContext,
    ) -> Result<Value, String> {
        for provider in &self.providers {
            let has_tool = contains_tool_name(&provider.list_tools(&context), name);
            if has_tool {
                return provider.call_tool(name, args, context).await;
            }
        }
        Err(format!("tool not found: {name}"))
    }
}
