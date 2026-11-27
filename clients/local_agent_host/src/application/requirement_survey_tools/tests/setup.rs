use super::*;
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_protocol::{
    ClaimNextRunCommand, ClaimNextToolCommand, CommitStepCommand, CreateConversationCommand,
    CreateRunCommand, LocalAgentStepOutcome, LocalAgentToolCall, LocalAgentToolClaim,
    LocalConversationResourceBinding, LocalConversationResourceKind, StartConversationTurnCommand,
};

pub(super) struct SurveyFlowSetup {
    pub(super) runtime: Arc<LocalAgentRuntime>,
    pub(super) conversation_id: &'static str,
    pub(super) tool_claim: LocalAgentToolClaim,
}

pub(super) async fn prepare_survey_flow(
    main_chat: bool,
    project_bound: bool,
    mixed_batch: bool,
    team_todo: bool,
) -> Result<SurveyFlowSetup, String> {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = Arc::new(LocalAgentRuntime::new(storage));
    let conversation_id = if team_todo {
        "agent-team-executor-delivery-1"
    } else {
        "conversation-survey-flow"
    };
    runtime
        .try_handle(envelope(
            "conversation",
            HostCommand::CreateConversation(CreateConversationCommand {
                conversation_id: conversation_id.to_string(),
                owner_user_id: "user-1".to_string(),
                title: "Project".to_string(),
                resource: (!team_todo).then_some(LocalConversationResourceBinding {
                    kind: if project_bound {
                        LocalConversationResourceKind::Project
                    } else {
                        LocalConversationResourceKind::Contact
                    },
                    resource_id: if project_bound {
                        "project-1".to_string()
                    } else {
                        "jiguli".to_string()
                    },
                }),
            }),
        ))
        .await
        .expect("conversation");
    runtime
            .try_handle(envelope(
                "run",
                if main_chat || team_todo {
                    HostCommand::StartConversationTurn(StartConversationTurnCommand {
                        owner_user_id: "user-1".into(),
                        conversation_id: conversation_id.into(),
                        expected_conversation_version: 1,
                        conversation_if_missing: None,
                        turn_id: "turn-survey-flow".into(),
                        message_id: "message-survey-flow".into(),
                        run_id: "run-survey-flow".into(),
                        message: "understand my goals first".into(),
                        message_metadata: if team_todo {
                            json!({
                                "source": "native_agent_team",
                                "agent_execution_context": {
                                    "schema_version": 2,
                                    "room_id": "team-1",
                                    "agent_id": "agent-1",
                                    "agent_name": "产品经理",
                                    "role_prompt": "负责项目需求。",
                                    "profession_key": "project_manager",
                                    "profession_label": "项目经理",
                                    "profession_skill_ref": "AS-profession-project_manager-123456789abc",
                                    "profession_skill_sha256": "a".repeat(64),
                                    "profession_skill_markdown": "# 工作流\n先调研，再规划。",
                                    "default_plugin_ids": [],
                                    "default_skill_ids": [],
                                    "project_id": "project-1",
                                    "team_name": "Alpha 团队",
                                    "team_goal": "交付 Alpha",
                                    "team_role": "项目经理",
                                    "team_responsibility": "负责需求与交付",
                                    "is_project_manager": true
                                },
                                "agent_team_execution": {
                                    "schema_version": 1,
                                    "delivery_lane": "executor",
                                    "trigger_kind": "todo",
                                    "delivery_id": "delivery-1",
                                    "todo_id": "todo-1",
                                    "team_id": "team-1",
                                    "agent_id": "agent-1",
                                    "project_id": "project-1",
                                    "execution_plan": {
                                        "requires_execution": true,
                                        "builtin_capabilities": [
                                            "requirement_survey_read",
                                            "requirement_survey_write"
                                        ],
                                        "plugins": []
                                    }
                                }
                            })
                        } else {
                            json!({
                                "agent_id": "jiguli",
                                "agent_name": "叽咕狸",
                            })
                        },
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
                            "source_conversation_id": conversation_id,
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
    wait_result.map_err(|error| error.to_string())?;
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
    Ok(SurveyFlowSetup {
        runtime,
        conversation_id,
        tool_claim,
    })
}
