// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{LocalToolExecutor, LocalToolRegistry};
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    CreateRequirementSurveyCommand, GetConversationCommand, HostCommand, HostRequestEnvelope,
    HostResult, LocalAgentRunRecord, LocalAgentToolInvocationRecord, LocalAgentToolOutcome,
    LocalConversationResourceKind, LocalRequirementSurveyQuestion, SaveRequirementPlanCommand,
    LOCAL_AGENT_PROTOCOL_VERSION, LOCAL_REQUIREMENT_PLAN_SAVE_TOOL_NAME,
    LOCAL_REQUIREMENT_SURVEY_CREATE_TOOL_NAME,
};
use chatos_local_agent_runtime::LocalAgentRuntime;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

pub const REQUIREMENT_SURVEY_CREATE_TOOL: &str = LOCAL_REQUIREMENT_SURVEY_CREATE_TOOL_NAME;
pub const REQUIREMENT_SURVEY_TOOL_NAMES: [&str; 2] = [
    REQUIREMENT_SURVEY_CREATE_TOOL,
    LOCAL_REQUIREMENT_PLAN_SAVE_TOOL_NAME,
];

#[derive(Clone)]
pub struct LocalRequirementSurveyToolExecutor {
    runtime: Arc<LocalAgentRuntime>,
    owner_user_id: String,
}

impl LocalRequirementSurveyToolExecutor {
    pub fn new(
        runtime: Arc<LocalAgentRuntime>,
        owner_user_id: impl Into<String>,
    ) -> Result<Self, String> {
        let owner_user_id = owner_user_id.into().trim().to_string();
        if owner_user_id.is_empty() || owner_user_id.len() > 256 {
            return Err("Requirement Survey tool owner must be 1..=256 characters".to_string());
        }
        Ok(Self {
            runtime,
            owner_user_id,
        })
    }

    pub fn register_into(&self, registry: &mut LocalToolRegistry) -> Result<(), String> {
        let executor: Arc<dyn LocalToolExecutor> = Arc::new(self.clone());
        for name in REQUIREMENT_SURVEY_TOOL_NAMES {
            registry.register_shared(name, Arc::clone(&executor))?;
        }
        Ok(())
    }

    async fn execute(&self, invocation: &LocalAgentToolInvocationRecord) -> Result<Value, String> {
        if invocation.tool_name == LOCAL_REQUIREMENT_PLAN_SAVE_TOOL_NAME {
            let args: SavePlanArgs = serde_json::from_value(invocation.arguments.clone())
                .map_err(|error| format!("invalid requirement_plan_save input: {error}"))?;
            let parent = self
                .runtime
                .get_run_for_host_worker(&invocation.run_id)
                .await
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "plan source Run not found".to_string())?;
            if parent.owner_user_id != self.owner_user_id {
                return Err("plan Run does not belong to active owner".into());
            }
            let survey_id = program_bound_plan_survey_id(&parent)?;
            let result = self
                .request(
                    invocation,
                    "save-plan",
                    HostCommand::SaveRequirementPlan(SaveRequirementPlanCommand {
                        owner_user_id: self.owner_user_id.clone(),
                        survey_id,
                        source_run_id: parent.run_id,
                        expected_revision: args.expected_revision,
                        markdown: args.markdown,
                    }),
                )
                .await?;
            let HostResult::RequirementPlan { plan: Some(plan) } = result else {
                return Err("unexpected plan result".into());
            };
            return Ok(
                json!({"plan": plan, "requires_user_confirmation": true, "tasks_started": false}),
            );
        }
        if invocation.tool_name != REQUIREMENT_SURVEY_CREATE_TOOL {
            return Err(format!(
                "unsupported Requirement Survey tool: {}",
                invocation.tool_name
            ));
        }
        let args: CreateSurveyArgs = serde_json::from_value(invocation.arguments.clone())
            .map_err(|error| format!("invalid requirement_survey_create input: {error}"))?;
        let parent = self
            .runtime
            .get_run_for_host_worker(&invocation.run_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("parent Run not found: {}", invocation.run_id))?;
        if parent.owner_user_id != self.owner_user_id {
            return Err("Requirement Survey Run does not belong to the active owner".to_string());
        }
        let (conversation_field, source_task_id, team_project_id) = match (
            parent.profile_key.as_str(),
            parent.owner_entity_type.as_str(),
        ) {
            ("main_chat", "conversation_turn") => ("conversation_id", None, None),
            ("task_execution", "task") => (
                "source_conversation_id",
                Some(parent.owner_entity_id.clone()),
                None,
            ),
            ("task_execution", "conversation_turn") => {
                let context = parent
                    .input
                    .get("team_executor_context")
                    .and_then(Value::as_object)
                    .ok_or_else(|| {
                        "Requirement Surveys require an active project Main Chat, Task, or team Todo Run".to_string()
                    })?;
                let todo_id = context
                    .get("todo_id")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| "team Requirement Survey has no bound Todo".to_string())?;
                let project_id = context
                    .get("project_id")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| "team Requirement Survey has no bound project".to_string())?;
                (
                    "conversation_id",
                    Some(todo_id.to_string()),
                    Some(project_id.to_string()),
                )
            }
            _ => return Err(
                "Requirement Surveys require an active project Main Chat, Task, or team Todo Run"
                    .to_string(),
            ),
        };
        let conversation_id = parent
            .input
            .get(conversation_field)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "Requirement Survey Run has no source conversation".to_string())?;
        let conversation = self
            .request(
                invocation,
                "get-conversation",
                HostCommand::GetConversationContext(GetConversationCommand {
                    owner_user_id: self.owner_user_id.clone(),
                    conversation_id: conversation_id.to_string(),
                }),
            )
            .await?;
        let HostResult::Conversation { conversation } = conversation else {
            return Err("Local Agent Host returned an unexpected Conversation result".to_string());
        };
        if parent.owner_entity_type == "conversation_turn"
            && !conversation
                .turns
                .iter()
                .any(|turn| turn.run_id == parent.run_id && turn.turn_id == parent.owner_entity_id)
        {
            return Err(
                "Requirement Survey Run does not match the source Main Chat turn".to_string(),
            );
        }
        let project_resource_id = if let Some(project_id) = team_project_id {
            if conversation.conversation.resource.is_some() {
                return Err("team executor conversation must remain internal".to_string());
            }
            project_id
        } else {
            match conversation.conversation.resource {
                Some(resource) if resource.kind == LocalConversationResourceKind::Project => {
                    resource.resource_id
                }
                _ => format!("conversation:{conversation_id}"),
            }
        };
        let result = self
            .request(
                invocation,
                "create",
                HostCommand::CreateRequirementSurvey(CreateRequirementSurveyCommand {
                    survey_id: format!("local-survey-{}", invocation.invocation_id),
                    owner_user_id: self.owner_user_id.clone(),
                    project_resource_id,
                    source_conversation_id: conversation_id.to_string(),
                    source_run_id: parent.run_id,
                    source_task_id,
                    title: args.title,
                    description: args.description,
                    questions: args.questions,
                }),
            )
            .await?;
        let HostResult::RequirementSurvey { survey } = result else {
            return Err("Local Agent Host returned an unexpected Survey result".to_string());
        };
        Ok(json!({
            "survey": survey,
            "run_state": "waiting_user"
        }))
    }

    async fn request(
        &self,
        invocation: &LocalAgentToolInvocationRecord,
        phase: &str,
        command: HostCommand,
    ) -> Result<HostResult, String> {
        self.runtime
            .try_handle(HostRequestEnvelope {
                protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
                command_id: format!("internal-survey-{}-{phase}", invocation.invocation_id),
                command,
            })
            .await
            .map_err(|error| error.to_string())
    }
}

#[async_trait]
impl LocalToolExecutor for LocalRequirementSurveyToolExecutor {
    async fn execute_tool(
        &self,
        invocation: &LocalAgentToolInvocationRecord,
    ) -> Result<LocalAgentToolOutcome, String> {
        Ok(match self.execute(invocation).await {
            Ok(output) => LocalAgentToolOutcome::Succeeded { output },
            Err(error) => LocalAgentToolOutcome::Failed {
                error,
                detail: json!({"phase": "local_requirement_survey_tool"}),
            },
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateSurveyArgs {
    title: String,
    #[serde(default)]
    description: Option<String>,
    questions: Vec<LocalRequirementSurveyQuestion>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavePlanArgs {
    expected_revision: u64,
    markdown: String,
}

fn program_bound_plan_survey_id(parent: &LocalAgentRunRecord) -> Result<String, String> {
    if let Some(continuation) = parent.continuation_input.as_ref() {
        if let Some(value) = continuation.get("survey_id") {
            return non_empty_program_id(value, "continuation_input.survey_id");
        }
    }
    if let Some(binding) = parent.input.get("requirement_plan_binding") {
        if let Some(value) = binding.get("source_survey_id") {
            return non_empty_program_id(value, "input.requirement_plan_binding.source_survey_id");
        }
    }
    if let Some(plan) = parent.input.get("requirement_plan") {
        if let Some(value) = plan.get("source_survey_id") {
            return non_empty_program_id(value, "input.requirement_plan.source_survey_id");
        }
    }
    if parent.profile_key == "task_execution"
        && parent.owner_entity_type == "task"
        && parent.input.get("task_kind").and_then(Value::as_str) == Some("planning")
    {
        return Ok(format!("plan-review:{}", parent.run_id));
    }
    Err(
        "requirement_plan_save has no program-bound Requirement Survey or planning Task context"
            .to_string(),
    )
}

fn non_empty_program_id(value: &Value, field: &str) -> Result<String, String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("invalid program-bound {field}"))
}

pub fn requirement_survey_model_tools() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "name": REQUIREMENT_SURVEY_CREATE_TOOL,
            "description": "Collect material project goals, audience, constraints, preferences and acceptance decisions in the Requirement Survey center. Project Main Chat, an authorized Agent team survey Todo, or a blocked Task can ask. Call alone, then wait for submitted answers; never invent answers or dispatch dependent work while waiting.",
            "parameters": {
                "type": "object",
                "properties": {
                    "title": {"type": "string"},
                    "description": {"type": "string"},
                    "questions": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": 50,
                        "items": {
                            "type": "object",
                            "properties": {
                                "question_id": {"type": "string"},
                                "prompt": {"type": "string"},
                                "response_kind": {
                                    "type": "string",
                                    "enum": ["text", "single_choice", "multiple_choice", "boolean"]
                                },
                                "required": {"type": "boolean"},
                                "options": {"type": "array", "items": {"type": "string"}, "maxItems": 50}
                            },
                            "required": ["question_id", "prompt", "response_kind"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["title", "questions"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function", "name": LOCAL_REQUIREMENT_PLAN_SAVE_TOOL_NAME,
            "description": "Publish or revise a versioned proposal from a Plugin-authorized planning Task, submitted Requirement Survey answers, or the program-bound current plan. The Host binds every internal ID, owner, project and source Run. Submit only expected_revision and markdown. Use expected_revision 0 only for the initial draft, otherwise the current revision. Never confirm for the user. Finish with the same proposal and ask the user to review and confirm its exact version before execution.",
            "parameters": {"type": "object", "properties": {
                "expected_revision": {"type": "integer", "minimum": 0},
                "markdown": {"type": "string", "minLength": 1, "maxLength": 65536}
            }, "required": ["expected_revision", "markdown"], "additionalProperties": false}
        }),
    ]
}

#[cfg(test)]
mod tests;
