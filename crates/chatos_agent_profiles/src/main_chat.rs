// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_local_agent_protocol::{ContextStrategy, LocalAgentRun, ModelStepResult};
use chatos_local_agent_runtime::{LocalAgentProfile, LocalAgentProfileStep, ModelGatewayOutput};
use serde_json::{json, Value};

pub const MAIN_CHAT_PROFILE_KEY: &str = "main_chat";
pub const MAIN_CHAT_ASK_USER_TOOL: &str = "ask_user";
pub const MAIN_CHAT_CREATE_TASK_TOOL: &str = "create_local_task";

#[derive(Debug, Clone, PartialEq)]
pub struct MainChatStepContext {
    pub base_system_prompt: String,
    pub contact_system_prompt: Option<String>,
    pub skill_catalog_prompt: Option<String>,
    pub project_context_prompt: Option<String>,
    pub current_goal_prompt: String,
    pub model_input_items: Vec<Value>,
    pub maximum_output_tokens: u32,
    pub native_compaction_threshold: Option<u64>,
    pub memory_engine_active_threshold: Option<u64>,
    pub maximum_summary_attempts: u8,
}

#[async_trait]
pub trait MainChatContextProvider: Send + Sync {
    async fn load_step_context(&self, run: &LocalAgentRun) -> Result<MainChatStepContext, String>;
}

pub struct MainChatAgentProfile {
    context_provider: Arc<dyn MainChatContextProvider>,
}

impl MainChatAgentProfile {
    pub fn new(context_provider: Arc<dyn MainChatContextProvider>) -> Self {
        Self { context_provider }
    }
}

#[async_trait]
impl LocalAgentProfile for MainChatAgentProfile {
    fn profile_key(&self) -> &'static str {
        MAIN_CHAT_PROFILE_KEY
    }

    async fn prepare_model_step(
        &self,
        run: &LocalAgentRun,
    ) -> Result<LocalAgentProfileStep, String> {
        let context = self.context_provider.load_step_context(run).await?;
        validate_context_strategy(run.context_strategy, &context)?;
        let mut sections = vec![context.base_system_prompt, main_chat_boundary_prompt()];
        sections.extend(
            [
                context.contact_system_prompt,
                context.skill_catalog_prompt,
                context.project_context_prompt,
                Some(format!(
                    "Current user goal:\n{}",
                    context.current_goal_prompt
                )),
            ]
            .into_iter()
            .flatten()
            .filter(|value| !value.trim().is_empty()),
        );
        Ok(LocalAgentProfileStep {
            model_input_items: context.model_input_items,
            tools: main_chat_tools(),
            instructions: Some(sections.join("\n\n")),
            maximum_output_tokens: context.maximum_output_tokens,
            reasoning_effort: None,
            temperature: None,
            native_compaction_threshold: context.native_compaction_threshold,
            memory_engine_active_threshold: context.memory_engine_active_threshold,
            maximum_summary_attempts: context.maximum_summary_attempts,
        })
    }

    fn interpret_completed_output(
        &self,
        _run: &LocalAgentRun,
        output: &ModelGatewayOutput,
    ) -> Result<ModelStepResult, String> {
        interpret_main_chat_output(output)
    }
}

fn interpret_main_chat_output(output: &ModelGatewayOutput) -> Result<ModelStepResult, String> {
    let calls = output
        .terminal
        .output_items
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("function_call"))
        .collect::<Vec<_>>();
    if calls.is_empty() {
        if output.content.trim().is_empty() {
            return Ok(ModelStepResult::Failed(json!({
                "reason": "empty_main_chat_result"
            })));
        }
        return Ok(ModelStepResult::Final(json!({"text": output.content})));
    }
    let mut task_calls = Vec::new();
    let mut ask_question = None;
    for call in calls {
        let name = call.get("name").and_then(Value::as_str).unwrap_or_default();
        let arguments = parse_arguments(call)?;
        match name {
            MAIN_CHAT_ASK_USER_TOOL => {
                if ask_question.is_some() || !task_calls.is_empty() {
                    return Err("ask_user must be the only tool call in a model step".to_string());
                }
                ask_question = Some(arguments);
            }
            MAIN_CHAT_CREATE_TASK_TOOL => {
                if ask_question.is_some() {
                    return Err("task creation cannot be mixed with ask_user".to_string());
                }
                task_calls.push(json!({
                    "call_id": call.get("call_id"),
                    "name": name,
                    "arguments": arguments,
                }));
            }
            _ => return Err(format!("main chat emitted unauthorized tool {name}")),
        }
    }
    if let Some(question) = ask_question {
        Ok(ModelStepResult::AskUser(question))
    } else {
        Ok(ModelStepResult::ToolCommand(json!({"calls": task_calls})))
    }
}

fn validate_context_strategy(
    strategy: ContextStrategy,
    context: &MainChatStepContext,
) -> Result<(), String> {
    match strategy {
        ContextStrategy::ProviderNative
            if context.native_compaction_threshold.is_some()
                && context.memory_engine_active_threshold.is_none()
                && context.maximum_summary_attempts == 0 =>
        {
            Ok(())
        }
        ContextStrategy::MemoryEngine
            if context.native_compaction_threshold.is_none()
                && context.memory_engine_active_threshold.is_some()
                && context.maximum_summary_attempts > 0 =>
        {
            Ok(())
        }
        _ => Err("main chat context settings do not match the frozen strategy".to_string()),
    }
}

fn main_chat_boundary_prompt() -> String {
    "You are the Main Chat collaboration agent. Answer and review directly. You have no terminal, file-write, project-mutation, marketplace-plugin, or arbitrary MCP capability. Never claim that code, files, deployments, or external systems changed unless a completed local Task result in the supplied context proves it. When real project execution is required, call create_local_task exactly once with a bounded objective. Ask the user only when a missing decision materially changes the requested outcome.".to_string()
}

fn main_chat_tools() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "name": MAIN_CHAT_ASK_USER_TOOL,
            "description": "Ask one material blocking question.",
            "parameters": {
                "type": "object",
                "properties": {"question": {"type": "string"}},
                "required": ["question"],
                "additionalProperties": true
            }
        }),
        json!({
            "type": "function",
            "name": MAIN_CHAT_CREATE_TASK_TOOL,
            "description": "Create one local Task for real project or computer execution.",
            "parameters": {
                "type": "object",
                "properties": {
                    "objective": {"type": "string"},
                    "project_id": {"type": ["string", "null"]}
                },
                "required": ["objective", "project_id"],
                "additionalProperties": false
            }
        }),
    ]
}

fn parse_arguments(call: &Value) -> Result<Value, String> {
    let arguments = call.get("arguments").cloned().unwrap_or(Value::Null);
    match arguments {
        Value::String(text) => serde_json::from_str(&text)
            .map_err(|error| format!("tool arguments are invalid JSON: {error}")),
        Value::Object(_) => Ok(arguments),
        _ => Err("tool arguments must be a JSON object".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use chatos_local_agent_protocol::{
        ModelGatewayTerminal, ModelGatewayTerminalSource, ModelGatewayTerminalStatus,
    };

    use super::*;

    #[test]
    fn main_chat_exposes_only_collaboration_and_task_creation_tools() {
        let names = main_chat_tools()
            .into_iter()
            .filter_map(|tool| tool["name"].as_str().map(ToOwned::to_owned))
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![MAIN_CHAT_ASK_USER_TOOL, MAIN_CHAT_CREATE_TASK_TOOL]
        );
    }

    #[test]
    fn project_execution_becomes_a_local_task_command() {
        let output = output(vec![json!({
            "type": "function_call",
            "call_id": "call-1",
            "name": MAIN_CHAT_CREATE_TASK_TOOL,
            "arguments": "{\"objective\":\"implement it\",\"project_id\":\"project-1\"}"
        })]);
        let result = interpret_main_chat_output(&output).unwrap();
        let ModelStepResult::ToolCommand(payload) = result else {
            panic!("expected task command");
        };
        assert_eq!(payload["calls"][0]["name"], MAIN_CHAT_CREATE_TASK_TOOL);
    }

    #[test]
    fn arbitrary_project_tools_are_rejected() {
        let output = output(vec![json!({
            "type": "function_call",
            "call_id": "call-1",
            "name": "write_file",
            "arguments": "{}"
        })]);
        assert!(interpret_main_chat_output(&output)
            .unwrap_err()
            .contains("unauthorized tool"));
    }

    fn output(items: Vec<Value>) -> ModelGatewayOutput {
        ModelGatewayOutput {
            content: String::new(),
            reasoning: String::new(),
            output_items: items.clone(),
            terminal: ModelGatewayTerminal {
                status: ModelGatewayTerminalStatus::Completed,
                source: ModelGatewayTerminalSource::Provider,
                response_id: Some("response-1".to_string()),
                provider_request_id: Some("request-1".to_string()),
                terminal_event: "response.completed".to_string(),
                provider_http_status: Some(200),
                usage: None,
                output_items: items,
                incomplete_details: None,
                provider_error: None,
            },
        }
    }
}
