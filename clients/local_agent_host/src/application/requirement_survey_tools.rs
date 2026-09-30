// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{LocalToolExecutor, LocalToolRegistry};
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    CreateRequirementSurveyCommand, GetConversationCommand, HostCommand, HostRequestEnvelope,
    HostResult, LocalAgentToolInvocationRecord, LocalAgentToolOutcome,
    LocalConversationResourceKind, LocalRequirementSurveyQuestion, LOCAL_AGENT_PROTOCOL_VERSION,
};
use chatos_local_agent_runtime::LocalAgentRuntime;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

pub const REQUIREMENT_SURVEY_CREATE_TOOL: &str = "requirement_survey_create";
pub const REQUIREMENT_SURVEY_TOOL_NAMES: [&str; 1] = [REQUIREMENT_SURVEY_CREATE_TOOL];

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
        if parent.owner_user_id != self.owner_user_id || parent.profile_key != "task_execution" {
            return Err(
                "Requirement Surveys can only be created by the active local Task".to_string(),
            );
        }
        if parent.owner_entity_type != "task" {
            return Err("Requirement Survey source Run is not owned by a local Task".to_string());
        }
        let conversation_id = parent
            .input
            .get("source_conversation_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "local Task has no source conversation".to_string())?;
        let conversation = self
            .request(
                invocation,
                "get-conversation",
                HostCommand::GetConversation(GetConversationCommand {
                    owner_user_id: self.owner_user_id.clone(),
                    conversation_id: conversation_id.to_string(),
                }),
            )
            .await?;
        let HostResult::Conversation { conversation } = conversation else {
            return Err("Local Agent Host returned an unexpected Conversation result".to_string());
        };
        let resource = conversation
            .conversation
            .resource
            .filter(|resource| resource.kind == LocalConversationResourceKind::Project)
            .ok_or_else(|| {
                "Requirement Surveys require a project-bound conversation".to_string()
            })?;
        let result = self
            .request(
                invocation,
                "create",
                HostCommand::CreateRequirementSurvey(CreateRequirementSurveyCommand {
                    survey_id: format!("local-survey-{}", invocation.invocation_id),
                    owner_user_id: self.owner_user_id.clone(),
                    project_resource_id: resource.resource_id,
                    source_conversation_id: conversation_id.to_string(),
                    source_run_id: parent.run_id,
                    source_task_id: Some(parent.owner_entity_id),
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
            "next_action": "Stop execution and wait for the user to resolve this survey."
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

pub fn requirement_survey_model_tools() -> Vec<Value> {
    vec![json!({
        "type": "function",
        "name": REQUIREMENT_SURVEY_CREATE_TOOL,
        "description": "Ask the user structured project-requirement questions. After creating the survey, stop and wait for the user's answers; never invent answers.",
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
    })]
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_client_storage::SqliteClientStorage;
    use chatos_local_agent_protocol::{
        CreateConversationCommand, CreateRunCommand, GetRequirementSurveyCommand,
        LocalAgentToolApprovalStatus, LocalAgentToolStatus, LocalConversationResourceBinding,
    };

    fn envelope(command_id: &str, command: HostCommand) -> HostRequestEnvelope {
        HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: command_id.to_string(),
            command,
        }
    }

    #[tokio::test]
    async fn task_tool_derives_project_and_source_identity_from_local_state() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = Arc::new(LocalAgentRuntime::new(storage));
        runtime
            .try_handle(envelope(
                "conversation",
                HostCommand::CreateConversation(CreateConversationCommand {
                    conversation_id: "conversation-1".to_string(),
                    owner_user_id: "user-1".to_string(),
                    title: "Project".to_string(),
                    resource: Some(LocalConversationResourceBinding {
                        kind: LocalConversationResourceKind::Project,
                        resource_id: "project-1".to_string(),
                    }),
                }),
            ))
            .await
            .expect("conversation");
        runtime
            .try_handle(envelope(
                "run",
                HostCommand::CreateRun(CreateRunCommand {
                    run_id: "run-1".to_string(),
                    owner_user_id: "user-1".to_string(),
                    owner_entity_type: "task".to_string(),
                    owner_entity_id: "task-1".to_string(),
                    profile_key: "task_execution".to_string(),
                    model_config_ref: "model-1".to_string(),
                    model_config_revision: "revision-1".to_string(),
                    capability_policy_revision: "policy-1".to_string(),
                    input: json!({"source_conversation_id": "conversation-1", "prompt": "build"}),
                    max_iterations: 8,
                }),
            ))
            .await
            .expect("run");
        let invocation = LocalAgentToolInvocationRecord {
            invocation_id: "invocation-1".to_string(),
            run_id: "run-1".to_string(),
            batch_id: "batch-1".to_string(),
            call_id: "call-1".to_string(),
            tool_name: REQUIREMENT_SURVEY_CREATE_TOOL.to_string(),
            arguments: json!({
                "title": "Choose deployment",
                "questions": [{
                    "question_id": "deployment",
                    "prompt": "Where should this run?",
                    "response_kind": "single_choice",
                    "required": true,
                    "options": ["local", "cloud"]
                }]
            }),
            side_effecting: true,
            requires_approval: false,
            approval_status: LocalAgentToolApprovalStatus::NotRequired,
            approval_decided_by: None,
            approval_reason: None,
            approval_decided_at_unix_ms: None,
            status: LocalAgentToolStatus::Running,
            result: None,
            error: None,
            version: 1,
            claim_token: Some("claim-1".to_string()),
            claim_until_unix_ms: Some(i64::MAX),
            created_at_unix_ms: 1_000,
            updated_at_unix_ms: 1_000,
        };
        let outcome = LocalRequirementSurveyToolExecutor::new(Arc::clone(&runtime), "user-1")
            .expect("executor")
            .execute_tool(&invocation)
            .await
            .expect("outcome");
        assert!(matches!(outcome, LocalAgentToolOutcome::Succeeded { .. }));
        let stored = runtime
            .try_handle(envelope(
                "get-survey",
                HostCommand::GetRequirementSurvey(GetRequirementSurveyCommand {
                    owner_user_id: "user-1".to_string(),
                    survey_id: "local-survey-invocation-1".to_string(),
                }),
            ))
            .await
            .expect("stored survey");
        assert!(matches!(
            stored,
            HostResult::RequirementSurvey { survey }
                if survey.project_resource_id == "project-1"
                    && survey.source_task_id.as_deref() == Some("task-1")
        ));
    }
}
