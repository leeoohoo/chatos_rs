// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{LocalToolExecutor, LocalToolRegistry};
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    CreateRequirementSurveyCommand, GetConversationCommand, HostCommand, HostRequestEnvelope,
    HostResult, LocalAgentToolInvocationRecord, LocalAgentToolOutcome,
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
            let result = self
                .request(
                    invocation,
                    "save-plan",
                    HostCommand::SaveRequirementPlan(SaveRequirementPlanCommand {
                        owner_user_id: self.owner_user_id.clone(),
                        survey_id: args.survey_id,
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
        let (conversation_field, source_task_id) = match (
            parent.profile_key.as_str(),
            parent.owner_entity_type.as_str(),
        ) {
            ("main_chat", "conversation_turn") => ("conversation_id", None),
            ("task_execution", "task") => (
                "source_conversation_id",
                Some(parent.owner_entity_id.clone()),
            ),
            _ => {
                return Err(
                    "Requirement Surveys require an active Main Chat turn or Task".to_string(),
                )
            }
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
        if source_task_id.is_none()
            && !conversation
                .turns
                .iter()
                .any(|turn| turn.run_id == parent.run_id && turn.turn_id == parent.owner_entity_id)
        {
            return Err(
                "Requirement Survey Run does not match the source Main Chat turn".to_string(),
            );
        }
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
    survey_id: String,
    expected_revision: u64,
    markdown: String,
}

pub fn requirement_survey_model_tools() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "name": REQUIREMENT_SURVEY_CREATE_TOOL,
            "description": "Collect material project goals, audience, constraints, preferences and acceptance decisions in the Requirement Survey center. Main Chat can ask before creating Tasks; a Task can ask about newly discovered blockers. Call alone, then wait for submitted answers; never invent answers or dispatch dependent work while waiting.",
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
            "description": "Publish a versioned proposal from submitted Requirement Survey answers, or revise that proposal. The Host binds owner, project and source Run. Use expected_revision 0 only for the initial draft; otherwise use the current revision from program-bound plan context. Never confirm for the user. Finish with a useful proposal and ask the user to review and confirm the exact version in Requirement Survey before creating Tasks. Saving does not authorize or start execution.",
            "parameters": {"type": "object", "properties": {
                "survey_id": {"type": "string", "minLength": 1},
                "expected_revision": {"type": "integer", "minimum": 0},
                "markdown": {"type": "string", "minLength": 1, "maxLength": 65536}
            }, "required": ["survey_id", "expected_revision", "markdown"], "additionalProperties": false}
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_client_storage::SqliteClientStorage;
    use chatos_local_agent_protocol::{
        CancelConversationTurnCommand, ClaimNextRunCommand, ClaimNextToolCommand,
        CommitStepCommand, CommitToolCommand, CreateConversationCommand, CreateRunCommand,
        GetRequirementSurveyCommand, LocalAgentRunStatus, LocalAgentStepOutcome,
        LocalAgentToolCall, LocalConversationResourceBinding, ResolveRequirementSurveyCommand,
        StartConversationTurnCommand,
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
        survey_flow(false, true, false, false).await;
    }

    #[tokio::test]
    async fn main_chat_survey_suspends_and_resumes_without_creating_a_task() {
        survey_flow(true, true, false, false).await;
    }

    #[tokio::test]
    async fn contact_main_chat_cannot_create_a_project_survey() {
        survey_flow(true, false, false, false).await;
    }

    #[tokio::test]
    async fn survey_rejects_a_mixed_tool_batch_before_creating_a_form() {
        survey_flow(true, true, true, false).await;
    }

    #[tokio::test]
    async fn cancelling_main_chat_survey_updates_notice_and_removes_unanswerable_form() {
        survey_flow(true, true, false, true).await;
    }

    async fn survey_flow(main_chat: bool, project_bound: bool, mixed_batch: bool, cancel: bool) {
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
                        kind: if project_bound {
                            LocalConversationResourceKind::Project
                        } else {
                            LocalConversationResourceKind::Contact
                        },
                        resource_id: "project-1".to_string(),
                    }),
                }),
            ))
            .await
            .expect("conversation");
        runtime
            .try_handle(envelope(
                "run",
                if main_chat {
                    HostCommand::StartConversationTurn(StartConversationTurnCommand {
                        owner_user_id: "user-1".into(),
                        conversation_id: "conversation-survey-flow".into(),
                        expected_conversation_version: 1,
                        turn_id: "turn-survey-flow".into(),
                        message_id: "message-survey-flow".into(),
                        run_id: "run-survey-flow".into(),
                        message: "understand my goals first".into(),
                        message_metadata: Value::Null,
                        attachments: vec![],
                        model_config_ref: "model-1".into(),
                        model_config_revision: "revision-1".into(),
                        capability_policy_revision: "policy-1".into(),
                        max_iterations: 8,
                    })
                } else {
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
                    })
                },
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
        let wait_result = runtime
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
                        }]
                        .into_iter()
                        .chain(mixed_batch.then_some(LocalAgentToolCall {
                            call_id: "other-call".into(),
                            tool_name: "list_tasks".into(),
                            arguments: json!({}),
                            side_effecting: false,
                            requires_approval: false,
                        }))
                        .collect(),
                        checkpoint: json!({"phase": "requirements"}),
                    },
                }),
            ))
            .await;
        if mixed_batch {
            assert!(wait_result.unwrap_err().to_string().contains("only call"));
            return;
        }
        wait_result.expect("wait for tool");
        let claimed_tool = runtime
            .try_handle(envelope(
                "claim-tool",
                HostCommand::ClaimNextTool(ClaimNextToolCommand {
                    owner_user_id: "user-1".to_string(),
                    worker_id: "local-tool-worker".to_string(),
                    lease_duration_ms: 30_000,
                    include_tool_names: Some(vec![REQUIREMENT_SURVEY_CREATE_TOOL.to_string()]),
                    exclude_tool_names: Vec::new(),
                    execution_route: None,
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
        let wrong_owner =
            LocalRequirementSurveyToolExecutor::new(Arc::clone(&runtime), "another-user").unwrap();
        assert!(matches!(
            wrong_owner
                .execute_tool(&tool_claim.invocation)
                .await
                .unwrap(),
            LocalAgentToolOutcome::Failed { .. }
        ));
        let first_outcome = executor
            .execute_tool(&tool_claim.invocation)
            .await
            .expect("execute survey tool");
        if !project_bound {
            assert!(
                matches!(first_outcome, LocalAgentToolOutcome::Failed { ref error, .. }
                if error.contains("project-bound"))
            );
            return;
        }
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
        if main_chat {
            let HostResult::ConversationHistory { page: conversation } = runtime
                .try_handle(envelope(
                    "get-waiting-chat",
                    HostCommand::GetConversationHistory(
                        chatos_local_agent_protocol::GetConversationHistoryCommand {
                            owner_user_id: "user-1".into(),
                            conversation_id: "conversation-survey-flow".into(),
                            before_ordinal: None,
                            limit: 100,
                        },
                    ),
                ))
                .await
                .unwrap()
            else {
                panic!("conversation")
            };
            assert_eq!(conversation.messages.len(), 2);
            assert_eq!(conversation.messages[1].content["status"], "open");
        }
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
                    && survey.source_task_id.as_deref() == if main_chat { None } else { Some("task-1") }
        ));
        if cancel {
            let HostResult::ConversationHistory { page: conversation } = runtime
                .try_handle(envelope(
                    "get-chat-before-cancel",
                    HostCommand::GetConversationHistory(
                        chatos_local_agent_protocol::GetConversationHistoryCommand {
                            owner_user_id: "user-1".into(),
                            conversation_id: "conversation-survey-flow".into(),
                            before_ordinal: None,
                            limit: 100,
                        },
                    ),
                ))
                .await
                .unwrap()
            else {
                panic!("conversation")
            };
            let original_notice = conversation.messages[1].clone();
            let cancelled = runtime
                .try_handle(envelope(
                    "cancel-survey-turn",
                    HostCommand::CancelConversationTurn(CancelConversationTurnCommand {
                        owner_user_id: "user-1".into(),
                        conversation_id: "conversation-survey-flow".into(),
                        expected_conversation_version: conversation.conversation.version,
                        turn_id: "turn-survey-flow".into(),
                        expected_run_version: None,
                        reason: "user cancelled".into(),
                    }),
                ))
                .await
                .unwrap();
            assert!(
                matches!(cancelled, HostResult::ConversationTurnUpdated { result }
                if result.run.status == LocalAgentRunStatus::Cancelled)
            );
            let HostResult::ConversationHistory { page: conversation } = runtime
                .try_handle(envelope(
                    "get-chat-after-cancel",
                    HostCommand::GetConversationHistory(
                        chatos_local_agent_protocol::GetConversationHistoryCommand {
                            owner_user_id: "user-1".into(),
                            conversation_id: "conversation-survey-flow".into(),
                            before_ordinal: None,
                            limit: 100,
                        },
                    ),
                ))
                .await
                .unwrap()
            else {
                panic!("conversation")
            };
            assert_eq!(conversation.messages.len(), 2);
            assert_eq!(
                conversation.messages[1].message_id,
                original_notice.message_id
            );
            assert_eq!(conversation.messages[1].ordinal, original_notice.ordinal);
            assert_eq!(conversation.messages[1].content["status"], "cancelled");
            assert!(runtime
                .try_handle(envelope(
                    "get-cancelled-survey",
                    HostCommand::GetRequirementSurvey(GetRequirementSurveyCommand {
                        owner_user_id: "user-1".into(),
                        survey_id,
                    }),
                ))
                .await
                .is_err());
            return;
        }
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
        // Exercise the real tool path after answers, not just a seeded DB row.
        let HostResult::Claim {
            claim: Some(generation),
        } = runtime
            .try_handle(envelope(
                "claim-proposal",
                HostCommand::ClaimNextRun(ClaimNextRunCommand {
                    owner_user_id: "user-1".into(),
                    worker_id: "model-worker".into(),
                    lease_duration_ms: 30_000,
                }),
            ))
            .await
            .unwrap()
        else {
            panic!("proposal claim")
        };
        let markdown = "## Fictional finance proposal\nLocal-only prototype. No real bookkeeping.";
        let mut publish = tool_claim.invocation.clone();
        publish.invocation_id = "publish-proposal".into();
        publish.tool_name = LOCAL_REQUIREMENT_PLAN_SAVE_TOOL_NAME.into();
        publish.arguments =
            json!({"survey_id": survey_id, "expected_revision": 0, "markdown": markdown});
        let mut forged = publish.clone();
        forged.arguments["confirmed_at_unix_ms"] = json!(1);
        assert!(matches!(
            executor.execute_tool(&forged).await.unwrap(),
            LocalAgentToolOutcome::Failed { .. }
        ));
        assert!(matches!(
            wrong_owner.execute_tool(&publish).await.unwrap(),
            LocalAgentToolOutcome::Failed { .. }
        ));
        let LocalAgentToolOutcome::Succeeded { output } =
            executor.execute_tool(&publish).await.unwrap()
        else {
            panic!("publish")
        };
        assert_eq!(output["requires_user_confirmation"], true);
        assert_eq!(output["tasks_started"], false);
        let confirm = chatos_local_agent_protocol::ConfirmRequirementPlanCommand {
            owner_user_id: "user-1".into(),
            survey_id: survey_id.clone(),
            expected_revision: 1,
        };
        assert!(runtime
            .try_handle(envelope(
                "premature-confirm",
                HostCommand::ConfirmRequirementPlan(confirm.clone())
            ))
            .await
            .is_err());
        assert_eq!(output["plan"]["source_run_id"], "run-survey-flow");
        assert!(!requirement_survey_model_tools()
            .iter()
            .any(|tool| tool["name"]
                .as_str()
                .is_some_and(|name| name.contains("confirm"))));
        // The standalone Task fixture has no graph. Both profiles exercise real
        // submission/resumption/publication and premature-confirmation guards;
        // real Task completion/retry/freezing is covered in storage tests.
        if !main_chat {
            return;
        }
        runtime
            .try_handle(envelope(
                "finish-proposal",
                HostCommand::CommitStep(CommitStepCommand {
                    owner_user_id: "user-1".into(),
                    run_id: generation.run.run_id,
                    claim_token: generation.claim_token,
                    expected_version: generation.run.version,
                    outcome: LocalAgentStepOutcome::Succeed {
                        output: json!({"content": markdown}),
                    },
                }),
            ))
            .await
            .unwrap();
        let HostResult::RequirementPlan { plan: Some(plan) } = runtime
            .try_handle(envelope(
                "get-proposal",
                HostCommand::GetRequirementPlan(
                    chatos_local_agent_protocol::GetRequirementPlanCommand {
                        owner_user_id: "user-1".into(),
                        survey_id: survey_id.clone(),
                        revision: None,
                    },
                ),
            ))
            .await
            .unwrap()
        else {
            panic!("proposal")
        };
        assert_eq!(plan.markdown, markdown);
        assert_eq!(plan.revision, 1);
        assert!(plan.confirmed_at_unix_ms.is_none());
        let HostResult::RequirementPlan {
            plan: Some(confirmed),
        } = runtime
            .try_handle(envelope(
                "confirm-proposal",
                HostCommand::ConfirmRequirementPlan(confirm),
            ))
            .await
            .unwrap()
        else {
            panic!("confirmed proposal")
        };
        assert!(confirmed.confirmed_at_unix_ms.is_some());
        assert_eq!(confirmed.markdown, markdown);
        assert!(!requirement_survey_model_tools()
            .iter()
            .any(|tool| tool["name"]
                .as_str()
                .is_some_and(|name| name.contains("confirm"))));
        if main_chat {
            assert_eq!(resolution.resumed_run.run_id, "run-survey-flow");
            assert_eq!(resolution.resumed_run.profile_key, "main_chat");
            let HostResult::ConversationHistory { page: conversation } = runtime
                .try_handle(envelope(
                    "get-resolved-chat",
                    HostCommand::GetConversationHistory(
                        chatos_local_agent_protocol::GetConversationHistoryCommand {
                            owner_user_id: "user-1".into(),
                            conversation_id: "conversation-survey-flow".into(),
                            before_ordinal: None,
                            limit: 100,
                        },
                    ),
                ))
                .await
                .unwrap()
            else {
                panic!("conversation")
            };
            assert_eq!(
                conversation.messages.len(),
                3,
                "submission updates the original notice, generation adds one final reply"
            );
            assert_eq!(conversation.messages[1].content["status"], "resolved");
        }
    }
}
