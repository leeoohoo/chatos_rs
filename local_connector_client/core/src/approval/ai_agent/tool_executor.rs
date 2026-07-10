// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use chatos_ai_runtime::ToolExecutor;
use chatos_mcp_runtime::{
    build_function_tool_schema, parse_tool_definition, to_text_and_structured_result,
    ToolCallContext, ToolResult, ToolResultCallback,
};

use super::APPROVAL_DECISION_TOOL;

#[derive(Debug, Clone)]
pub(super) struct ApprovalToolDecision {
    pub(super) decision: String,
    pub(super) reason: String,
    pub(super) remember_allow: bool,
}

#[derive(Clone)]
pub(super) struct ApprovalAgentToolExecutor {
    pub(super) code_service: chatos_builtin_tools::CodeMaintainerService,
    pub(super) decision: Arc<Mutex<Option<ApprovalToolDecision>>>,
    pub(super) allow_code_tools: bool,
    pub(super) allow_approval_decision: bool,
}

#[derive(Debug, Deserialize)]
struct ApprovalDecisionToolArgs {
    decision: String,
    reason: String,
    #[serde(default)]
    remember_allow: bool,
}

#[async_trait]
impl ToolExecutor for ApprovalAgentToolExecutor {
    fn available_tools(&self) -> Vec<Value> {
        let mut tools = Vec::new();
        if self.allow_code_tools {
            tools.extend(
                self.code_service
                    .list_tools()
                    .into_iter()
                    .filter_map(|tool| {
                        let def = parse_tool_definition(&tool)?;
                        matches!(
                            def.name.as_str(),
                            "read_file_raw" | "read_file_range" | "list_dir" | "search_text"
                        )
                        .then(|| {
                            build_function_tool_schema(
                                def.name.as_str(),
                                def.description.as_str(),
                                &def.parameters,
                            )
                        })
                    }),
            );
        }
        if self.allow_approval_decision {
            tools.push(approval_decision_tool_schema());
        }
        tools
    }

    async fn execute_tools_stream(
        &self,
        tool_calls: &[Value],
        context: ToolCallContext,
        on_tool_result: Option<ToolResultCallback>,
    ) -> Vec<ToolResult> {
        let mut results = Vec::new();
        for tool_call in tool_calls {
            if context.is_aborted() {
                break;
            }
            let name = chatos_ai_runtime::tool_call::extract_tool_call_name(tool_call)
                .unwrap_or("")
                .to_string();
            let call_id = chatos_ai_runtime::tool_call::extract_tool_call_id(tool_call)
                .unwrap_or("")
                .to_string();
            let args = match parse_tool_args(
                chatos_ai_runtime::tool_call::clone_tool_call_arguments(tool_call),
            ) {
                Ok(args) => args,
                Err(err) => {
                    push_result(
                        &mut results,
                        ToolResult {
                            tool_call_id: call_id,
                            name,
                            success: false,
                            is_error: true,
                            is_stream: false,
                            conversation_turn_id: context.conversation_turn_id.clone(),
                            content: format!("参数解析失败: {err}"),
                            result: None,
                        },
                        on_tool_result.as_ref(),
                    );
                    continue;
                }
            };
            let result = if name == APPROVAL_DECISION_TOOL && self.allow_approval_decision {
                self.execute_approval_decision(call_id, args, &context)
            } else if self.allow_code_tools {
                self.execute_code_tool(name.as_str(), call_id, args, &context)
            } else {
                ToolResult {
                    tool_call_id: call_id,
                    name,
                    success: false,
                    is_error: true,
                    is_stream: false,
                    conversation_turn_id: context.conversation_turn_id.clone(),
                    content: "approval agent capability is not allowed by policy".to_string(),
                    result: None,
                }
            };
            push_result(&mut results, result, on_tool_result.as_ref());
        }
        results
    }
}

impl ApprovalAgentToolExecutor {
    fn execute_approval_decision(
        &self,
        call_id: String,
        args: Value,
        context: &ToolCallContext,
    ) -> ToolResult {
        let parsed = serde_json::from_value::<ApprovalDecisionToolArgs>(args);
        let parsed = match parsed {
            Ok(parsed) => parsed,
            Err(err) => {
                return ToolResult {
                    tool_call_id: call_id,
                    name: APPROVAL_DECISION_TOOL.to_string(),
                    success: false,
                    is_error: true,
                    is_stream: false,
                    conversation_turn_id: context.conversation_turn_id.clone(),
                    content: format!("approval_decision 参数无效: {err}"),
                    result: None,
                };
            }
        };
        let decision = parsed.decision.trim().to_ascii_lowercase();
        if !matches!(decision.as_str(), "approve" | "deny" | "ask_user") {
            return ToolResult {
                tool_call_id: call_id,
                name: APPROVAL_DECISION_TOOL.to_string(),
                success: false,
                is_error: true,
                is_stream: false,
                conversation_turn_id: context.conversation_turn_id.clone(),
                content: "approval_decision.decision must be approve, deny, or ask_user"
                    .to_string(),
                result: None,
            };
        }
        let reason = parsed.reason.trim().to_string();
        if reason.is_empty() {
            return ToolResult {
                tool_call_id: call_id,
                name: APPROVAL_DECISION_TOOL.to_string(),
                success: false,
                is_error: true,
                is_stream: false,
                conversation_turn_id: context.conversation_turn_id.clone(),
                content: "approval_decision.reason is required".to_string(),
                result: None,
            };
        }
        let remember_allow = decision == "approve" && parsed.remember_allow;
        if let Ok(mut guard) = self.decision.lock() {
            if guard.is_some() {
                return ToolResult {
                    tool_call_id: call_id,
                    name: APPROVAL_DECISION_TOOL.to_string(),
                    success: false,
                    is_error: true,
                    is_stream: false,
                    conversation_turn_id: context.conversation_turn_id.clone(),
                    content: "approval_decision has already been called for this request"
                        .to_string(),
                    result: None,
                };
            }
            *guard = Some(ApprovalToolDecision {
                decision: decision.clone(),
                reason: reason.clone(),
                remember_allow,
            });
        }
        let structured = json!({
            "decision": decision,
            "reason": reason,
            "remember_allow": remember_allow,
        });
        ToolResult {
            tool_call_id: call_id,
            name: APPROVAL_DECISION_TOOL.to_string(),
            success: true,
            is_error: false,
            is_stream: false,
            conversation_turn_id: context.conversation_turn_id.clone(),
            content: structured.to_string(),
            result: Some(structured),
        }
    }

    fn execute_code_tool(
        &self,
        name: &str,
        call_id: String,
        args: Value,
        context: &ToolCallContext,
    ) -> ToolResult {
        if !matches!(
            name,
            "read_file_raw" | "read_file_range" | "list_dir" | "search_text"
        ) {
            return ToolResult {
                tool_call_id: call_id,
                name: name.to_string(),
                success: false,
                is_error: true,
                is_stream: false,
                conversation_turn_id: context.conversation_turn_id.clone(),
                content: format!("approval agent tool is not allowed: {name}"),
                result: None,
            };
        }
        match self.code_service.call_tool(name, args, None) {
            Ok(result) => {
                let (content, structured) = to_text_and_structured_result(&result);
                ToolResult {
                    tool_call_id: call_id,
                    name: name.to_string(),
                    success: true,
                    is_error: false,
                    is_stream: false,
                    conversation_turn_id: context.conversation_turn_id.clone(),
                    content,
                    result: structured,
                }
            }
            Err(err) => ToolResult {
                tool_call_id: call_id,
                name: name.to_string(),
                success: false,
                is_error: true,
                is_stream: false,
                conversation_turn_id: context.conversation_turn_id.clone(),
                content: format!("工具执行失败: {err}"),
                result: None,
            },
        }
    }
}

fn approval_decision_tool_schema() -> Value {
    build_function_tool_schema(
        APPROVAL_DECISION_TOOL,
        "Return the final command approval decision for this request. Must be called exactly once.",
        &json!({
            "type": "object",
            "properties": {
                "decision": {
                    "type": "string",
                    "enum": ["approve", "deny", "ask_user"]
                },
                "reason": {
                    "type": "string",
                    "description": "Short concrete reason for the decision."
                },
                "remember_allow": {
                    "type": "boolean",
                    "description": "Set true only for a stable low-risk approve decision that should be whitelisted."
                }
            },
            "required": ["decision", "reason"],
            "additionalProperties": false
        }),
    )
}

fn parse_tool_args(args: Value) -> std::result::Result<Value, serde_json::Error> {
    match args {
        Value::String(raw) => serde_json::from_str(raw.as_str()),
        other => Ok(other),
    }
}

fn push_result(
    results: &mut Vec<ToolResult>,
    result: ToolResult,
    on_tool_result: Option<&ToolResultCallback>,
) {
    if let Some(callback) = on_tool_result {
        callback(&result);
    }
    results.push(result);
}
