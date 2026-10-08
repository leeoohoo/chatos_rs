use super::*;
use chatos_ai_runtime::AiRuntime;
use chatos_local_agent_protocol::{LocalAgentRunRecord, LocalAgentRunStatus};

struct OwnerCheckingModelResolver;

#[async_trait]
impl LocalModelRuntimeResolver for OwnerCheckingModelResolver {
    async fn resolve_model_runtime(
        &self,
        owner_user_id: &str,
        _model_config_ref: &str,
        _model_config_revision: &str,
    ) -> Result<TransientLocalModelRuntime, String> {
        if owner_user_id != "user-1" {
            return Err("wrong model owner".to_string());
        }
        Ok(TransientLocalModelRuntime {
            runner: Arc::new(ContextualTurnRunner::new(AiRuntime::new(None), None)),
            model_config: ModelRuntimeConfig {
                model: "test-model".to_string(),
                ..ModelRuntimeConfig::default()
            },
        })
    }
}

struct OwnerCheckingCapabilityResolver;

#[async_trait]
impl LocalCapabilityResolver for OwnerCheckingCapabilityResolver {
    async fn resolve_capabilities(
        &self,
        owner_user_id: &str,
        _profile_key: &str,
        _capability_policy_revision: &str,
    ) -> Result<ResolvedLocalCapabilities, String> {
        if owner_user_id != "user-1" {
            return Err("wrong capability owner".to_string());
        }
        Ok(ResolvedLocalCapabilities::default())
    }
}

fn claim(checkpoint: Value, continuation_input: Option<Value>) -> LocalAgentRunClaim {
    LocalAgentRunClaim {
        worker_id: "worker".to_string(),
        claim_token: "token".to_string(),
        run: LocalAgentRunRecord {
            run_id: "run-1".to_string(),
            owner_user_id: "user-1".to_string(),
            owner_entity_type: "conversation".to_string(),
            owner_entity_id: "conversation-1".to_string(),
            profile_key: MAIN_CHAT_PROFILE_KEY.to_string(),
            model_config_ref: "model-1".to_string(),
            model_config_revision: "revision-1".to_string(),
            capability_policy_revision: "policy-1".to_string(),
            input: json!({"message": "hello"}),
            status: LocalAgentRunStatus::ModelRunning,
            iteration: 2,
            model_attempt: 1,
            max_iterations: 8,
            version: 4,
            claim_token: Some("token".to_string()),
            claim_until_unix_ms: Some(20_000),
            next_attempt_at_unix_ms: None,
            pending_tool_batch: None,
            checkpoint,
            continuation_input,
            terminal_outcome: None,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 2,
        },
    }
}

#[tokio::test]
async fn planner_resolves_control_plane_state_for_the_run_owner() {
    let planner = ControlPlaneLocalAiStepPlanner::main_chat(
        OwnerCheckingModelResolver,
        OwnerCheckingCapabilityResolver,
    );
    let prepared = planner
        .prepare_ai_step(&claim(Value::Null, None))
        .await
        .expect("prepare owner-scoped step");
    assert_eq!(
        prepared.request.runtime_options.caller_model.as_deref(),
        Some("test-model")
    );
}

#[tokio::test]
async fn task_execution_keeps_the_bound_workspace_root_authoritative() {
    let planner = ControlPlaneLocalAiStepPlanner::task_execution(
        OwnerCheckingModelResolver,
        OwnerCheckingCapabilityResolver,
    );
    let mut task_claim = claim(Value::Null, None);
    task_claim.run.profile_key = TASK_EXECUTION_PROFILE_KEY.to_string();
    task_claim.run.owner_entity_type = "task".to_string();
    task_claim.run.owner_entity_id = "task-1".to_string();
    task_claim.run.input = json!({
        "prompt": "list the project root",
        "tool_options": {
            "requires_execution": false,
            "enabled_builtin_kinds": [],
            "plugin_hints": []
        }
    });

    let prepared = planner
        .prepare_ai_step(&task_claim)
        .await
        .expect("prepare Task execution step");
    let instructions = prepared
        .request
        .model_request
        .instructions
        .as_deref()
        .expect("Task workspace instructions");

    assert!(instructions.contains("path `.` is the authoritative root"));
    assert!(instructions.contains("Never replace that root with a child directory"));
}

#[test]
fn conversation_runtime_settings_override_the_snapshot_thinking_level_per_run() {
    let mut config = ModelRuntimeConfig {
        provider: "openai".to_string(),
        thinking_level: Some("medium".to_string()),
        ..ModelRuntimeConfig::default()
    };
    apply_run_thinking_level(
        &mut config,
        &json!({"runtime_settings": {
            "reasoning_enabled": true,
            "selected_thinking_level": "high"
        }}),
    )
    .expect("enabled override");
    assert_eq!(config.thinking_level.as_deref(), Some("high"));
    apply_run_thinking_level(
        &mut config,
        &json!({"runtime_settings": {
            "reasoning_enabled": false,
            "selected_thinking_level": "high"
        }}),
    )
    .expect("disabled override");
    assert_eq!(config.thinking_level.as_deref(), Some("none"));
}

#[test]
fn local_tool_definitions_replace_control_plane_copies() {
    let controlled = vec![
        json!({"name": "notepad_read_note", "description": "server"}),
        json!({"name": "notepad_delete_note"}),
        json!({"name": "read_file"}),
    ];
    let local = vec![json!({"name": "notepad_read_note", "description": "local"})];
    let merged =
        merge_local_tools(controlled, &local, &["notepad_".to_string()]).expect("merge tools");
    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0]["name"], "read_file");
    assert_eq!(merged[1]["description"], "local");
}

#[test]
fn local_task_tools_keep_request_scoped_plugin_and_external_mcp_choices() {
    let controlled = vec![json!({
        "type": "function",
        "name": "create_task",
        "parameters": {
            "type": "object",
            "properties": {
                "plugin_hints": {
                    "type": "array",
                    "items": {"type": "object", "properties": {
                        "plugin_key": {"type": "string", "enum": ["plugin-1"]}
                    }}
                },
                "external_mcp_config_ids": {
                    "type": "array",
                    "items": {"type": "string", "enum": ["mcp-1"]}
                }
            }
        }
    })];
    let local = vec![json!({
        "type": "function",
        "name": "create_task",
        "parameters": {
            "type": "object",
            "properties": {
                "title": {"type": "string"},
                "plugin_hints": {"type": "array", "maxItems": 0},
                "external_mcp_config_ids": {"type": "array", "maxItems": 0}
            },
            "additionalProperties": false
        }
    })];

    let merged = merge_local_tools(controlled, &local, &[]).expect("merge Task schema");

    assert_eq!(merged.len(), 1);
    assert_eq!(
        merged[0].pointer("/parameters/properties/plugin_hints/items/properties/plugin_key/enum/0"),
        Some(&json!("plugin-1"))
    );
    assert_eq!(
        merged[0].pointer("/parameters/properties/external_mcp_config_ids/items/enum/0"),
        Some(&json!("mcp-1"))
    );
    assert_eq!(
        merged[0].pointer("/parameters/additionalProperties"),
        Some(&json!(false))
    );
}

#[test]
fn task_tools_are_filtered_to_the_declared_minimum_capabilities() {
    let tools = vec![
        json!({"name": "read_file"}),
        json!({"name": "project_read"}),
        json!({"name": "commit_edit_session"}),
        json!({"name": "project_write"}),
        json!({"name": "execute_command"}),
        json!({"name": "terminal_exec"}),
        json!({"name": "requirement_survey_create"}),
        json!({"name": "notepad_read_note"}),
        json!({"name": "capability_search"}),
        json!({"name": "task_run_process_record_process"}),
    ];
    let input = json!({"tool_options": {
        "requires_execution": false,
        "enabled_builtin_kinds": ["CodeMaintainerRead"],
        "plugin_hints": []
    }});
    let filtered = task_scoped_tools(tools, &input).expect("filter Task tools");
    let names = filtered.iter().filter_map(tool_name).collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "read_file",
            "project_read",
            "task_run_process_record_process"
        ]
    );
}

#[test]
fn ask_user_tools_are_available_only_when_the_required_capability_is_frozen() {
    let tools = vec![
        json!({"name": "ask_user_prompt_key_values"}),
        json!({"name": "ask_user_prompt_choices"}),
        json!({"name": "ask_user_prompt_mixed_form"}),
    ];
    let disabled = json!({"tool_options": {
        "requires_execution": false,
        "enabled_builtin_kinds": [],
        "plugin_hints": []
    }});
    assert!(task_scoped_tools(tools.clone(), &disabled)
        .expect("filter disabled Ask User")
        .is_empty());
    let enabled = json!({"tool_options": {
        "requires_execution": false,
        "enabled_builtin_kinds": ["AskUser"],
        "plugin_hints": []
    }});
    assert_eq!(
        task_scoped_tools(tools.clone(), &enabled).expect("filter enabled Ask User"),
        tools
    );
}

#[test]
fn windows_task_tools_follow_the_same_declared_capability_scope() {
    let tools = vec![
        json!({"name": "project_list"}),
        json!({"name": "project_read"}),
        json!({"name": "project_search"}),
        json!({"name": "project_write"}),
        json!({"name": "terminal_exec"}),
        json!({"name": "capability_search"}),
    ];
    let input = json!({"tool_options": {
        "requires_execution": true,
        "enabled_builtin_kinds": ["CodeMaintainerWrite", "CodeMaintainerRead"],
        "plugin_hints": []
    }});
    let filtered = task_scoped_tools(tools, &input).expect("filter Windows Task tools");
    assert_eq!(
        filtered.iter().filter_map(tool_name).collect::<Vec<_>>(),
        vec![
            "project_list",
            "project_read",
            "project_search",
            "project_write"
        ]
    );
}

#[test]
fn remote_connection_tools_require_both_selection_and_a_program_bound_connection() {
    let tools = vec![json!({"name": "remote_connection_controller_run_command"})];
    let input = json!({"tool_options": {
        "requires_execution": true,
        "enabled_builtin_kinds": ["RemoteConnectionController"],
        "plugin_hints": []
    }});
    assert!(task_scoped_tools(tools.clone(), &input)
        .expect("filter unbound remote tools")
        .is_empty());

    let mut bound = input;
    bound["remote_connection_id"] = json!("connection-1");
    assert_eq!(
        task_scoped_tools(tools.clone(), &bound).expect("filter bound remote tools"),
        tools
    );
}

#[test]
fn task_attachment_tool_is_available_only_when_the_source_task_carries_attachments() {
    let tools = vec![
        json!({"name": "local_attachment_read"}),
        json!({"name": "task_run_process_record_process"}),
    ];
    let base = json!({"tool_options": {
        "requires_execution": false,
        "enabled_builtin_kinds": [],
        "plugin_hints": []
    }});
    assert_eq!(
        task_scoped_tools(tools.clone(), &base)
            .expect("filter Task tools without attachments")
            .iter()
            .filter_map(tool_name)
            .collect::<Vec<_>>(),
        vec!["task_run_process_record_process"]
    );

    let mut with_attachment = base;
    with_attachment["attachments"] = json!([{
        "attachment_id": "attachment-1",
        "authorized_local_ref": "local-attachment:brief-1"
    }]);
    assert_eq!(
        task_scoped_tools(tools, &with_attachment)
            .expect("filter Task tools with attachments")
            .iter()
            .filter_map(tool_name)
            .collect::<Vec<_>>(),
        vec!["local_attachment_read", "task_run_process_record_process"]
    );
}

#[test]
fn legacy_task_without_tool_options_keeps_its_immutable_capability_snapshot() {
    let tools = vec![
        json!({"name": "read_file"}),
        json!({"name": "execute_command"}),
    ];
    assert_eq!(
        task_scoped_tools(tools.clone(), &json!({"prompt": "legacy"})).expect("legacy tools"),
        tools
    );
}

#[test]
fn reconstructs_tool_results_from_checkpoint_and_continuation() {
    let claim = claim(
        json!({"response": {
            "request_input_items": [{"role": "user", "content": "hello"}],
            "response_output_items": [{"type": "function_call", "call_id": "call-1"}]
        }}),
        Some(json!({
            "type": "tool_results",
            "invocations": [{
                "call_id": "call-1",
                "tool_name": "read_file",
                "status": "succeeded",
                "result": {"content": "ok"}
            }]
        })),
    );
    let (items, reason) = durable_step_input(&claim, "message").expect("input");
    assert_eq!(reason, "tool_results");
    assert_eq!(
        items.last().and_then(|item| item.get("call_id")),
        Some(&json!("call-1"))
    );
    assert_eq!(
        items.last().and_then(|item| item.get("type")),
        Some(&json!("function_call_output"))
    );
}

#[test]
fn reconstructs_wrapped_mcp_images_as_model_input_from_durable_continuation() {
    let checkpoint = json!({"response": {
        "request_input_items": [{"role": "user", "content": "observe"}],
        "response_output_items": [{
            "type": "function_call",
            "call_id": "call-visual",
            "name": "capability_invoke",
            "arguments": "{}"
        }]
    }});
    let continuation = json!({
        "type": "tool_results",
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
                "is_error": false
            }
        }]
    });

    let (items, reason) = durable_step_input(&claim(checkpoint, Some(continuation)), "message")
        .expect("durable Visual continuation");

    assert_eq!(reason, "tool_results");
    let output = items
        .iter()
        .find(|item| item["type"] == "function_call_output")
        .expect("function output");
    assert_eq!(output["call_id"], "call-visual");
    assert_eq!(output["output"], "Active application: ChatOS");
    let image = items
        .iter()
        .find(|item| item["type"] == "message" && item["role"] == "user")
        .and_then(|item| item["content"].as_array())
        .and_then(|content| content.first())
        .expect("transient image input");
    assert_eq!(image["type"], "input_image");
    assert_eq!(image["image_url"], "data:image/jpeg;base64,/9j/AA==");
}

#[tokio::test]
async fn successful_task_handoff_forces_one_tool_free_natural_acknowledgement() {
    let planner = ControlPlaneLocalAiStepPlanner::main_chat(
        OwnerCheckingModelResolver,
        OwnerCheckingCapabilityResolver,
    )
    .with_local_tools(vec![json!({
        "type": "function",
        "name": "wait_for_task_completion",
        "parameters": {"type": "object", "properties": {}}
    })])
    .expect("local Task tool");
    let handoff = claim(
        json!({"response": {
            "request_input_items": [{"role": "user", "content": "build it"}],
            "response_output_items": [{
                "type": "function_call",
                "call_id": "call-handoff",
                "name": "wait_for_task_completion",
                "arguments": "{}"
            }]
        }}),
        Some(json!({
            "type": "tool_results",
            "invocations": [{
                "call_id": "call-handoff",
                "tool_name": "wait_for_task_completion",
                "status": "succeeded",
                "result": {"accepted": true, "mode": "background"}
            }]
        })),
    );

    let prepared = planner
        .prepare_ai_step(&handoff)
        .await
        .expect("prepare handoff acknowledgement");

    assert!(prepared.request.model_request.tools.is_empty());
    let guidance = prepared
        .request
        .current_input_items
        .last()
        .expect("handoff guidance");
    assert_eq!(guidance["role"], "system");
    assert!(guidance["content"]
        .as_str()
        .is_some_and(|text| text.contains("Do not mention tasks, Task Runner")));
}

#[tokio::test]
async fn successful_task_outcome_report_forces_a_tool_free_final_response() {
    let planner = ControlPlaneLocalAiStepPlanner::task_execution(
        OwnerCheckingModelResolver,
        OwnerCheckingCapabilityResolver,
    )
    .with_local_tools(vec![
        json!({"type": "function", "name": "read_file", "parameters": {}}),
        json!({
            "type": "function",
            "name": "task_run_process_report_outcome",
            "parameters": {}
        }),
    ])
    .expect("Task tools");
    let mut reported = claim(
        json!({"response": {
            "request_input_items": [{"role": "user", "content": "do the work"}],
            "response_output_items": [{
                "type": "function_call",
                "call_id": "call-outcome",
                "name": "task_run_process_report_outcome",
                "arguments": "{\"status\":\"succeeded\",\"reason\":\"done\"}"
            }]
        }}),
        Some(json!({
            "type": "tool_results",
            "invocations": [{
                "call_id": "call-outcome",
                "tool_name": "task_run_process_report_outcome",
                "status": "succeeded",
                "result": {"reported": true}
            }]
        })),
    );
    reported.run.profile_key = TASK_EXECUTION_PROFILE_KEY.to_string();
    reported.run.owner_entity_type = "task".to_string();
    reported.run.owner_entity_id = "task-1".to_string();
    reported.run.input = json!({
        "prompt": "do the work",
        "tool_options": {
            "requires_execution": false,
            "enabled_builtin_kinds": ["CodeMaintainerRead"],
            "plugin_hints": []
        }
    });

    let prepared = planner
        .prepare_ai_step(&reported)
        .await
        .expect("prepare final Task response");

    assert!(prepared.request.model_request.tools.is_empty());
    let guidance = prepared
        .request
        .current_input_items
        .last()
        .expect("Task outcome guidance");
    assert_eq!(guidance["role"], "system");
    assert!(guidance["content"]
        .as_str()
        .is_some_and(|text| text.contains("[Task Outcome Reported]")));
}

#[test]
fn failed_or_unaccepted_wait_does_not_close_the_main_chat_tool_boundary() {
    assert!(!completed_async_task_handoff(Some(&json!({
        "type": "tool_results",
        "invocations": [{
            "tool_name": "wait_for_task_completion",
            "status": "failed",
            "result": {"accepted": true}
        }]
    }))));
    assert!(!completed_async_task_handoff(Some(&json!({
        "type": "tool_results",
        "invocations": [{
            "tool_name": "wait_for_task_completion",
            "status": "succeeded",
            "result": {"accepted": false}
        }]
    }))));
}

#[test]
fn retry_guidance_and_attachments_remain_durable() {
    let mut retry = claim(Value::Null, None);
    retry.run.model_attempt = 2;
    assert_eq!(
        durable_step_input(&retry, "message").expect("retry").1,
        "model_retry"
    );

    let guidance = Some(json!({
        "type": "guidance",
        "guidance": [{"message_id": "message-2", "message": "inspect tests", "attachments": []}]
    }));
    let (items, reason) =
        durable_step_input(&claim(Value::Null, guidance), "message").expect("guidance");
    assert_eq!(reason, "initial_request_with_guidance");
    assert_eq!(items.len(), 2);

    let mut attachment = claim(Value::Null, None);
    attachment.run.input = json!({
        "message": "",
        "attachments": [{
            "attachment_id": "attachment-1",
            "display_name": "brief.pdf",
            "media_type": "application/pdf",
            "byte_size": 42,
            "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "authorized_local_ref": "local-attachment:authority-1",
            "metadata": {"must_not_be_forwarded": true}
        }]
    });
    let (items, _) = durable_step_input(&attachment, "message").expect("attachment");
    let manifest = items[0]["content"][0]["text"].as_str().expect("manifest");
    assert!(manifest.contains("local-attachment:authority-1"));
    assert!(!manifest.contains("must_not_be_forwarded"));
}

#[test]
fn ask_user_resume_becomes_function_call_output_instead_of_a_new_user_message() {
    let checkpoint = json!({
        "response": {
            "request_input_items": [{"role": "user", "content": "build it"}],
            "response_output_items": [{
                "type": "function_call",
                "name": "ask_user_prompt_choices",
                "call_id": "ask-call-1",
                "arguments": "{}"
            }]
        }
    });
    let continuation = json!({
        "type": "resume",
        "reason": "ask_user_submitted",
        "input": {
            "source": "ask_user",
            "tool_call_id": "ask-call-1",
            "values": {},
            "selection": "local"
        }
    });
    let (items, reason) = durable_step_input(&claim(checkpoint, Some(continuation)), "message")
        .expect("Ask User continuation");
    assert_eq!(reason, "user_resume");
    let output = items.last().expect("function output");
    assert_eq!(output["type"], "function_call_output");
    assert_eq!(output["call_id"], "ask-call-1");
    assert!(output["output"]
        .as_str()
        .is_some_and(|value| value.contains("\"selection\":\"local\"")));
}
