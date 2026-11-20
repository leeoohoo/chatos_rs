fn claim(profile_key: &str, mut input: Value) -> LocalAgentRunClaim {
    let has_nested_agent = ["agent_execution_context", "agent_team_execution"]
        .into_iter()
        .any(|container| input.pointer(&format!("/{container}/agent_id")).is_some());
    if !has_nested_agent {
        input
            .as_object_mut()
            .expect("test Run input is an object")
            .entry("agent_id")
            .or_insert_with(|| json!(JIGULI_AGENT_ID));
    }
    LocalAgentRunClaim {
        worker_id: "worker-1".to_string(),
        claim_token: "claim-1".to_string(),
        run: LocalAgentRunRecord {
            run_id: "run-1".to_string(),
            owner_user_id: "user-1".to_string(),
            owner_entity_type: if profile_key == MAIN_CHAT_PROFILE_KEY {
                "conversation_turn".to_string()
            } else {
                "task".to_string()
            },
            owner_entity_id: if profile_key == MAIN_CHAT_PROFILE_KEY {
                "turn-1".to_string()
            } else {
                "task-1".to_string()
            },
            profile_key: profile_key.to_string(),
            model_config_ref: "model-1".to_string(),
            model_config_revision: "revision-1".to_string(),
            capability_policy_revision: "policy-1".to_string(),
            input,
            status: LocalAgentRunStatus::ModelRunning,
            iteration: 1,
            model_attempt: 1,
            max_iterations: 8,
            version: 2,
            claim_token: Some("claim-1".to_string()),
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
fn main_chat_memory_uses_stable_agent_thread_and_stable_message_ids() {
    let claim = claim(
        MAIN_CHAT_PROFILE_KEY,
        json!({
            "conversation_id": "conversation-1",
            "turn_id": "turn-1",
            "message_id": "message-1",
            "message": "hello",
            "conversation_title": "红果会话",
            "conversation_resource": {"kind": "project", "resource_id": "project-1"},
            "ui_message_metadata": {"composer": "main_chat"},
            "attachments": [{
                "attachment_id": "attachment-1",
                "display_name": "brief.pdf",
                "media_type": "application/pdf",
                "byte_size": 128,
                "sha256": "a".repeat(64),
                "authorized_local_ref": "local-attachment:brief.pdf",
                "metadata": {}
            }],
            "project_id": "project-1",
            "project_name": "红果逆向"
        }),
    );
    let plan = memory_plan(&claim, "local_agent", "message").expect("plan");

    assert_eq!(plan.thread_id, "client-agent:agent:jiguli");
    assert_eq!(plan.turn_id, "turn-1");
    assert_eq!(plan.scope.tenant_id, "user-1");
    assert_eq!(plan.scope.source_id, "local_agent");
    assert_eq!(plan.scope.thread_id, "client-agent:agent:jiguli");
    assert_eq!(plan.scope.subject_id.as_deref(), Some("agent:jiguli"));
    assert_eq!(
        plan.scope.context_project_subject_id.as_deref(),
        Some("project:project-1")
    );
    let user = plan.user_record.expect("user record");
    assert_eq!(user.message_id.as_deref(), Some("message-1"));
    assert_eq!(user.conversation_id, "client-agent:agent:jiguli");
    assert_eq!(
        plan.record_options.assistant_message_id.as_deref(),
        Some("run-1:assistant:1")
    );
    assert_eq!(user.metadata.expect("metadata")["tenant_id"], "user-1");
    assert_eq!(
        plan.record_options.assistant_metadata.as_ref().unwrap()["conversation_title"],
        "红果会话"
    );
    assert_eq!(
        plan.record_options.assistant_metadata.as_ref().unwrap()["ui_attachments"][0]
            ["attachment_id"],
        "attachment-1"
    );
    let metadata = plan.record_options.assistant_metadata.as_ref().unwrap();
    assert_eq!(metadata["conversation_id"], "conversation-1");
    assert_eq!(metadata["project_id"], "project-1");
    assert_eq!(metadata["project_name"], "红果逆向");
    assert_eq!(metadata["memory_project_id"], "project-1");
    assert!(metadata["thread_project_aggregation_id"].is_null());
    assert_eq!(
        metadata["local_agent_step_claim"],
        json!({"run_id": "run-1", "claim_token": "claim-1", "version": 2})
    );
    assert!(plan
        .record_options
        .tool_metadata
        .as_ref()
        .unwrap()
        .get("local_agent_step_claim")
        .is_none());
}

#[test]
fn memory_plan_rejects_runs_without_an_explicit_agent_identity() {
    let mut claim = claim(MAIN_CHAT_PROFILE_KEY, json!({"message": "hello"}));
    claim
        .run
        .input
        .as_object_mut()
        .expect("test Run input is an object")
        .remove("agent_id");

    match memory_plan(&claim, "local_agent", "message") {
        Ok(_) => panic!("missing Agent identity must be rejected"),
        Err(error) => assert_eq!(error, "Memory identity requires an explicit agent_id"),
    }
}

#[test]
fn memory_plan_rejects_runtime_profiles_as_agent_identities() {
    for agent_id in ["main-chat", "main_chat", "task-execution", "task_execution"] {
        let claim = claim(
            MAIN_CHAT_PROFILE_KEY,
            json!({"message": "hello", "agent_id": agent_id}),
        );

        match memory_plan(&claim, "local_agent", "message") {
            Ok(_) => panic!("runtime profile {agent_id} must not become an Agent identity"),
            Err(error) => assert_eq!(
                error,
                "Memory identity must not use a runtime profile key as agent_id"
            ),
        }
    }
}

#[test]
fn memory_plan_rejects_conflicting_explicit_identity() {
    let claim = claim(
        MAIN_CHAT_PROFILE_KEY,
        json!({
            "message": "hello",
            "agent_id": "wrong-agent",
            "project_id": "wrong-project",
            "agent_execution_context": {
                "agent_id": "agent-1",
                "agent_name": "工程 Agent",
                "project_id": "project-1"
            }
        }),
    );

    match memory_plan(&claim, "local_agent", "message") {
        Ok(_) => panic!("conflicting Agent identity must be rejected"),
        Err(error) => assert_eq!(error, "Memory identity has conflicting agent_id values"),
    }
}

#[test]
fn team_todo_memory_uses_executor_agent_and_project_subjects_not_room() {
    let claim = claim(
        "task_execution",
        json!({
            "prompt": "implement the todo",
            "agent_execution_context": {
                "agent_id": "agent-1",
                "agent_name": "工程 Agent",
                "project_id": "project-1",
                "room_id": "room-1"
            },
            "agent_team_execution": {
                "agent_id": "agent-1",
                "project_id": "project-1",
                "team_id": "room-1",
                "todo_id": "todo-1"
            }
        }),
    );

    let plan = memory_plan(&claim, "local_agent", "prompt").expect("plan");

    assert_eq!(plan.scope.subject_id.as_deref(), Some("agent:agent-1"));
    assert_eq!(
        plan.scope.context_project_subject_id.as_deref(),
        Some("project:project-1")
    );
    let metadata = plan.record_options.assistant_metadata.expect("metadata");
    assert_eq!(metadata["thread_kind"], "todo_execution");
    assert_eq!(metadata["agent_name"], "工程 Agent");
    assert_eq!(metadata["project_id"], "project-1");
    assert_eq!(metadata["memory_project_id"], "project-1");
    assert_eq!(metadata["thread_project_aggregation_id"], "project-1");
    assert_ne!(plan.scope.subject_id.as_deref(), Some("room:room-1"));
}

#[test]
fn agent_conversation_uses_real_agent_identity_even_on_main_chat_profile() {
    let claim = claim(
        MAIN_CHAT_PROFILE_KEY,
        json!({
            "message": "continue the project",
            "agent_execution_context": {
                "agent_id": "agent-1",
                "project_id": "project-1",
                "room_id": "room-1"
            }
        }),
    );

    let plan = memory_plan(&claim, "local_agent", "message").expect("plan");

    assert_eq!(plan.thread_id, "client-agent:agent:agent-1");
    assert_eq!(plan.scope.subject_id.as_deref(), Some("agent:agent-1"));
    assert_eq!(
        plan.scope.context_project_subject_id.as_deref(),
        Some("project:project-1")
    );
    let metadata = plan.record_options.assistant_metadata.expect("metadata");
    assert_eq!(metadata["thread_kind"], "agent_conversation");
    assert_eq!(metadata["memory_project_id"], "project-1");
    assert!(metadata["thread_project_aggregation_id"].is_null());
}

#[test]
fn retries_and_checkpointed_steps_do_not_resave_initial_user_record() {
    let mut retry = claim(MAIN_CHAT_PROFILE_KEY, json!({"message": "hello"}));
    retry.run.model_attempt = 2;
    assert!(memory_plan(&retry, "local_agent", "message")
        .expect("retry plan")
        .user_record
        .is_none());

    let mut continued = claim(MAIN_CHAT_PROFILE_KEY, json!({"message": "hello"}));
    continued.run.checkpoint = json!({"response": {}});
    assert!(memory_plan(&continued, "local_agent", "message")
        .expect("continued plan")
        .user_record
        .is_none());
}

#[test]
fn task_memory_uses_task_thread_and_decodes_external_tool_results() {
    let mut claim = claim("task_execution", json!({"prompt": "inspect repo"}));
    claim.run.iteration = 2;
    claim.run.continuation_input = Some(json!({
        "type": "tool_results",
        "batch_id": "batch-1",
        "invocations": [{
            "call_id": "call-1",
            "tool_name": "read_file",
            "status": "succeeded",
            "result": {"content": "ok"},
            "error": null
        }]
    }));
    let plan = memory_plan(&claim, "local_agent", "prompt").expect("plan");

    assert_eq!(plan.thread_id, "task-1");
    assert_eq!(plan.turn_id, "run-1");
    assert_eq!(plan.scope.subject_id.as_deref(), Some("agent:jiguli"));
    assert!(plan.user_record.is_none());
    assert_eq!(plan.external_tool_results.len(), 1);
    assert_eq!(plan.external_tool_results[0].tool_call_id, "call-1");
    assert_eq!(plan.external_tool_results[0].name, "read_file");
    assert!(plan.external_tool_results[0].success);
    assert_eq!(
        plan.record_options.tool_message_id_prefix.as_deref(),
        Some("run-1:tool:batch-1")
    );
}

#[test]
fn retried_task_reuses_the_logical_task_memory_thread() {
    let claim = claim(
        "task_execution",
        json!({
            "prompt": "review the complete solution again",
            "retry_instructions": ["重新读取完整方案，不要复用上一次的截断结论"]
        }),
    );

    let plan = memory_plan(&claim, "local_agent", "prompt").expect("retry plan");

    assert_eq!(plan.thread_id, "task-1");
    assert_eq!(plan.scope.thread_id, "task-1");
    assert_eq!(plan.turn_id, "run-1");
}
