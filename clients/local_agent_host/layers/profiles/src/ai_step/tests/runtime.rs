fn response(tool_calls: Option<Value>) -> AiRuntimeResult {
    AiRuntimeResult {
        content: "done".to_string(),
        reasoning: Some("reason".to_string()),
        tool_calls,
        finish_reason: Some("stop".to_string()),
        usage: Some(json!({"input_tokens": 12})),
        response_id: Some("response-1".to_string()),
        response_output_items: vec![json!({"type": "message"})],
        request_input_items: vec![json!({"role": "user", "content": "hello"})],
    }
}

#[test]
fn durable_checkpoint_preserves_request_history_exactly() {
    let mut result = response(None);
    let request_items = vec![
        message_item("user", Value::String(String::from("old").repeat(500_000))),
        json!({
            "type": "function_call_output",
            "call_id": "call-1",
            "output": String::from("T").repeat(500_000)
        }),
        json!({
            "type": "compaction",
            "id": "cmp-1",
            "encrypted_content": "opaque"
        }),
        message_item("user", Value::String("current".into())),
    ];
    result.request_input_items = request_items.clone();

    let checkpoint = response_checkpoint(result, None);

    assert_eq!(
        checkpoint["response"]["request_input_items"],
        Value::Array(request_items)
    );
    assert!(checkpoint["response"]
        .get("checkpoint_compaction")
        .is_none());
}

#[test]
fn tool_outcome_maps_calls_and_uses_conservative_safety() {
    let tool_calls = json!([
        {
            "id": "call-read",
            "type": "function",
            "function": {"name": "read_file", "arguments": "{\"path\":\"README.md\"}"}
        },
        {
            "call_id": "call-write",
            "name": "write_file",
            "arguments": {"path": "result.txt"}
        }
    ]);
    let outcome = reduce_ai_step_outcome(
        AiSingleStepOutcome::ToolCommand {
            response: response(Some(tool_calls.clone())),
            tool_calls,
        },
        10_000,
        &NamedReadOnlyTools::new(["read_file"]),
    )
    .expect("reduce");
    let LocalAgentStepOutcome::WaitForTool { tool_calls, .. } = outcome else {
        panic!("expected tool outcome")
    };
    assert_eq!(tool_calls.len(), 2);
    assert!(!tool_calls[0].side_effecting);
    assert!(!tool_calls[0].requires_approval);
    assert!(tool_calls[1].side_effecting);
    assert!(tool_calls[1].requires_approval);
    assert_eq!(tool_calls[0].arguments["path"], "README.md");

    let exempt = NamedReadOnlyTools::new(["read_file"]).with_approval_exempt(["create_task"]);
    assert!(exempt.is_side_effecting("create_task"));
    assert!(!exempt.requires_approval("create_task"));
}

#[test]
fn retry_is_scheduled_without_sleeping() {
    let outcome = reduce_ai_step_outcome(
        AiSingleStepOutcome::Retry {
            error: "busy".to_string(),
            retry_kind: "provider_busy".to_string(),
            next_model_attempt: 2,
            backoff_ms: 750,
        },
        10_000,
        &ConservativeToolSafetyPolicy,
    )
    .expect("reduce");
    assert!(matches!(
        outcome,
        LocalAgentStepOutcome::Retry {
            resume_at_unix_ms: 10_750,
            ..
        }
    ));
}

#[test]
fn final_response_keeps_provider_output_for_terminal_audit() {
    let outcome = reduce_ai_step_outcome(
        AiSingleStepOutcome::Final(response(None)),
        10_000,
        &ConservativeToolSafetyPolicy,
    )
    .expect("reduce");
    let LocalAgentStepOutcome::Succeed { output } = outcome else {
        panic!("expected final outcome")
    };
    assert_eq!(output["content"], "done");
    assert_eq!(output["response_id"], "response-1");
    assert_eq!(output["request_input_items"][0]["role"], "user");
}

#[test]
fn composer_selected_plugin_cannot_finish_before_task_handoff() {
    let mut claim = task_claim();
    claim.run.owner_entity_type = "conversation_turn".to_string();
    claim.run.owner_entity_id = "turn-1".to_string();
    claim.run.profile_key = "main_chat".to_string();
    claim.run.input = json!({
        "message": "帮我设计咖啡店活动网页首屏",
        "explicit_capability_selection": {
            "kind": "plugin",
            "plugin_key": "web-design-plugin",
            "component_key": "web-design-mcp",
            "display_name": "Web Design Studio"
        }
    });

    let outcome = reduce_ai_step_outcome_for_claim(
        AiSingleStepOutcome::Final(response(None)),
        &claim,
        10_000,
        &ConservativeToolSafetyPolicy,
    )
    .expect("reduce selected Plugin final");

    let LocalAgentStepOutcome::Continue { checkpoint } = outcome else {
        panic!("selected Plugin must dispatch a Task before Main Chat can finish")
    };
    assert_eq!(
        checkpoint["continuation"]["reason"],
        "selected_plugin_task_handoff_required"
    );
    let guidance = checkpoint["continuation"]["input_items"][0]["content"]
        .as_str()
        .expect("corrective guidance");
    assert!(guidance.contains("create_task"));
    assert!(guidance.contains("Ask User"));
    assert!(!guidance.contains("Requirement Survey"));

    claim.run.continuation_input = Some(json!({
        "invocations": [{
            "tool_name": "wait_for_task_completion",
            "status": "succeeded",
            "result": {"accepted": true}
        }]
    }));
    assert!(matches!(
        reduce_ai_step_outcome_for_claim(
            AiSingleStepOutcome::Final(response(None)),
            &claim,
            10_000,
            &ConservativeToolSafetyPolicy,
        )
        .expect("reduce handed-off Plugin final"),
        LocalAgentStepOutcome::Succeed { .. }
    ));
}

#[test]
fn pending_followup_review_cannot_finish_before_resolution() {
    let mut claim = task_claim();
    claim.run.owner_entity_type = "conversation_turn".to_string();
    claim.run.owner_entity_id = "turn-1".to_string();
    claim.run.profile_key = "main_chat".to_string();
    claim.run.input = json!({
        "message": "Make the running work match this correction",
        "task_followup_review": {
            "type": "task_followup_review",
            "review_message_id": "message-followup",
            "pending": true,
            "tasks": [{
                "task_id": "task-1",
                "review_state": "pending"
            }]
        }
    });

    let outcome = reduce_ai_step_outcome_for_claim(
        AiSingleStepOutcome::Final(response(None)),
        &claim,
        10_000,
        &ConservativeToolSafetyPolicy,
    )
    .expect("reduce pending follow-up final");

    let LocalAgentStepOutcome::Continue { checkpoint } = outcome else {
        panic!("pending follow-up review must continue until it is resolved")
    };
    assert_eq!(
        checkpoint["continuation"]["reason"],
        "task_followup_review_required"
    );
    let guidance = checkpoint["continuation"]["input_items"][0]["content"]
        .as_str()
        .expect("corrective guidance");
    assert!(guidance.contains("resolve_task_followup"));
    assert!(guidance.contains("every pending candidate"));
    assert!(guidance.contains("choose replace"));

    claim.run.input["task_followup_review"]["pending"] = json!(false);
    assert!(matches!(
        reduce_ai_step_outcome_for_claim(
            AiSingleStepOutcome::Final(response(None)),
            &claim,
            10_000,
            &ConservativeToolSafetyPolicy,
        )
        .expect("reduce resolved follow-up final"),
        LocalAgentStepOutcome::Succeed { .. }
    ));
}

#[test]
fn held_replacement_cannot_finish_before_corrected_graph_creation() {
    let mut claim = task_claim();
    claim.run.owner_entity_type = "conversation_turn".to_string();
    claim.run.owner_entity_id = "turn-1".to_string();
    claim.run.profile_key = "main_chat".to_string();
    claim.run.input = json!({
        "message": "Make the result match the corrected acceptance criteria",
        "task_followup_review": {
            "type": "task_followup_review",
            "review_message_id": "message-followup",
            "pending": false,
            "replacement_pending": true,
            "tasks": [{
                "task_id": "task-1",
                "review_state": "held",
                "decision_action": "replace"
            }]
        }
    });

    let outcome = reduce_ai_step_outcome_for_claim(
        AiSingleStepOutcome::Final(response(None)),
        &claim,
        10_000,
        &ConservativeToolSafetyPolicy,
    )
    .expect("reduce held replacement final");
    let LocalAgentStepOutcome::Continue { checkpoint } = outcome else {
        panic!("held replacement must continue until corrected work is created")
    };
    assert_eq!(
        checkpoint["continuation"]["reason"],
        "task_replacement_required"
    );
    let guidance = checkpoint["continuation"]["input_items"][0]["content"]
        .as_str()
        .unwrap();
    assert!(guidance.contains("supersedes_task_ids"));
}
