// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_protocol::{
    ClaimNextRunCommand, CommitStepCommand, CreateConversationCommand,
    CreateRequirementSurveyCommand, CreateRunCommand, HostCommand, HostRequestEnvelope, HostResult,
    ListRequirementSurveysCommand, LocalAgentRunStatus, LocalAgentStepOutcome,
    LocalConversationResourceBinding, LocalConversationResourceKind,
    LocalRequirementSurveyQuestion, LocalRequirementSurveyResponseKind,
    LocalRequirementSurveyStatus, ResolveRequirementSurveyCommand, LOCAL_AGENT_PROTOCOL_VERSION,
};
use serde_json::json;
use std::{collections::BTreeMap, sync::Arc};

fn request(command_id: &str, command: HostCommand) -> HostRequestEnvelope {
    HostRequestEnvelope {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        command_id: command_id.to_string(),
        command,
    }
}

#[tokio::test]
async fn resolves_project_survey_and_atomically_resumes_waiting_task_run() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
    runtime
        .try_handle(request(
            "create-conversation",
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
        .try_handle(request(
            "create-run",
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
    let claimed = runtime
        .try_handle(request(
            "claim-run",
            HostCommand::ClaimNextRun(ClaimNextRunCommand {
                owner_user_id: "user-1".to_string(),
                worker_id: "worker-1".to_string(),
                lease_duration_ms: 30_000,
            }),
        ))
        .await
        .expect("claim");
    let HostResult::Claim { claim: Some(claim) } = claimed else {
        panic!("expected run claim");
    };
    runtime
        .try_handle(request(
            "wait-user",
            HostCommand::CommitStep(CommitStepCommand {
                owner_user_id: "user-1".to_string(),
                run_id: "run-1".to_string(),
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                outcome: LocalAgentStepOutcome::WaitForUser {
                    prompt: json!({"survey_id": "survey-1"}),
                    checkpoint: json!({"phase": "requirements"}),
                },
            }),
        ))
        .await
        .expect("wait for user");
    let question = LocalRequirementSurveyQuestion {
        question_id: "deployment".to_string(),
        prompt: "Where should this run?".to_string(),
        response_kind: LocalRequirementSurveyResponseKind::SingleChoice,
        required: true,
        options: vec!["local".to_string(), "cloud".to_string()],
    };
    runtime
        .try_handle(request(
            "create-survey",
            HostCommand::CreateRequirementSurvey(CreateRequirementSurveyCommand {
                survey_id: "survey-1".to_string(),
                owner_user_id: "user-1".to_string(),
                project_resource_id: "project-1".to_string(),
                source_conversation_id: "conversation-1".to_string(),
                source_run_id: "run-1".to_string(),
                source_task_id: Some("task-1".to_string()),
                title: "Deployment requirements".to_string(),
                description: None,
                questions: vec![question],
            }),
        ))
        .await
        .expect("survey");
    let listed = runtime
        .try_handle(request(
            "list-surveys",
            HostCommand::ListRequirementSurveys(ListRequirementSurveysCommand {
                owner_user_id: "user-1".to_string(),
                project_resource_id: Some("project-1".to_string()),
                status: Some(LocalRequirementSurveyStatus::Open),
                limit: 20,
            }),
        ))
        .await
        .expect("list");
    assert!(matches!(
        listed,
        HostResult::RequirementSurveys { surveys } if surveys.len() == 1
    ));
    let answers = BTreeMap::from([("deployment".to_string(), json!("local"))]);
    let resolved = runtime
        .try_handle(request(
            "resolve-survey",
            HostCommand::ResolveRequirementSurvey(ResolveRequirementSurveyCommand {
                owner_user_id: "user-1".to_string(),
                survey_id: "survey-1".to_string(),
                expected_version: 1,
                answers,
            }),
        ))
        .await
        .expect("resolve");
    let HostResult::RequirementSurveyResolved { resolution } = resolved else {
        panic!("expected resolved survey");
    };
    assert_eq!(
        resolution.survey.status,
        LocalRequirementSurveyStatus::Resolved
    );
    assert_eq!(resolution.survey.version, 2);
    assert_eq!(
        resolution.resumed_run.status,
        LocalAgentRunStatus::ContinuationReady
    );
    assert_eq!(
        resolution.resumed_run.continuation_input.as_ref().unwrap()["survey_id"],
        "survey-1"
    );
}
