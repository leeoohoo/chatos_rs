#[test]
fn native_team_executor_uses_the_stable_todo_thread() {
    let claim = claim(
        "task_execution",
        json!({
            "prompt": "continue the todo",
            "memory_thread_id": "client-agent:todo:todo-1",
            "agent_execution_context": {
                "agent_id": "agent-1",
                "project_id": "project-1"
            },
            "team_executor_context": {
                "todo_id": "todo-1",
                "agent_id": "agent-1",
                "project_id": "project-1"
            }
        }),
    );

    let plan = memory_plan(&claim, "local_agent", "prompt").expect("todo plan");

    assert_eq!(plan.thread_id, "client-agent:todo:todo-1");
    assert_eq!(plan.scope.thread_id, "client-agent:todo:todo-1");
    assert_eq!(
        plan.record_options.assistant_metadata.as_ref().unwrap()["thread_kind"],
        "todo_execution"
    );
}

#[test]
fn nineteen_thousand_character_solution_result_is_preserved_without_truncation() {
    let content = "方案".repeat(9_980) + "终";
    assert_eq!(content.chars().count(), 19_961);
    let mut claim = claim("task_execution", json!({"prompt": "review solution"}));
    claim.run.continuation_input = Some(json!({
        "type": "tool_results",
        "batch_id": "batch-solution",
        "invocations": [{
            "call_id": "call-solution",
            "tool_name": "capability_invoke",
            "status": "succeeded",
            "result": content,
            "error": null
        }]
    }));

    let result = external_tool_results(&claim).unwrap().remove(0);

    assert_eq!(result.content, content);
    assert!(!result.content.contains("truncated"));
    assert!(!result.content.contains("tool_result_reference"));
}

#[test]
fn over_one_hundred_sixty_thousand_bytes_returns_a_durable_page_reference() {
    let content = "x".repeat(INLINE_TOOL_RESULT_MAX_BYTES + 1);
    let mut claim = claim("task_execution", json!({"prompt": "review large result"}));
    claim.run.continuation_input = Some(json!({
        "type": "tool_results",
        "invocations": [{
            "call_id": "call-large",
            "tool_name": "capability_invoke",
            "status": "succeeded",
            "result": content
        }]
    }));

    let result = external_tool_results(&claim).unwrap().remove(0);
    let descriptor: Value = serde_json::from_str(&result.content).expect("descriptor");

    assert_eq!(descriptor["type"], "tool_result_reference");
    assert_eq!(descriptor["source_call_id"], "call-large");
    assert_eq!(descriptor["total_chars"], INLINE_TOOL_RESULT_MAX_BYTES + 1);
    assert_eq!(descriptor["total_bytes"], INLINE_TOOL_RESULT_MAX_BYTES + 1);
    assert_eq!(descriptor["read_tool"], "tool_result_read");
    assert_eq!(descriptor["sha256"].as_str().unwrap().len(), 64);
    assert!(!result.content.contains("...[truncated"));
}

#[test]
fn browser_screenshots_in_structured_result_become_a_durable_reference() {
    let screenshot = "a".repeat(621_248);
    let mut claim = claim("task_execution", json!({"prompt": "verify browser"}));
    claim.run.continuation_input = Some(json!({
        "type": "tool_results",
        "invocations": [{
            "call_id": "call-browser",
            "tool_name": "capability_invoke",
            "status": "succeeded",
            "result": {
                "content": {
                    "content": [{
                        "type": "text",
                        "text": "[Duplicate JSON text omitted; use structuredContent as the canonical result.]"
                    }],
                    "structuredContent": {
                        "normalized_result": {
                            "steps": [
                                {"artifacts": {"screenshot": {"value": {"data_base64": screenshot}}}},
                                {"artifacts": {"screenshot": {"value": {"data_base64": screenshot}}}}
                            ]
                        }
                    }
                }
            }
        }]
    }));

    let result = external_tool_results(&claim).unwrap().remove(0);
    let descriptor: Value = serde_json::from_str(&result.content).expect("descriptor");

    assert_eq!(descriptor["type"], "tool_result_reference");
    assert_eq!(descriptor["source_call_id"], "call-browser");
    assert_eq!(descriptor["source_format"], "tool_result_envelope_json");
    assert!(descriptor["total_bytes"].as_u64().unwrap() > 1_048_576);
    assert!(serde_json::to_vec(&result).unwrap().len() < 8_000);
    assert!(!result.content.contains(&"a".repeat(8_192)));
}

#[test]
fn cumulative_large_results_use_references_before_the_runtime_total_budget() {
    let content = "x".repeat(140_000);
    let invocations: Vec<Value> = (0..6)
        .map(|index| {
            json!({
                "call_id": format!("call-{index}"),
                "tool_name": "capability_invoke",
                "status": "succeeded",
                "result": content
            })
        })
        .collect();
    let mut claim = claim("task_execution", json!({"prompt": "review result batch"}));
    claim.run.continuation_input = Some(json!({
        "type": "tool_results",
        "invocations": invocations
    }));

    let results = external_tool_results(&claim).unwrap();

    assert_eq!(results.len(), 6);
    assert!(results[..5]
        .iter()
        .all(|result| result.content.chars().count() == 140_000));
    assert!(results[5].content.contains("tool_result_reference"));
    assert!(!results[5].content.contains("tool_result_truncated"));
}

#[test]
fn failed_external_tools_preserve_actionable_details_for_both_profiles() {
    for profile in [MAIN_CHAT_PROFILE_KEY, "task_execution"] {
        let mut claim = claim(profile, json!({"message": "browse", "turn_id": "turn-1"}));
        let detail = json!({"content": {"content": [
                {"type": "text", "text": "no browser session is bound; call browser_session_open first"},
                {"type": "text", "text": "Then retry browser_navigate with a non-empty url."},
                {"type": "image", "data": "/9j/AA==", "mimeType": "image/jpeg"}
            ]}, "is_error": true});
        claim.run.continuation_input = Some(json!({
            "type": "tool_results", "invocations": [{
                "call_id": "call-failed", "tool_name": "capability_invoke", "status": "failed",
                "error": "native_tool_reported_error", "result": detail
            }]
        }));
        let plan = memory_plan(&claim, "local_agent", "prompt").unwrap();
        let result = &plan.external_tool_results[0];
        assert!(!result.success && result.is_error && !result.fatal_error);
        assert_eq!(result.content, "native_tool_reported_error\nno browser session is bound; call browser_session_open first\nThen retry browser_navigate with a non-empty url.");
        assert_eq!(result.result.as_ref(), Some(&detail));
        assert!(result.transient_model_input.is_none());
        assert_eq!(
            result.conversation_turn_id.as_deref(),
            Some(if profile == MAIN_CHAT_PROFILE_KEY {
                "turn-1"
            } else {
                "run-1"
            })
        );
    }
}

#[test]
fn failed_external_tools_support_plain_structured_and_missing_diagnostics() {
    let cases = [
        (
            json!({"content": "url must be a non-empty string"}),
            "bad_arguments",
            "url must be a non-empty string",
        ),
        (
            json!({"content": {"error": "skill_activation_required", "required_skill_refs": ["product-skill:test"]}}),
            "native_tool_reported_error",
            "product-skill:test",
        ),
        (
            json!("permission denied"),
            "permission_denied",
            "permission denied",
        ),
        (Value::Null, "connection_lost", "connection_lost"),
        (
            json!({"is_error": true, "content": {"message": "runtime unavailable"}}),
            "",
            "runtime unavailable",
        ),
        (Value::Null, "", "tool execution failed"),
    ];
    for (detail, error, expected) in cases {
        let mut claim = claim(MAIN_CHAT_PROFILE_KEY, json!({"message": "test"}));
        claim.run.continuation_input = Some(json!({"type": "tool_results", "invocations": [{
            "call_id": "call", "tool_name": "external_mcp__test", "status": "failed",
            "result": detail, "error": error
        }]}));
        let result = external_tool_results(&claim).unwrap().remove(0);
        assert!(result.content.contains(expected), "{}", result.content);
        assert!(error.is_empty() || result.content.starts_with(error));
        assert!(result.is_error && !result.success);
    }
}

#[test]
fn task_memory_restores_wrapped_mcp_images_as_transient_model_input() {
    let mut claim = claim("task_execution", json!({"task": "observe"}));
    claim.run.continuation_input = Some(json!({
        "type": "tool_results",
        "batch_id": "batch-visual",
        "invocations": [{
            "call_id": "call-visual",
            "tool_name": "capability_invoke",
            "status": "succeeded",
            "result": {
                "content": {
                    "content": [
                        {"type": "text", "text": "Active application: ChatOS"},
                        {"type": "image", "data": "/9j/AA==", "mimeType": "image/jpeg"}
                    ],
                    "structuredContent": {
                        "activeApplication": {"name": "ChatOS"}
                    }
                },
                "is_error": false,
                "made_progress": true
            }
        }]
    }));

    let plan = memory_plan(&claim, "local_agent", "task").expect("memory plan");
    let result = plan.external_tool_results.first().expect("tool result");

    assert_eq!(result.content, "Active application: ChatOS");
    assert_eq!(
        result
            .result
            .as_ref()
            .and_then(|value| value.pointer("/activeApplication/name"))
            .and_then(Value::as_str),
        Some("ChatOS")
    );
    assert_eq!(
        result
            .transient_model_input
            .as_ref()
            .and_then(|input| input.items().first())
            .and_then(|value| value.get("image_url"))
            .and_then(Value::as_str),
        Some("data:image/jpeg;base64,/9j/AA==")
    );
}
