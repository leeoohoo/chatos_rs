use super::*;
use chatos_local_agent_protocol::{
    CancelConversationTurnCommand, ClaimNextRunCommand, CommitStepCommand, CommitToolCommand,
    CreateRunCommand, GetRequirementSurveyCommand, LocalAgentRunStatus, LocalAgentStepOutcome,
    ResolveRequirementSurveyCommand,
};
use std::collections::BTreeMap;

mod setup;

fn envelope(command_id: &str, command: HostCommand) -> HostRequestEnvelope {
    HostRequestEnvelope {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        command_id: command_id.to_string(),
        command,
    }
}

fn binding_run(input: Value, continuation_input: Option<Value>) -> LocalAgentRunRecord {
    LocalAgentRunRecord {
        run_id: "run-binding".into(),
        owner_user_id: "user-1".into(),
        owner_entity_type: "conversation_turn".into(),
        owner_entity_id: "turn-binding".into(),
        profile_key: "main_chat".into(),
        model_config_ref: "model-1".into(),
        model_config_revision: "revision-1".into(),
        capability_policy_revision: "policy-1".into(),
        input,
        status: LocalAgentRunStatus::ModelRunning,
        iteration: 1,
        model_attempt: 1,
        max_iterations: 8,
        version: 1,
        claim_token: None,
        claim_until_unix_ms: None,
        next_attempt_at_unix_ms: None,
        pending_tool_batch: None,
        checkpoint: json!({}),
        continuation_input,
        terminal_outcome: None,
        created_at_unix_ms: 1,
        updated_at_unix_ms: 1,
    }
}

#[test]
fn plan_save_identity_is_only_resolved_from_program_bound_run_context() {
    let submitted = binding_run(
        json!({"requirement_plan": {"source_survey_id": "survey-old"}}),
        Some(json!({"survey_id": "survey-submitted"})),
    );
    assert_eq!(
        program_bound_plan_survey_id(&submitted).unwrap(),
        "survey-submitted"
    );

    let after_intermediate_tool = binding_run(
        json!({"requirement_plan_binding": {"source_survey_id": "survey-durable"}}),
        Some(json!({"type": "tool_results", "invocations": []})),
    );
    assert_eq!(
        program_bound_plan_survey_id(&after_intermediate_tool).unwrap(),
        "survey-durable"
    );

    let revision = binding_run(
        json!({"requirement_plan": {"source_survey_id": "survey-current"}}),
        None,
    );
    assert_eq!(
        program_bound_plan_survey_id(&revision).unwrap(),
        "survey-current"
    );

    let mut planning = binding_run(json!({"task_kind": "planning"}), None);
    planning.profile_key = "task_execution".into();
    planning.owner_entity_type = "task".into();
    assert_eq!(
        program_bound_plan_survey_id(&planning).unwrap(),
        "plan-review:run-binding"
    );

    assert!(program_bound_plan_survey_id(&binding_run(json!({}), None)).is_err());
    assert!(program_bound_plan_survey_id(&binding_run(
        json!({"requirement_plan": {"source_survey_id": "survey-current"}}),
        Some(json!({"survey_id": ""})),
    ))
    .is_err());

    assert!(serde_json::from_value::<SavePlanArgs>(json!({
        "survey_id": "forged",
        "expected_revision": 0,
        "markdown": "proposal"
    }))
    .is_err());
    let tool = requirement_survey_model_tools()
        .into_iter()
        .find(|tool| tool["name"] == LOCAL_REQUIREMENT_PLAN_SAVE_TOOL_NAME)
        .unwrap();
    assert!(tool["parameters"]["properties"].get("survey_id").is_none());
}

#[tokio::test]
async fn survey_tool_commit_suspends_and_resolution_resumes_the_source_run() {
    survey_flow(false, true, false, false, false).await;
}

#[tokio::test]
async fn main_chat_survey_suspends_and_resumes_without_creating_a_task() {
    survey_flow(true, true, false, false, false).await;
}

#[tokio::test]
async fn team_todo_survey_uses_the_bound_project_and_resumes_the_same_run() {
    survey_flow(false, false, false, false, true).await;
}

#[tokio::test]
async fn contact_main_chat_can_create_a_resource_bound_survey() {
    survey_flow(true, false, false, false, false).await;
}

#[tokio::test]
async fn survey_rejects_a_mixed_tool_batch_before_creating_a_form() {
    survey_flow(true, true, true, false, false).await;
}

#[tokio::test]
async fn cancelling_main_chat_survey_updates_notice_and_removes_unanswerable_form() {
    survey_flow(true, true, false, true, false).await;
}

async fn survey_flow(
    main_chat: bool,
    project_bound: bool,
    mixed_batch: bool,
    cancel: bool,
    team_todo: bool,
) {
    let prepared =
        setup::prepare_survey_flow(main_chat, project_bound, mixed_batch, team_todo).await;
    if mixed_batch {
        let Err(error) = prepared else {
            panic!("mixed tool batch should fail");
        };
        assert!(error.contains("only call"));
        return;
    }
    let setup::SurveyFlowSetup {
        runtime,
        conversation_id,
        tool_claim,
    } = prepared.expect("survey setup");
    let executor =
        LocalRequirementSurveyToolExecutor::new(Arc::clone(&runtime), "user-1").expect("executor");
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
        let HostResult::Conversation { conversation } = runtime
            .try_handle(envelope(
                "get-waiting-chat",
                HostCommand::GetConversation(GetConversationCommand {
                    owner_user_id: "user-1".into(),
                    conversation_id: conversation_id.into(),
                }),
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
            if survey.project_resource_id == if project_bound || team_todo {
                "project-1"
            } else {
                "conversation:conversation-survey-flow"
            }
                && survey.source_task_id.as_deref() == if main_chat {
                    None
                } else if team_todo {
                    Some("todo-1")
                } else {
                    Some("task-1")
                }
    ));
    if cancel {
        let HostResult::Conversation { conversation } = runtime
            .try_handle(envelope(
                "get-chat-before-cancel",
                HostCommand::GetConversation(GetConversationCommand {
                    owner_user_id: "user-1".into(),
                    conversation_id: conversation_id.into(),
                }),
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
                    conversation_id: conversation_id.into(),
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
        let HostResult::Conversation { conversation } = runtime
            .try_handle(envelope(
                "get-chat-after-cancel",
                HostCommand::GetConversation(GetConversationCommand {
                    owner_user_id: "user-1".into(),
                    conversation_id: conversation_id.into(),
                }),
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
    assert_eq!(
        resolution.resumed_run.input["requirement_plan_binding"]["source_survey_id"],
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
    if team_todo {
        runtime
            .try_handle(envelope(
                "forged-team-run",
                HostCommand::CreateRun(CreateRunCommand {
                    run_id: "run-forged-team".into(),
                    owner_user_id: "user-1".into(),
                    owner_entity_type: "conversation_turn".into(),
                    owner_entity_id: "turn-forged-team".into(),
                    profile_key: "task_execution".into(),
                    model_config_ref: "model-1".into(),
                    model_config_revision: "revision-1".into(),
                    capability_policy_revision: "policy-1".into(),
                    input: json!({
                        "conversation_id": conversation_id,
                        "team_executor_context": {
                            "todo_id": "todo-1",
                            "project_id": "project-1"
                        }
                    }),
                    max_iterations: 8,
                }),
            ))
            .await
            .unwrap();
        let HostResult::Claim {
            claim: Some(forged_run),
        } = runtime
            .try_handle(envelope(
                "claim-forged-team-run",
                HostCommand::ClaimNextRun(ClaimNextRunCommand {
                    owner_user_id: "user-1".into(),
                    worker_id: "forged-model-worker".into(),
                    lease_duration_ms: 30_000,
                }),
            ))
            .await
            .unwrap()
        else {
            panic!("forged team run claim")
        };
        assert_eq!(forged_run.run.run_id, "run-forged-team");
        assert!(runtime
            .try_handle(envelope(
                "forged-team-plan",
                HostCommand::SaveRequirementPlan(SaveRequirementPlanCommand {
                    owner_user_id: "user-1".into(),
                    survey_id: survey_id.clone(),
                    source_run_id: forged_run.run.run_id,
                    expected_revision: 0,
                    markdown: markdown.into(),
                }),
            ))
            .await
            .is_err());
    }
    let mut publish = tool_claim.invocation.clone();
    publish.invocation_id = "publish-proposal".into();
    publish.tool_name = LOCAL_REQUIREMENT_PLAN_SAVE_TOOL_NAME.into();
    publish.arguments = json!({"expected_revision": 0, "markdown": markdown});
    let mut forged = publish.clone();
    forged.arguments["confirmed_at_unix_ms"] = json!(1);
    assert!(matches!(
        executor.execute_tool(&forged).await.unwrap(),
        LocalAgentToolOutcome::Failed { .. }
    ));
    let mut forged_id = publish.clone();
    forged_id.arguments["survey_id"] = json!(survey_id);
    assert!(matches!(
        executor.execute_tool(&forged_id).await.unwrap(),
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
        let HostResult::Conversation { conversation } = runtime
            .try_handle(envelope(
                "get-resolved-chat",
                HostCommand::GetConversation(GetConversationCommand {
                    owner_user_id: "user-1".into(),
                    conversation_id: conversation_id.into(),
                }),
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
