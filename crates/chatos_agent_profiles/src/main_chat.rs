// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_local_agent_protocol::{LocalAgentRun, ModelStepResult};
use chatos_local_agent_runtime::{LocalAgentProfile, LocalAgentProfileStep, ModelGatewayOutput};
use serde_json::{json, Value};

use crate::shared::{
    ask_user_schema, parse_tool_arguments, validate_ask_user_arguments, validate_context_strategy,
};

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
        validate_context_strategy(
            run,
            context.native_compaction_threshold,
            context.memory_engine_active_threshold,
            context.maximum_summary_attempts,
        )?;
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

    async fn interpret_completed_output(
        &self,
        run: &LocalAgentRun,
        output: &ModelGatewayOutput,
    ) -> Result<ModelStepResult, String> {
        interpret_main_chat_output(
            run.project_id.as_deref(),
            run.capability_snapshot_ref.as_str(),
            output,
        )
    }
}

fn interpret_main_chat_output(
    project_id: Option<&str>,
    capability_snapshot_ref: &str,
    output: &ModelGatewayOutput,
) -> Result<ModelStepResult, String> {
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
        let arguments = parse_tool_arguments(call)?;
        match name {
            MAIN_CHAT_ASK_USER_TOOL => {
                if ask_question.is_some() || !task_calls.is_empty() {
                    return Err("ask_user must be the only tool call in a model step".to_string());
                }
                ask_question = Some(validate_ask_user_arguments(arguments)?);
            }
            MAIN_CHAT_CREATE_TASK_TOOL => {
                if ask_question.is_some() {
                    return Err("task creation cannot be mixed with ask_user".to_string());
                }
                if project_id.is_none() {
                    return Err(
                        "create_local_task requires the Main Chat run to have a frozen project_id"
                            .to_string(),
                    );
                }
                let arguments = validate_create_task_arguments(arguments)?;
                task_calls.push(json!({
                    "call_id": call.get("call_id"),
                    "name": name,
                    "effect": "idempotent_write",
                    "arguments": arguments,
                }));
            }
            _ => return Err(format!("main chat emitted unauthorized tool {name}")),
        }
    }
    if let Some(question) = ask_question {
        Ok(ModelStepResult::AskUser(question))
    } else {
        Ok(ModelStepResult::ToolCommand(json!({
            "project_id": project_id,
            "capability_snapshot_ref": capability_snapshot_ref,
            "calls": task_calls,
        })))
    }
}

fn validate_create_task_arguments(arguments: Value) -> Result<Value, String> {
    let object = arguments
        .as_object()
        .ok_or_else(|| "create_local_task arguments must be an object".to_string())?;
    if object
        .keys()
        .any(|key| key == "project_id" || key == "projectId")
    {
        return Err(
            "create_local_task must not choose project_id; the runtime injects it".to_string(),
        );
    }
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "objective" | "acceptance_criteria"))
    {
        return Err("create_local_task contains unsupported arguments".to_string());
    }
    let objective = object
        .get("objective")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "create_local_task objective must not be empty".to_string())?;
    let acceptance_criteria = object
        .get("acceptance_criteria")
        .and_then(Value::as_array)
        .filter(|criteria| {
            !criteria.is_empty()
                && criteria.iter().all(|criterion| {
                    criterion
                        .as_str()
                        .is_some_and(|value| !value.trim().is_empty())
                })
        })
        .ok_or_else(|| {
            "create_local_task acceptance_criteria must contain non-empty strings".to_string()
        })?;
    Ok(json!({
        "objective": objective,
        "acceptance_criteria": acceptance_criteria,
    }))
}

fn main_chat_boundary_prompt() -> String {
    "You are the Main Chat collaboration agent. Answer and review directly. You have no terminal, file-write, project-mutation, marketplace-plugin, or arbitrary MCP capability. Never claim that code, files, deployments, or external systems changed unless a completed local Task result in the supplied context proves it. When real project execution is required, call create_local_task exactly once with a bounded objective and deterministic acceptance criteria. The runtime owns project scope; never choose or emit a project ID. Ask the user only when a missing decision materially changes the requested outcome.".to_string()
}

fn main_chat_tools() -> Vec<Value> {
    vec![
        ask_user_schema(
            MAIN_CHAT_ASK_USER_TOOL,
            "Ask one material blocking question. Include relevant visual references whenever the decision depends on an image or UI state.",
        ),
        json!({
            "type": "function",
            "name": MAIN_CHAT_CREATE_TASK_TOOL,
            "description": "Create one local Task inside the project already frozen by the runtime. Provide a bounded objective and deterministic acceptance criteria; never choose a project ID.",
            "parameters": {
                "type": "object",
                "properties": {
                    "objective": {"type": "string"},
                    "acceptance_criteria": {
                        "type": "array",
                        "items": {"type": "string"},
                        "minItems": 1
                    }
                },
                "required": ["objective", "acceptance_criteria"],
                "additionalProperties": false
            }
        }),
    ]
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
        let gateway_output = output(vec![json!({
            "type": "function_call",
            "call_id": "call-1",
            "name": MAIN_CHAT_CREATE_TASK_TOOL,
            "arguments": "{\"objective\":\"implement it\",\"acceptance_criteria\":[\"the approved UI is implemented\"]}"
        })]);
        let result =
            interpret_main_chat_output(Some("project-1"), "capabilities-1", &gateway_output)
                .unwrap();
        let ModelStepResult::ToolCommand(payload) = result else {
            panic!("expected task command");
        };
        assert_eq!(payload["project_id"], "project-1");
        assert_eq!(payload["calls"][0]["name"], MAIN_CHAT_CREATE_TASK_TOOL);
        assert_eq!(payload["calls"][0]["effect"], "idempotent_write");
        assert!(payload["calls"][0]["arguments"].get("project_id").is_none());
    }

    #[test]
    fn task_creation_cannot_choose_or_invent_project_scope() {
        let model_chosen_project = output(vec![json!({
            "type": "function_call",
            "call_id": "call-project",
            "name": MAIN_CHAT_CREATE_TASK_TOOL,
            "arguments": {
                "objective": "implement it",
                "acceptance_criteria": ["tests pass"],
                "project_id": "project-model-selected"
            }
        })]);
        let error = interpret_main_chat_output(
            Some("project-frozen"),
            "capabilities-1",
            &model_chosen_project,
        )
        .unwrap_err();
        assert!(error.contains("runtime injects"));

        let projectless = output(vec![json!({
            "type": "function_call",
            "call_id": "call-projectless",
            "name": MAIN_CHAT_CREATE_TASK_TOOL,
            "arguments": {
                "objective": "implement it",
                "acceptance_criteria": ["tests pass"]
            }
        })]);
        let error = interpret_main_chat_output(None, "capabilities-1", &projectless).unwrap_err();
        assert!(error.contains("frozen project_id"));
    }

    #[test]
    fn ask_user_requires_and_preserves_the_visual_question_contract() {
        let visual_output = output(vec![json!({
            "type": "function_call",
            "call_id": "call-ask",
            "name": MAIN_CHAT_ASK_USER_TOOL,
            "arguments": {
                "prompt": "Which composition should I continue?",
                "options": [{
                    "option_id": "asymmetric",
                    "label": "Asymmetric",
                    "description": "More visual tension"
                }],
                "image_references": ["composition-preview-1"],
                "details": {"annotation_id": "annotation-1"}
            }
        })]);
        let result = interpret_main_chat_output(None, "capabilities-1", &visual_output).unwrap();
        let ModelStepResult::AskUser(question) = result else {
            panic!("expected Ask User result");
        };
        assert_eq!(question["prompt"], "Which composition should I continue?");
        assert_eq!(question["image_references"][0], "composition-preview-1");

        let obsolete = output(vec![json!({
            "type": "function_call",
            "call_id": "call-obsolete",
            "name": MAIN_CHAT_ASK_USER_TOOL,
            "arguments": {"question": "Which one?"}
        })]);
        assert!(
            interpret_main_chat_output(None, "capabilities-1", &obsolete)
                .unwrap_err()
                .contains("visual question contract")
        );
    }

    #[test]
    fn arbitrary_project_tools_are_rejected() {
        let output = output(vec![json!({
            "type": "function_call",
            "call_id": "call-1",
            "name": "write_file",
            "arguments": "{}"
        })]);
        assert!(interpret_main_chat_output(None, "capabilities-1", &output)
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
