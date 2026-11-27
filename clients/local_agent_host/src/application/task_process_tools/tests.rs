use super::*;
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_protocol::{
    ClaimNextRunCommand, ClaimNextToolCommand, CommitStepCommand, CommitToolCommand,
    CreateRunCommand, HostCommand, HostRequestEnvelope, HostResult, LocalAgentStepOutcome,
    LocalAgentToolApprovalStatus, LocalAgentToolCall, LocalAgentToolStatus,
    LOCAL_AGENT_PROTOCOL_VERSION,
};

#[test]
fn definitions_preserve_tool_names_and_expose_structured_acceptance() {
    let tools = task_process_model_tools();
    assert_eq!(tools[0]["name"], TASK_PROCESS_ALLOCATE_SERVICE_TOOL);
    assert_eq!(tools[1]["name"], TASK_PROCESS_RECORD_TOOL);
    assert_eq!(tools[2]["name"], TASK_OUTCOME_REPORT_TOOL);
    assert_eq!(
        tools[2]["parameters"]["properties"]["acceptance_checks"]["maxItems"],
        50
    );
    assert_eq!(
        tools[2]["parameters"]["properties"]["final_response"]["maxLength"],
        32000
    );
    assert!(
        tools[2]["parameters"]["properties"]["result_status"]["description"]
            .as_str()
            .expect("result_status description")
            .contains("does not terminate the Run")
    );
    assert_eq!(
        tools[2]["parameters"]["properties"]["verification_report"]["properties"]["checks"]
            ["maxItems"],
        50
    );
    let prompt = task_process_prompt_item().to_string();
    assert!(prompt.contains("meaningful milestones"));
    assert!(prompt.contains("If any actionable check is failed or not_verified, keep working"));
    assert!(prompt.contains(TASK_PROCESS_ALLOCATE_SERVICE_TOOL));
    assert!(prompt.contains("final tool call"));
}

#[test]
fn service_registry_avoids_busy_and_run_reserved_ports() {
    let occupied =
        TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).expect("occupied listener");
    let occupied_port = occupied.local_addr().unwrap().port();
    let mut registry = RunServicePortRegistry::default();
    let first = registry
        .allocate("run-1", "backend", Some(occupied_port), "http")
        .expect("first allocation");
    assert_ne!(first.port, occupied_port);
    let same = registry
        .allocate("run-1", "backend", Some(18080), "http")
        .expect("idempotent allocation");
    assert_eq!(same, first);
    let second = registry
        .allocate("run-1", "frontend", Some(first.port), "http")
        .expect("second allocation");
    assert_ne!(second.port, first.port);
    assert_eq!(registry.release("run-1", "backend"), Some(first));
    assert!(registry.release("run-1", "backend").is_none());
}

async fn runtime_with_run(profile_key: &str, owner_entity_type: &str) -> Arc<LocalAgentRuntime> {
    runtime_with_input(profile_key, owner_entity_type, json!({"prompt":"do work"})).await
}

async fn runtime_with_input(
    profile_key: &str,
    owner_entity_type: &str,
    input: Value,
) -> Arc<LocalAgentRuntime> {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = Arc::new(LocalAgentRuntime::new(storage));
    runtime.initialize("user-1").await.expect("initialize");
    runtime
        .try_handle(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "create-run".to_string(),
            command: HostCommand::CreateRun(CreateRunCommand {
                run_id: "run-1".to_string(),
                owner_user_id: "user-1".to_string(),
                owner_entity_type: owner_entity_type.to_string(),
                owner_entity_id: "task-1".to_string(),
                profile_key: profile_key.to_string(),
                model_config_ref: "model-1".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                input,
                max_iterations: 8,
            }),
        })
        .await
        .expect("create run");
    runtime
}

fn invocation(tool_name: &str, arguments: Value) -> LocalAgentToolInvocationRecord {
    LocalAgentToolInvocationRecord {
        invocation_id: "invocation-1".to_string(),
        run_id: "run-1".to_string(),
        batch_id: "batch-1".to_string(),
        call_id: "call-1".to_string(),
        tool_name: tool_name.to_string(),
        execution_route: chatos_local_agent_protocol::LocalAgentToolExecutionRoute::Shared,
        arguments,
        side_effecting: false,
        requires_approval: false,
        approval_status: LocalAgentToolApprovalStatus::NotRequired,
        approval_decided_by: None,
        approval_reason: None,
        approval_decided_at_unix_ms: None,
        status: LocalAgentToolStatus::Running,
        result: None,
        error: None,
        version: 2,
        claim_token: Some("claim-1".to_string()),
        claim_until_unix_ms: Some(i64::MAX),
        created_at_unix_ms: 1,
        updated_at_unix_ms: 1,
    }
}

#[tokio::test]
async fn process_tools_execute_only_for_task_owned_task_execution_runs() {
    let executor = LocalTaskProcessToolExecutor::new(
        runtime_with_run("task_execution", "task").await,
        "user-1",
    )
    .expect("executor");
    let outcome = executor
        .execute_tool(&invocation(
            TASK_PROCESS_RECORD_TOOL,
            json!({"operation": "append", "heading": "Verify", "content": "Tests pass"}),
        ))
        .await
        .expect("record process");
    let LocalAgentToolOutcome::Succeeded { output } = outcome else {
        panic!("expected successful process record")
    };
    assert_eq!(output["recorded"], true);

    let wrong_scope = LocalTaskProcessToolExecutor::new(
        runtime_with_run("main_chat", "conversation_turn").await,
        "user-1",
    )
    .expect("executor");
    let error = wrong_scope
        .execute_tool(&invocation(
            TASK_OUTCOME_REPORT_TOOL,
            json!({"status": "succeeded", "reason": "done"}),
        ))
        .await
        .expect_err("Main Chat must not use Task process tools");
    assert!(error.contains("active Task execution"));
}

#[tokio::test]
async fn process_updates_validate_and_return_live_acceptance_checks() {
    let executor = LocalTaskProcessToolExecutor::new(
        runtime_with_input(
            "task_execution",
            "task",
            json!({
                "prompt": "do work",
                "acceptance_criteria": ["Saved", "Verified"]
            }),
        )
        .await,
        "user-1",
    )
    .unwrap();
    let outcome = executor
        .execute_tool(&invocation(
            TASK_PROCESS_RECORD_TOOL,
            json!({
                "acceptance_checks": [
                    {"criterion_index":0,"status":"passed","evidence":"Saved row exists"},
                    {"criterion_index":1,"status":"in_progress"}
                ]
            }),
        ))
        .await
        .expect("record live acceptance progress");
    let LocalAgentToolOutcome::Succeeded { output } = outcome else {
        panic!("expected successful progress update")
    };
    assert_eq!(output["acceptance_checks"][0]["status"], "passed");
    assert_eq!(output["acceptance_checks"][1]["status"], "in_progress");

    let error = executor
        .execute_tool(&invocation(
            TASK_PROCESS_RECORD_TOOL,
            json!({"acceptance_checks":[{"criterion_index":1,"status":"failed"}]}),
        ))
        .await
        .expect_err("failed progress requires evidence");
    assert!(error.contains("requires concrete evidence"));
}

#[tokio::test]
async fn incomplete_outcome_report_can_be_corrected_through_the_real_scheduler() {
    use crate::{LocalToolRegistry, LocalToolScheduler, ToolSchedulerTick};
    use chatos_local_agent_protocol::{
        ClaimNextRunCommand, CommitStepCommand, HostResult, LocalAgentRunStatus,
        LocalAgentStepOutcome, LocalAgentToolCall,
    };
    let runtime = runtime_with_input("task_execution", "task", json!({
            "acceptance_report_version":1,
            "acceptance_criteria":["target", "services", "read only", "redaction", "no mutation", "evidence"]
        })).await;
    let mut tools = LocalToolRegistry::new();
    tools
        .register(
            TASK_OUTCOME_REPORT_TOOL,
            LocalTaskProcessToolExecutor::new(runtime.clone(), "user-1").unwrap(),
        )
        .unwrap();
    let scheduler =
        LocalToolScheduler::new(runtime.clone(), tools, "user-1", "test-worker").unwrap();
    for count in [5, 6] {
        let request = |id: String, command| HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: id,
            command,
        };
        let HostResult::Claim { claim: Some(claim) } = runtime
            .try_handle(request(
                format!("claim-{count}"),
                HostCommand::ClaimNextRun(ClaimNextRunCommand {
                    owner_user_id: "user-1".into(),
                    worker_id: "model".into(),
                    lease_duration_ms: 10_000,
                }),
            ))
            .await
            .unwrap()
        else {
            panic!("model must be able to continue")
        };
        let checks = (0..count).map(|i| json!({"criterion_index":i,"status":if i==3 {"failed"} else {"passed"},"evidence":"actual finding"})).collect::<Vec<_>>();
        runtime.try_handle(request(format!("step-{count}"), HostCommand::CommitStep(CommitStepCommand {
                owner_user_id:"user-1".into(), run_id:claim.run.run_id, claim_token:claim.claim_token, expected_version:claim.run.version,
                outcome:LocalAgentStepOutcome::WaitForTool {
                    batch_id:format!("batch-{count}"), checkpoint:json!({}),
                    tool_calls:vec![LocalAgentToolCall {
                        call_id:format!("call-{count}"), tool_name:TASK_OUTCOME_REPORT_TOOL.into(),
                        arguments:json!({"result_status":"failed", "reason":"A business criterion failed", "final_response":"Actual findings with an unmet criterion", "acceptance_checks":checks}),
                        side_effecting:true, requires_approval:false
                    }]
                }
            }))).await.unwrap();
        let ToolSchedulerTick::Committed(result) = scheduler.run_once().await.unwrap() else {
            panic!("expected report execution")
        };
        assert_eq!(result.run.status, LocalAgentRunStatus::ContinuationReady);
        if count == 5 {
            assert_eq!(result.invocation.status, LocalAgentToolStatus::Failed);
            assert!(result.invocation.error.unwrap().contains("all 6 criteria"));
            let repair = result.invocation.result.unwrap();
            assert_eq!(repair["retryable"], true);
            assert_eq!(
                repair["task_acceptance_contract"]["acceptance_criteria"],
                json!([
                    "target",
                    "services",
                    "read only",
                    "redaction",
                    "no mutation",
                    "evidence"
                ])
            );
        } else {
            assert_eq!(result.invocation.status, LocalAgentToolStatus::Succeeded);
            assert_eq!(result.invocation.result.unwrap()["result_status"], "failed");
        }
    }
}

#[tokio::test]
async fn outcome_executor_separates_unverified_acceptance_from_execution_and_enforces_limit() {
    let executor = LocalTaskProcessToolExecutor::new(runtime_with_input("task_execution","task",json!({"acceptance_report_version":1,"acceptance_criteria":["Short response"],"final_response_max_chars":4})).await,"user-1").unwrap();
    let mut report = json!({"status":"succeeded","reason":"done","final_response":"中文\n!!","acceptance_checks":[{"criterion_index":0,"status":"passed","evidence":"Checked actual final response"}]});
    let outcome = executor
        .execute_tool(&invocation(TASK_OUTCOME_REPORT_TOOL, report.clone()))
        .await
        .unwrap();
    let LocalAgentToolOutcome::Failed { error, .. } = outcome else {
        panic!("expected correctable validation failure")
    };
    assert!(error.contains("5 characters"));
    report["final_response"] = json!("中文");
    report["acceptance_checks"][0]["status"] = json!("not_verified");
    let LocalAgentToolOutcome::Succeeded { output } = executor
        .execute_tool(&invocation(TASK_OUTCOME_REPORT_TOOL, report.clone()))
        .await
        .expect("completed evaluation may have an unverified finding")
    else {
        panic!("accepted incomplete acceptance report")
    };
    assert_eq!(output["status"], "succeeded");
    assert_eq!(output["acceptance_checks"][0]["status"], "not_verified");
    report["status"] = json!("blocked");
    let LocalAgentToolOutcome::Succeeded { output } = executor
        .execute_tool(&invocation(TASK_OUTCOME_REPORT_TOOL, report.clone()))
        .await
        .unwrap()
    else {
        panic!("accepted blocked report")
    };
    assert_eq!(output["status"], "blocked");
    assert_eq!(output["final_response"], "中文");
    assert_eq!(output["final_response_chars"], 2);
    report["status"] = json!("succeeded");
    report["acceptance_checks"][0]["status"] = json!("passed");
    assert!(executor
        .execute_tool(&invocation(TASK_OUTCOME_REPORT_TOOL, report))
        .await
        .is_ok());
}

#[tokio::test]
async fn selected_research_plugin_does_not_require_successful_plugin_delivery() {
    let executor = LocalTaskProcessToolExecutor::new(
            runtime_with_input(
                "task_execution",
                "task",
                json!({
                    "acceptance_report_version": 1,
                    "acceptance_criteria": ["Attempt browser research; HTTP fallback may supply the sourced text report"],
                    "tool_options": {
                        "requires_execution": true,
                        "enabled_builtin_kinds": ["TerminalController"],
                        "plugin_hints": [{"plugin_key":"browser-cdp","component_key":"browser-cdp-mcp","reason":"research; fall back to HTTP when unavailable"}]
                    }
                }),
            )
            .await,
            "user-1",
        )
        .unwrap();
    // No successful capability_invoke exists in this Run. The report still must
    // cover the actual criterion and disclose the fallback, not invent an artifact.
    let outcome = executor
            .execute_tool(&invocation(
                TASK_OUTCOME_REPORT_TOOL,
                json!({
                    "status":"succeeded",
                    "reason":"Research delivered using the allowed fallback; screenshot unavailable",
                    "final_response":"HTTP sources and research findings; no visual layout claims.",
                    "acceptance_checks":[{"criterion_index":0,"status":"passed","evidence":"Browser unavailable; HTTP GET returned 200 and report cites the retrieved content"}]
                }),
            ))
            .await
            .expect("allowed fallback must not require a successful Plugin invocation");
    let LocalAgentToolOutcome::Succeeded { output } = outcome else {
        panic!("expected accepted research report")
    };
    assert_eq!(output["status"], "succeeded");
}

#[tokio::test]
async fn completed_plugin_attempt_can_report_an_unverified_deliverable() {
    let executor = LocalTaskProcessToolExecutor::new(
            runtime_with_input(
                "task_execution",
                "task",
                json!({
                    "acceptance_report_version":1,
                    "acceptance_criteria":["Persist and verify the requested plan in Solution Studio"],
                    "tool_options":{"plugin_hints":[{"plugin_key":"solution-studio","component_key":"solution-authoring-mcp","reason":"persist the plan"}]}
                }),
            ).await,
            "user-1",
        ).unwrap();
    let mut report = json!({
        "status":"succeeded",
        "reason":"Only a text draft exists; Plugin persistence unavailable",
        "final_response":"Draft plan; no verified Plugin deliverable.",
        "acceptance_checks":[{"criterion_index":0,"status":"not_verified","evidence":"No persisted plan could be verified in Solution Studio"}]
    });
    let outcome = executor
        .execute_tool(&invocation(TASK_OUTCOME_REPORT_TOOL, report.clone()))
        .await
        .expect("execution completion and acceptance findings are separate");
    let LocalAgentToolOutcome::Succeeded { output } = outcome else {
        panic!("expected accepted report with incomplete acceptance")
    };
    assert_eq!(output["status"], "succeeded");
    assert_eq!(output["acceptance_checks"][0]["status"], "not_verified");
    report["status"] = json!("blocked");
    let outcome = executor
        .execute_tool(&invocation(TASK_OUTCOME_REPORT_TOOL, report))
        .await
        .unwrap();
    let LocalAgentToolOutcome::Succeeded { output } = outcome else {
        panic!("expected accepted blocked report")
    };
    assert_eq!(output["status"], "blocked");
}

#[tokio::test]
async fn blocked_result_and_criterion_are_preserved_for_the_host_gate() {
    let executor = LocalTaskProcessToolExecutor::new(
        runtime_with_input(
            "task_execution",
            "task",
            json!({
                "acceptance_report_version":1,
                "acceptance_criteria":["Inspect the host"]
            }),
        )
        .await,
        "user-1",
    )
    .unwrap();
    let LocalAgentToolOutcome::Succeeded { output } = executor
            .execute_tool(&invocation(
                TASK_OUTCOME_REPORT_TOOL,
                json!({
                    "result_status":"blocked",
                    "reason":"The required host connection is unavailable",
                    "final_response":"No inspection could be performed because the required connection is unavailable.",
                    "acceptance_checks":[
                        {"criterion_index":0,"status":"blocked","evidence":"No authorized connection was available; the user must provide one before host evidence can be collected"}
                    ]
                }),
            ))
            .await
            .expect("the internal result may block downstream work")
        else {
            panic!("expected accepted blocked report")
        };
    assert_eq!(output["result_status"], "blocked");
    assert_eq!(
        output["status"], "blocked",
        "legacy alias must preserve the blocked result"
    );
}

#[tokio::test]
async fn plan_bound_blockers_continue_retry_replan_or_wait_by_classification() {
    let executor = LocalTaskProcessToolExecutor::new(
        runtime_with_input(
            "task_execution",
            "task",
            json!({
                "acceptance_report_version":1,
                "acceptance_criteria":["Deliver the confirmed plan"],
                "requirement_plan":{"snapshot":{"plan_id":"plan-1","revision":1}}
            }),
        )
        .await,
        "user-1",
    )
    .unwrap();
    let base = json!({
        "result_status":"blocked",
        "reason":"The current method cannot complete the confirmed plan",
        "final_response":"Execution needs a recovery decision.",
        "acceptance_checks":[
            {"criterion_index":0,"status":"blocked","evidence":"The current method cannot satisfy the confirmed plan"}
        ]
    });

    let LocalAgentToolOutcome::Failed { error, .. } = executor
        .execute_tool(&invocation(TASK_OUTCOME_REPORT_TOOL, base.clone()))
        .await
        .unwrap()
    else {
        panic!("plan-bound blockers require classification")
    };
    assert!(error.contains("structured blocker"));

    let mut retry = base.clone();
    retry["result_status"] = json!("failed");
    retry["acceptance_checks"][0]["status"] = json!("failed");
    retry["blocker"] = json!({
        "classification":"retry_with_changed_method",
        "evidence":"The first parser rejects this valid legacy format",
        "failure_fingerprint":"legacy-parser-rejection",
        "previous_method":"parse with the strict reader",
        "next_method":"parse with the compatibility reader"
    });
    let LocalAgentToolOutcome::Succeeded { output: detail } = executor
        .execute_tool(&invocation(TASK_OUTCOME_REPORT_TOOL, retry))
        .await
        .unwrap()
    else {
        panic!("changed-method recovery must continue the same Run")
    };
    assert_eq!(detail["phase"], "task_blocker_recovery");
    assert_eq!(detail["classification"], "retry_with_changed_method");

    let mut replan = base.clone();
    replan["blocker"] = json!({
        "classification":"replan_required",
        "evidence":"The confirmed dependency no longer exposes the required API",
        "affected_task_ids":["task-1"]
    });
    let LocalAgentToolOutcome::Succeeded { output } = executor
        .execute_tool(&invocation(TASK_OUTCOME_REPORT_TOOL, replan))
        .await
        .unwrap()
    else {
        panic!("replan blocker must reach graph recovery")
    };
    assert_eq!(output["blocker"]["classification"], "replan_required");

    let mut user = base.clone();
    user["blocker"] = json!({
        "classification":"user_action_required",
        "evidence":"The service requires a credential that is not available to this Task",
        "user_question":"Which approved credential reference should this Task use?"
    });
    let LocalAgentToolOutcome::Succeeded { output } = executor
        .execute_tool(&invocation(TASK_OUTCOME_REPORT_TOOL, user))
        .await
        .unwrap()
    else {
        panic!("genuine user action must remain a classified blocker")
    };
    assert_eq!(output["blocker"]["classification"], "user_action_required");

    let mut unknown = base;
    unknown["blocker"] = json!({
        "classification":"unknown_side_effects_needs_review",
        "evidence":"The model did not receive a response from the external mutation"
    });
    let LocalAgentToolOutcome::Failed { error, .. } = executor
        .execute_tool(&invocation(TASK_OUTCOME_REPORT_TOOL, unknown))
        .await
        .unwrap()
    else {
        panic!("the model cannot claim Host-owned needs_review")
    };
    assert!(error.contains("Only the Host"));
}

#[tokio::test]
async fn requirement_survey_planning_task_cannot_shift_internal_blocker_to_user() {
    let executor = LocalTaskProcessToolExecutor::new(
        runtime_with_input(
            "task_execution",
            "task",
            json!({
                "task_kind":"planning",
                "acceptance_report_version":1,
                "acceptance_criteria":["Publish the plan"],
                "requirement_plan":{"snapshot":{"plan_id":"plan-1","revision":1}},
                "tool_options":{
                    "enabled_builtin_kinds":[
                        "RequirementSurveyRead",
                        "RequirementSurveyWrite"
                    ]
                }
            }),
        )
        .await,
        "user-1",
    )
    .unwrap();
    let LocalAgentToolOutcome::Failed { error, .. } = executor
        .execute_tool(&invocation(
            TASK_OUTCOME_REPORT_TOOL,
            json!({
                "result_status":"blocked",
                "reason":"The declared survey capability was unavailable",
                "final_response":"Please configure the Task capability.",
                "acceptance_checks":[{
                    "criterion_index":0,
                    "status":"blocked",
                    "evidence":"The capability could not be activated"
                }],
                "blocker":{
                    "classification":"user_action_required",
                    "evidence":"The Task capability could not be activated",
                    "user_question":"Please configure the Task capability."
                }
            }),
        ))
        .await
        .unwrap()
    else {
        panic!("planning capability mismatch must not become a user blocker")
    };
    assert!(
        error.contains("classify unavailable or inconsistent Task capabilities as replan_required")
    );
}

#[tokio::test]
async fn changed_method_recovery_does_not_repeat_a_persisted_attempt() {
    let runtime = runtime_with_input(
        "task_execution",
        "task",
        json!({
            "acceptance_report_version":1,
            "acceptance_criteria":["Deliver the confirmed plan"],
            "requirement_plan":{"snapshot":{"plan_id":"plan-1","revision":1}}
        }),
    )
    .await;
    let executor =
        LocalTaskProcessToolExecutor::new(Arc::clone(&runtime), "user-1").expect("executor");
    let report = json!({
        "result_status":"failed",
        "reason":"The strict reader rejects this valid legacy format",
        "final_response":"Execution is continuing with a compatible reader.",
        "acceptance_checks":[{
            "criterion_index":0,
            "status":"failed",
            "evidence":"The strict reader rejected the legacy header"
        }],
        "blocker":{
            "classification":"retry_with_changed_method",
            "evidence":"The strict reader rejects this valid legacy format",
            "failure_fingerprint":"legacy-parser-rejection",
            "previous_method":"parse with the strict reader",
            "next_method":"parse with the compatibility reader"
        }
    });
    let HostResult::Claim { claim: Some(claim) } = runtime
        .try_handle(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "claim-recovery-run".into(),
            command: HostCommand::ClaimNextRun(ClaimNextRunCommand {
                owner_user_id: "user-1".into(),
                worker_id: "model-worker".into(),
                lease_duration_ms: 30_000,
            }),
        })
        .await
        .expect("claim Run")
    else {
        panic!("expected Run claim")
    };
    runtime
        .try_handle(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "queue-recovery-report".into(),
            command: HostCommand::CommitStep(CommitStepCommand {
                owner_user_id: "user-1".into(),
                run_id: claim.run.run_id,
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                outcome: LocalAgentStepOutcome::WaitForTool {
                    batch_id: "recovery-batch".into(),
                    tool_calls: vec![LocalAgentToolCall {
                        call_id: "recovery-call".into(),
                        tool_name: TASK_OUTCOME_REPORT_TOOL.into(),
                        arguments: report.clone(),
                        side_effecting: false,
                        requires_approval: false,
                    }],
                    checkpoint: json!({"phase":"recovering"}),
                },
            }),
        })
        .await
        .expect("queue recovery report");
    let HostResult::ToolClaim {
        claim: Some(tool_claim),
    } = runtime
        .try_handle(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "claim-recovery-report".into(),
            command: HostCommand::ClaimNextTool(ClaimNextToolCommand {
                owner_user_id: "user-1".into(),
                worker_id: "tool-worker".into(),
                lease_duration_ms: 30_000,
                include_tool_names: Some(vec![TASK_OUTCOME_REPORT_TOOL.into()]),
                exclude_tool_names: Vec::new(),
                execution_route: None,
            }),
        })
        .await
        .expect("claim recovery report")
    else {
        panic!("expected Tool claim")
    };
    let first = executor
        .execute_tool(&tool_claim.invocation)
        .await
        .expect("execute recovery report");
    runtime
        .try_handle(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "commit-recovery-report".into(),
            command: HostCommand::CommitTool(CommitToolCommand {
                owner_user_id: "user-1".into(),
                invocation_id: tool_claim.invocation.invocation_id,
                claim_token: tool_claim.claim_token,
                expected_version: tool_claim.invocation.version,
                outcome: first,
            }),
        })
        .await
        .expect("persist recovery attempt");

    let LocalAgentToolOutcome::Failed { error, .. } = executor
        .execute_tool(&invocation(TASK_OUTCOME_REPORT_TOOL, report.clone()))
        .await
        .expect("check duplicate recovery")
    else {
        panic!("the persisted recovery method must not repeat")
    };
    assert!(error.contains("already tried"));

    let mut changed = report;
    changed["blocker"]["next_method"] = json!("parse with the streaming legacy reader");
    assert!(matches!(
        executor
            .execute_tool(&invocation(TASK_OUTCOME_REPORT_TOOL, changed))
            .await
            .expect("check different recovery"),
        LocalAgentToolOutcome::Succeeded { .. }
    ));
}
