// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_local_agent_protocol::{FrozenSnapshot, LocalAgentRun, ModelStepResult};
use chatos_local_agent_runtime::{LocalAgentProfile, LocalAgentProfileStep, ModelGatewayOutput};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::shared::{
    ask_user_schema, parse_tool_arguments, validate_ask_user_arguments, validate_context_strategy,
};

pub const MAIN_CHAT_PROFILE_KEY: &str = "main_chat";
pub const MAIN_CHAT_ASK_USER_TOOL: &str = "ask_user";
pub const MAIN_CHAT_CREATE_TASK_TOOL: &str = "create_local_task";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MainChatPromptSnapshot {
    pub prompt_revision: String,
    pub base_system_prompt: String,
    pub contact_system_prompt: Option<String>,
    pub skill_catalog_prompt: Option<String>,
}

impl MainChatPromptSnapshot {
    pub fn from_frozen(snapshot: &FrozenSnapshot) -> Result<Self, String> {
        let decoded: Self = decode_frozen_snapshot(snapshot, "Main Chat prompt snapshot")?;
        decoded.validate()?;
        if decoded.prompt_revision != snapshot.revision {
            return Err(
                "Main Chat prompt snapshot identity does not match its envelope".to_string(),
            );
        }
        Ok(decoded)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.prompt_revision.trim().is_empty() {
            return Err("Main Chat prompt revision must not be empty".to_string());
        }
        if self.base_system_prompt.trim().is_empty() {
            return Err("Main Chat base system prompt must not be empty".to_string());
        }
        if contains_local_path(&self.base_system_prompt) {
            return Err("Main Chat base system prompt contains a local path".to_string());
        }
        for (field, value) in [
            (
                "contact system prompt",
                self.contact_system_prompt.as_deref(),
            ),
            ("skill catalog prompt", self.skill_catalog_prompt.as_deref()),
        ] {
            if value.is_some_and(|value| value.trim().is_empty()) {
                return Err(format!("Main Chat {field} must not be empty when supplied"));
            }
            if value.is_some_and(contains_local_path) {
                return Err(format!("Main Chat {field} contains a local path"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MainChatProjectSnapshot {
    pub project_id: String,
    pub snapshot_revision: String,
    pub project_name: String,
    pub design_context: Value,
}

impl MainChatProjectSnapshot {
    pub fn from_frozen(snapshot: &FrozenSnapshot) -> Result<Self, String> {
        let decoded: Self = decode_frozen_snapshot(snapshot, "Main Chat project snapshot")?;
        decoded.validate()?;
        if decoded.snapshot_revision != snapshot.revision {
            return Err(
                "Main Chat project snapshot identity does not match its envelope".to_string(),
            );
        }
        Ok(decoded)
    }

    pub fn validate(&self) -> Result<(), String> {
        for (field, value) in [
            ("project ID", self.project_id.as_str()),
            ("project snapshot revision", self.snapshot_revision.as_str()),
            ("project name", self.project_name.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(format!("Main Chat {field} must not be empty"));
            }
        }
        if !self.design_context.is_object() {
            return Err("Main Chat design context must be an object".to_string());
        }
        validate_safe_project_context(&self.design_context)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MainChatCapabilitySnapshot {
    pub snapshot_ref: String,
    pub allowed_tools: Vec<String>,
}

impl MainChatCapabilitySnapshot {
    pub fn from_frozen(snapshot: &FrozenSnapshot) -> Result<Self, String> {
        let decoded: Self = decode_frozen_snapshot(snapshot, "Main Chat capability snapshot")?;
        decoded.validate()?;
        if decoded.snapshot_ref != snapshot.snapshot_id {
            return Err(
                "Main Chat capability snapshot identity does not match its envelope".to_string(),
            );
        }
        Ok(decoded)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.snapshot_ref.trim().is_empty() {
            return Err("Main Chat capability snapshot reference must not be empty".to_string());
        }
        let mut tools = self
            .allowed_tools
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        tools.sort_unstable();
        tools.dedup();
        let mut expected = vec![MAIN_CHAT_ASK_USER_TOOL, MAIN_CHAT_CREATE_TASK_TOOL];
        expected.sort_unstable();
        if tools != expected || self.allowed_tools.len() != expected.len() {
            return Err(
                "Main Chat capabilities must contain only ask_user and create_local_task"
                    .to_string(),
            );
        }
        Ok(())
    }
}

fn decode_frozen_snapshot<T: serde::de::DeserializeOwned>(
    snapshot: &FrozenSnapshot,
    label: &str,
) -> Result<T, String> {
    snapshot
        .validate("main_chat_snapshot")
        .map_err(|error| format!("{label} failed integrity validation: {error}"))?;
    serde_json::from_value(snapshot.payload.clone())
        .map_err(|error| format!("{label} payload is invalid: {error}"))
}

#[derive(Debug, Clone, PartialEq)]
pub struct MainChatStepContext {
    pub prompt_snapshot: MainChatPromptSnapshot,
    pub capability_snapshot: MainChatCapabilitySnapshot,
    pub project_snapshot: Option<MainChatProjectSnapshot>,
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
        validate_main_chat_context(run, &context)?;
        let mut sections = vec![
            context.prompt_snapshot.base_system_prompt,
            main_chat_boundary_prompt(),
        ];
        sections.extend(
            [
                context.prompt_snapshot.contact_system_prompt,
                context.prompt_snapshot.skill_catalog_prompt,
                context
                    .project_snapshot
                    .as_ref()
                    .map(project_context_prompt)
                    .transpose()?,
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

fn validate_main_chat_context(
    run: &LocalAgentRun,
    context: &MainChatStepContext,
) -> Result<(), String> {
    context.prompt_snapshot.validate()?;
    context.capability_snapshot.validate()?;
    if context.prompt_snapshot.prompt_revision != run.prompt_revision {
        return Err("Main Chat prompt snapshot does not match the frozen Run".to_string());
    }
    if context.capability_snapshot.snapshot_ref != run.capability_snapshot_ref {
        return Err("Main Chat capability snapshot does not match the frozen Run".to_string());
    }
    match (run.project_id.as_deref(), context.project_snapshot.as_ref()) {
        (None, None) => Ok(()),
        (Some(project_id), Some(snapshot)) if snapshot.project_id == project_id => {
            snapshot.validate()
        }
        _ => Err("Main Chat project snapshot does not match the frozen Run".to_string()),
    }
}

fn project_context_prompt(snapshot: &MainChatProjectSnapshot) -> Result<String, String> {
    serde_json::to_string(&json!({
        "project_id": snapshot.project_id,
        "snapshot_revision": snapshot.snapshot_revision,
        "project_name": snapshot.project_name,
        "design_context": snapshot.design_context,
    }))
    .map(|context| format!("Frozen project design context:\n{context}"))
    .map_err(|error| format!("failed to serialize Main Chat project context: {error}"))
}

fn validate_safe_project_context(value: &Value) -> Result<(), String> {
    match value {
        Value::Array(values) => {
            for value in values {
                validate_safe_project_context(value)?;
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                let normalized = key.to_ascii_lowercase();
                if normalized.contains("authority")
                    || normalized.contains("payload_reference")
                    || normalized.contains("working_directory")
                    || normalized.contains("root_reference")
                    || normalized == "path"
                {
                    return Err(format!(
                        "Main Chat project design context contains forbidden field {key}"
                    ));
                }
                validate_safe_project_context(value)?;
            }
        }
        Value::String(value) if looks_like_local_path(value) => {
            return Err("Main Chat project design context contains a local path".to_string());
        }
        _ => {}
    }
    Ok(())
}

fn looks_like_local_path(value: &str) -> bool {
    let trimmed = value.trim();
    contains_local_path(trimmed)
}

fn contains_local_path(value: &str) -> bool {
    value.contains("file://")
        || value.contains("/Users/")
        || value.contains("/Volumes/")
        || value.contains("/home/")
        || value.as_bytes().windows(3).any(|window| {
            window[0].is_ascii_alphabetic()
                && window[1] == b':'
                && matches!(window[2], b'\\' | b'/')
        })
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
