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
        ClaimNextRunCommand, ClaimNextToolCommand, CommitStepCommand, CommitToolCommand,
        CreateConversationCommand, CreateRunCommand, GetRequirementSurveyCommand,
        LocalAgentRunStatus, LocalAgentStepOutcome, LocalAgentToolCall,
        LocalConversationResourceBinding, ResolveRequirementSurveyCommand,
    };
    use std::collections::BTreeMap;

    fn envelope(command_id: &str, command: HostCommand) -> HostRequestEnvelope {
        HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: command_id.to_string(),
            command,
        }
    }

    #[tokio::test]
    async fn survey_tool_commit_suspends_and_resolution_resumes_the_source_run() {
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
                    conversation_id: "conversation-survey-flow".to_string(),
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
                    run_id: "run-survey-flow".to_string(),
                    owner_user_id: "user-1".to_string(),
                    owner_entity_type: "task".to_string(),
                    owner_entity_id: "task-1".to_string(),
                    profile_key: "task_execution".to_string(),
                    model_config_ref: "model-1".to_string(),
                    model_config_revision: "revision-1".to_string(),
                    capability_policy_revision: "policy-1".to_string(),
                    input: json!({
                        "source_conversation_id": "conversation-survey-flow",
                        "prompt": "build"
                    }),
                    max_iterations: 8,
                }),
            ))
            .await
            .expect("run");
        let claimed = runtime
            .try_handle(envelope(
                "claim-run",
                HostCommand::ClaimNextRun(ClaimNextRunCommand {
                    owner_user_id: "user-1".to_string(),
                    worker_id: "model-worker".to_string(),
                    lease_duration_ms: 30_000,
                }),
            ))
            .await
            .expect("claim run");
        let HostResult::Claim { claim: Some(claim) } = claimed else {
            panic!("expected run claim");
        };
        runtime
            .try_handle(envelope(
                "wait-tool",
                HostCommand::CommitStep(CommitStepCommand {
                    owner_user_id: "user-1".to_string(),
                    run_id: claim.run.run_id,
                    claim_token: claim.claim_token,
                    expected_version: claim.run.version,
                    outcome: LocalAgentStepOutcome::WaitForTool {
                        batch_id: "survey-batch".to_string(),
                        tool_calls: vec![LocalAgentToolCall {
                            call_id: "survey-call".to_string(),
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
                        }],
                        checkpoint: json!({"phase": "requirements"}),
                    },
                }),
            ))
            .await
            .expect("wait for tool");
        let claimed_tool = runtime
            .try_handle(envelope(
                "claim-tool",
                HostCommand::ClaimNextTool(ClaimNextToolCommand {
                    owner_user_id: "user-1".to_string(),
                    worker_id: "local-tool-worker".to_string(),
                    lease_duration_ms: 30_000,
                    include_tool_names: Some(vec![REQUIREMENT_SURVEY_CREATE_TOOL.to_string()]),
                    exclude_tool_names: Vec::new(),
                }),
            ))
            .await
            .expect("claim tool");
        let HostResult::ToolClaim {
            claim: Some(tool_claim),
        } = claimed_tool
        else {
            panic!("expected tool claim");
        };
        let executor = LocalRequirementSurveyToolExecutor::new(Arc::clone(&runtime), "user-1")
            .expect("executor");
        let first_outcome = executor
            .execute_tool(&tool_claim.invocation)
            .await
            .expect("execute survey tool");
        let replayed_outcome = executor
            .execute_tool(&tool_claim.invocation)
            .await
            .expect("replay survey tool");
        assert_eq!(replayed_outcome, first_outcome);
        let commit_command = CommitToolCommand {
            owner_user_id: "user-1".to_string(),
            invocation_id: tool_claim.invocation.invocation_id.clone(),
            claim_token: tool_claim.claim_token.clone(),
            expected_version: tool_claim.invocation.version,
            outcome: first_outcome.clone(),
        };
        let committed = runtime
            .try_handle(envelope(
                "commit-survey-tool",
                HostCommand::CommitTool(commit_command.clone()),
            ))
            .await
            .expect("commit tool");
        let HostResult::ToolCommit { result } = committed else {
            panic!("expected tool commit");
        };
        assert_eq!(result.run.status, LocalAgentRunStatus::WaitingUser);
        assert_eq!(result.run.checkpoint, json!({"phase": "requirements"}));
        let replayed_commit = runtime
            .try_handle(envelope(
                "commit-survey-tool",
                HostCommand::CommitTool(commit_command),
            ))
            .await
            .expect("replay tool commit");
        assert_eq!(replayed_commit, HostResult::ToolCommit { result });

        let survey_id = format!("local-survey-{}", tool_claim.invocation.invocation_id);
        let stored = runtime
            .try_handle(envelope(
                "get-survey",
                HostCommand::GetRequirementSurvey(GetRequirementSurveyCommand {
                    owner_user_id: "user-1".to_string(),
                    survey_id: survey_id.clone(),
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
        let answers = BTreeMap::from([("deployment".to_string(), json!("local"))]);
        let resolve_command = ResolveRequirementSurveyCommand {
            owner_user_id: "user-1".to_string(),
            survey_id: survey_id.clone(),
            expected_version: 1,
            answers,
        };
        let resolved = runtime
            .try_handle(envelope(
                "resolve-survey-flow",
                HostCommand::ResolveRequirementSurvey(resolve_command.clone()),
            ))
            .await
            .expect("resolve survey");
        let HostResult::RequirementSurveyResolved { resolution } = &resolved else {
            panic!("expected survey resolution");
        };
        assert_eq!(
            resolution.resumed_run.status,
            LocalAgentRunStatus::ContinuationReady
        );
        assert_eq!(
            resolution.resumed_run.continuation_input.as_ref().unwrap()["survey_id"],
            survey_id
        );
        let replayed_resolution = runtime
            .try_handle(envelope(
                "resolve-survey-flow",
                HostCommand::ResolveRequirementSurvey(resolve_command),
            ))
            .await
            .expect("replay survey resolution");
        assert_eq!(replayed_resolution, resolved);
    }
}
