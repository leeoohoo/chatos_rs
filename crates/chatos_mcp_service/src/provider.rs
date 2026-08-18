// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::catalog::{contains_tool_name, sort_tools_by_name, tool_name};

pub const TOOL_RESULT_MAX_CHARS_META_KEY: &str = "chatos/toolResultMaxChars";
pub const TOOL_RESULT_MAX_CHARS_UPPER_BOUND: usize = 10_000_000;

#[derive(Debug, Clone, Default)]
pub struct McpRequestContext {
    pub metadata: BTreeMap<String, String>,
}

impl McpRequestContext {
    pub fn with_tool_call_params(mut self, params: &Value) -> Self {
        if let Some(max_chars) = tool_result_max_chars_from_params(params) {
            self.metadata.insert(
                TOOL_RESULT_MAX_CHARS_META_KEY.to_string(),
                max_chars.to_string(),
            );
        }
        self
    }

    pub fn tool_result_max_chars(&self) -> Option<usize> {
        self.metadata
            .get(TOOL_RESULT_MAX_CHARS_META_KEY)
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| (1..=TOOL_RESULT_MAX_CHARS_UPPER_BOUND).contains(value))
    }
}

pub fn tool_result_max_chars_from_params(params: &Value) -> Option<usize> {
    params
        .get("_meta")
        .and_then(|value| value.get(TOOL_RESULT_MAX_CHARS_META_KEY))
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| (1..=TOOL_RESULT_MAX_CHARS_UPPER_BOUND).contains(value))
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
