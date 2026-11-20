fn task_claim() -> LocalAgentRunClaim {
    LocalAgentRunClaim {
        worker_id: "worker".to_string(),
        claim_token: "token".to_string(),
        run: chatos_local_agent_protocol::LocalAgentRunRecord {
            run_id: "run-1".to_string(),
            owner_user_id: "user-1".to_string(),
            owner_entity_type: "task".to_string(),
            owner_entity_id: "task-1".to_string(),
            profile_key: "task_execution".to_string(),
            model_config_ref: "model-1".to_string(),
            model_config_revision: "revision-1".to_string(),
            capability_policy_revision: "policy-1".to_string(),
            input: json!({"prompt": "do the work"}),
            status: chatos_local_agent_protocol::LocalAgentRunStatus::ModelRunning,
            iteration: 1,
            model_attempt: 1,
            max_iterations: 8,
            version: 2,
            claim_token: Some("token".to_string()),
            claim_until_unix_ms: Some(20_000),
            next_attempt_at_unix_ms: None,
            pending_tool_batch: None,
            checkpoint: Value::Null,
            continuation_input: None,
            terminal_outcome: None,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 2,
        },
    }
}

#[test]
fn task_final_without_reported_outcome_continues_with_required_guidance() {
    let claim = task_claim();

    let outcome = reduce_ai_step_outcome_for_claim(
        AiSingleStepOutcome::Final(response(None)),
        &claim,
        10_000,
        &ConservativeToolSafetyPolicy,
    )
    .expect("reduce Task final");

    let LocalAgentStepOutcome::Continue { checkpoint } = outcome else {
        panic!("Task must continue until its outcome is reported")
    };
    assert_eq!(
        checkpoint["continuation"]["reason"],
        "task_outcome_required"
    );
    assert!(checkpoint["continuation"]["input_items"][0]["content"]
        .as_str()
        .is_some_and(|text| text.contains("[Task Outcome Required]")));
}

#[test]
fn successful_task_outcome_report_is_detected() {
    assert!(task_outcome_reported(Some(&json!({
        "type": "tool_results",
        "invocations": [{
            "tool_name": "task_run_process_report_outcome",
            "status": "succeeded",
            "result": {"reported": true}
        }]
    }))));
}

#[test]
fn incomplete_task_does_not_request_terminal_memory_summary() {
    let claim = task_claim();

    assert!(!crate::memory::terminal_memory_summary_requested(&claim));
}

#[test]
fn terminal_task_requests_post_sync_memory_summary() {
    let mut claim = task_claim();
    claim.run.continuation_input = Some(json!({
        "type": "tool_results",
        "invocations": [{
            "tool_name": "task_run_process_report_outcome",
            "status": "succeeded",
            "result": {"reported": true}
        }]
    }));

    assert!(crate::memory::terminal_memory_summary_requested(&claim));
}

#[test]
fn unfinished_team_todo_attempt_does_not_request_memory_summary() {
    let mut claim = task_claim();
    claim.run.owner_entity_type = "conversation_turn".to_string();
    claim.run.owner_entity_id = "todo-1".to_string();

    assert!(!crate::memory::terminal_memory_summary_requested(&claim));
}

#[test]
fn completed_team_todo_requests_post_sync_memory_summary() {
    let mut claim = task_claim();
    claim.run.owner_entity_type = "conversation_turn".to_string();
    claim.run.owner_entity_id = "todo-1".to_string();
    claim.run.continuation_input = Some(json!({
        "type": "tool_results",
        "invocations": [{
            "tool_name": "todo_complete",
            "status": "succeeded",
            "result": {"completed": true}
        }]
    }));

    assert!(crate::memory::terminal_memory_summary_requested(&claim));
}

#[test]
fn blocked_team_todo_requests_post_sync_memory_summary() {
    let mut claim = task_claim();
    claim.run.owner_entity_type = "conversation_turn".to_string();
    claim.run.owner_entity_id = "todo-1".to_string();
    claim.run.continuation_input = Some(json!({
        "type": "tool_results",
        "invocations": [{
            "tool_name": "todo_block",
            "status": "succeeded",
            "result": {"blocked": true}
        }]
    }));

    assert!(crate::memory::terminal_memory_summary_requested(&claim));
}

#[test]
fn stable_agent_never_requests_memory_summary_per_turn() {
    let mut claim = task_claim();
    claim.run.profile_key = "main_chat".to_string();
    claim.run.owner_entity_type = "conversation_turn".to_string();

    assert!(!crate::memory::terminal_memory_summary_requested(&claim));
}

#[test]
fn structured_final_delivers_checked_text_not_a_later_model_rewrite() {
    let mut claim = task_claim();
    claim.run.input = json!({"acceptance_report_version":1,"acceptance_criteria":["Evidence-based review"],"final_response_max_chars":100});
    let report = json!({"reported":true,"status":"succeeded","reason":"Review completed; artifact still needs repairs","final_response":"Review complete: the artifact fails its content requirements.","acceptance_checks":[{"criterion_index":0,"status":"passed","evidence":"Compared the artifact against the requested criteria"}]});
    claim.run.continuation_input = Some(
        json!({"invocations":[{"tool_name":"task_run_process_report_outcome","status":"succeeded","result":report}]}),
    );
    let mut rewritten = response(None);
    rewritten.content = "x".repeat(500);
    let outcome = reduce_ai_step_outcome_for_claim(
        AiSingleStepOutcome::Final(rewritten),
        &claim,
        10000,
        &ConservativeToolSafetyPolicy,
    )
    .unwrap();
    let LocalAgentStepOutcome::Succeed { output } = outcome else {
        panic!("expected completed review")
    };
    assert_eq!(output["content"], report["final_response"]);
    assert_eq!(output["task_acceptance"]["status"], "succeeded");
    claim.run.continuation_input = None;
    claim.run.checkpoint = json!({
        "response": {
            "content": report["final_response"],
            "task_acceptance": report,
            "request_input_items": [],
            "response_output_items": []
        },
        "continuation": {
            "reason": "trusted_plugin_hook_stop_block",
            "input_items": [],
            "stop_hook_block": {
                "count": 1,
                "reason_sha256": "a".repeat(64),
                "event": "SubagentStop"
            }
        }
    });
    let LocalAgentStepOutcome::Succeed { output } = reduce_ai_step_outcome_for_claim(
        AiSingleStepOutcome::Final(response(None)),
        &claim,
        10_000,
        &ConservativeToolSafetyPolicy,
    )
    .expect("retained acceptance must survive SubagentStop") else {
        panic!("retained acceptance should allow final completion")
    };
    assert_eq!(output["content"], report["final_response"]);
    // Failed or unverified criteria are preserved and automatically returned
    // to the same model instead of allowing a summary to end the Task.
    for verdict in ["failed", "not_verified"] {
        claim.run.checkpoint["response"]["task_acceptance"]["acceptance_checks"][0]["status"] =
            json!(verdict);
        let LocalAgentStepOutcome::Continue { checkpoint } = reduce_ai_step_outcome_for_claim(
            AiSingleStepOutcome::Final(response(None)),
            &claim,
            10000,
            &ConservativeToolSafetyPolicy,
        )
        .expect("incomplete acceptance must continue the Run") else {
            panic!("expected automatic continuation for incomplete acceptance")
        };
        assert_eq!(
            checkpoint["continuation"]["reason"],
            "task_acceptance_incomplete"
        );
        let guidance = checkpoint["continuation"]["input_items"][0]["content"]
            .as_str()
            .expect("acceptance continuation guidance");
        assert!(guidance.contains("actionable_acceptance_checks"));
        assert!(guidance.contains(verdict));
    }
    claim.run.checkpoint["response"]["task_acceptance"] = json!({
        "reported": true,
        "result_status": "blocked",
        "reason": "Waiting for user credentials",
        "final_response": "Blocked pending credentials.",
        "acceptance_checks": [
            {"criterion_index":0,"status":"blocked","evidence":"A required credential must be supplied by the user"}
        ]
    });
    let LocalAgentStepOutcome::Succeed { output } = reduce_ai_step_outcome_for_claim(
        AiSingleStepOutcome::Final(response(None)),
        &claim,
        10_000,
        &ConservativeToolSafetyPolicy,
    )
    .expect("a fully resolved blocked report must stop automatic continuation") else {
        panic!("expected blocked Task report to become terminal")
    };
    assert_eq!(output["task_acceptance"]["result_status"], "blocked");
    // Malformed reports still fail validation rather than bypassing it.
    claim.run.checkpoint["response"]["task_acceptance"]["acceptance_checks"][0]["status"] =
        json!("unknown-verdict");
    let error = reduce_ai_step_outcome_for_claim(
        AiSingleStepOutcome::Final(response(None)),
        &claim,
        10000,
        &ConservativeToolSafetyPolicy,
    )
    .unwrap_err();
    assert!(
        error.contains("acceptance check status must be passed, failed, not_verified, or blocked")
    );
}
