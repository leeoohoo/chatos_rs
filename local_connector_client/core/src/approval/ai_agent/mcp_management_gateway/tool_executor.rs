// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chatos_ai_runtime::{McpRuntimeToolExecutor, ToolExecutor};
use chatos_mcp_runtime::{McpExecutor, ToolCallContext, ToolResult, ToolResultCallback};
use serde_json::Value;

use super::super::super::decision_tool::{
    approval_decision_from_tool_result, ApprovalToolDecision, APPROVAL_DECISION_TOOL,
};

pub(super) const APPROVAL_AGGREGATED_TOOL: &str = "local_connector_approval_approval_decision";
const CODE_READ_NAMESPACE: &str = "code_maintainer_read_";

#[derive(Clone)]
pub(in super::super) struct ApprovalMcpGatewayToolExecutor {
    inner: McpRuntimeToolExecutor,
    available_tools: Vec<Value>,
    public_to_aggregated: Arc<HashMap<String, String>>,
    aggregated_to_public: Arc<HashMap<String, String>>,
    decision: Arc<Mutex<Option<ApprovalToolDecision>>>,
}

impl ApprovalMcpGatewayToolExecutor {
    pub(super) fn new(
        executor: McpExecutor,
        decision: Arc<Mutex<Option<ApprovalToolDecision>>>,
    ) -> Result<Self> {
        let inner = McpRuntimeToolExecutor::new(executor);
        let mut available_tools = Vec::new();
        let mut public_to_aggregated = HashMap::new();
        let mut aggregated_to_public = HashMap::new();
        let mut found_approval_decision = false;
        for mut tool in inner.available_tools() {
            let Some(aggregated_name) = tool
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
            else {
                continue;
            };
            let Some(public_name) = approval_public_tool_name(aggregated_name.as_str()) else {
                continue;
            };
            found_approval_decision |= public_name == APPROVAL_DECISION_TOOL;
            if public_to_aggregated.contains_key(public_name.as_str()) {
                return Err(anyhow!(
                    "command approval MCP tools contain a public name collision: {public_name}"
                ));
            }
            tool["name"] = Value::String(public_name.clone());
            public_to_aggregated.insert(public_name.clone(), aggregated_name.clone());
            aggregated_to_public.insert(aggregated_name, public_name);
            available_tools.push(tool);
        }
        if !found_approval_decision {
            return Err(anyhow!(
                "command approval MCP tools do not expose approval_decision"
            ));
        }
        Ok(Self {
            inner,
            available_tools,
            public_to_aggregated: Arc::new(public_to_aggregated),
            aggregated_to_public: Arc::new(aggregated_to_public),
            decision,
        })
    }

    fn process_result(&self, result: &mut ToolResult) {
        let aggregated_name = result.name.clone();
        let public_name = self
            .aggregated_to_public
            .get(aggregated_name.as_str())
            .cloned()
            .unwrap_or_else(|| aggregated_name.clone());
        if is_approval_aggregated_tool(aggregated_name.as_str()) && result.success {
            let parsed = result
                .result
                .as_ref()
                .ok_or_else(|| "approval_decision result is missing".to_string())
                .and_then(approval_decision_from_tool_result);
            match parsed {
                Ok(decision) => match self.decision.lock() {
                    Ok(mut guard) if guard.is_none() => *guard = Some(decision),
                    Ok(_) => mark_result_error(
                        result,
                        "approval_decision has already been called for this request",
                    ),
                    Err(_) => mark_result_error(result, "approval_decision state is unavailable"),
                },
                Err(error) => mark_result_error(result, error.as_str()),
            }
        }
        result.name = public_name;
    }
}

#[async_trait]
impl ToolExecutor for ApprovalMcpGatewayToolExecutor {
    fn available_tools(&self) -> Vec<Value> {
        self.available_tools.clone()
    }

    async fn execute_tools_stream(
        &self,
        tool_calls: &[Value],
        context: ToolCallContext,
        on_tool_result: Option<ToolResultCallback>,
    ) -> Vec<ToolResult> {
        let translated = tool_calls
            .iter()
            .map(|tool_call| {
                let Some(public_name) =
                    chatos_ai_runtime::tool_call::extract_tool_call_name(tool_call)
                else {
                    return tool_call.clone();
                };
                let Some(aggregated_name) = self.public_to_aggregated.get(public_name) else {
                    return tool_call.clone();
                };
                rename_tool_call(tool_call, aggregated_name.as_str())
            })
            .collect::<Vec<_>>();
        let mut results = self
            .inner
            .execute_tools_stream(translated.as_slice(), context, None)
            .await;
        for result in &mut results {
            self.process_result(result);
            if let Some(callback) = on_tool_result.as_ref() {
                callback(result);
            }
        }
        results
    }
}

pub(super) fn approval_public_tool_name(aggregated_name: &str) -> Option<String> {
    if is_approval_aggregated_tool(aggregated_name) {
        return Some(APPROVAL_DECISION_TOOL.to_string());
    }
    if let Some(original_name) = aggregated_name.strip_prefix(CODE_READ_NAMESPACE) {
        return matches!(
            original_name,
            "read_file_raw" | "read_file_range" | "list_dir" | "search_text"
        )
        .then(|| original_name.to_string());
    }
    Some(aggregated_name.to_string())
}

fn is_approval_aggregated_tool(name: &str) -> bool {
    matches!(
        name,
        APPROVAL_DECISION_TOOL
            | APPROVAL_AGGREGATED_TOOL
            | "local_connector_approval__approval_decision"
    )
}

pub(super) fn rename_tool_call(tool_call: &Value, name: &str) -> Value {
    let mut translated = tool_call.clone();
    if let Some(function) = translated
        .get_mut("function")
        .and_then(Value::as_object_mut)
    {
        function.insert("name".to_string(), Value::String(name.to_string()));
    } else if let Some(object) = translated.as_object_mut() {
        object.insert("name".to_string(), Value::String(name.to_string()));
    }
    translated
}

fn mark_result_error(result: &mut ToolResult, message: &str) {
    result.success = false;
    result.is_error = true;
    result.content = message.to_string();
    result.result = None;
}
