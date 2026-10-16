use super::*;
use chatos_ai_runtime::AiRuntime;
use chatos_local_agent_protocol::{LocalAgentRunRecord, LocalAgentRunStatus};

#[path = "planner_attachment_tests.rs"]
mod attachment_tests;

struct OwnerCheckingModelResolver;

#[test]
fn plugin_installer_main_chat_exposes_only_installer_tools() {
    let tools = vec![
        json!({"name": "create_task"}),
        json!({"name": "agent_skill_activate"}),
        json!({"name": "plugin_installer_search_npm"}),
        json!({"name": "plugin_installer_install_npm"}),
    ];
    let scoped = main_chat_scoped_tools(tools, &json!({"conversation_id": "installer"}));
    let names = scoped
        .iter()
        .filter_map(tool_name)
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "plugin_installer_search_npm",
            "plugin_installer_install_npm"
        ]
    );
}

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
async fn requirement_plan_is_sent_each_step_outside_truncated_memory_for_main_and_task() {
    for task in [false, true] {
        let planner = if task {
            ControlPlaneLocalAiStepPlanner::task_execution(
                OwnerCheckingModelResolver,
                OwnerCheckingCapabilityResolver,
            )
        } else {
            ControlPlaneLocalAiStepPlanner::main_chat(
                OwnerCheckingModelResolver,
                OwnerCheckingCapabilityResolver,
            )
        };
        let plan = if task {
            json!({"snapshot": {"revision": 1, "markdown": "frozen v1"}, "task_scope": {"title": "review"}})
        } else {
            json!({"revision": 2, "markdown": "current v2", "confirmed_at_unix_ms": 10})
        };
        for checkpoint in [
            Value::Null,
            json!({"request_input_items": [], "conversation_memory": []}),
        ] {
            let mut current = claim(checkpoint, None);
            if task {
                current.run.profile_key = TASK_EXECUTION_PROFILE_KEY.into();
                current.run.input = json!({"prompt": "review", "acceptance_report_version":1,"acceptance_criteria":["Compare evidence"],"expected_outputs":["Review"],"final_response_max_chars":400,"source_user_message":"Review the original three checklist items verbatim.","tool_options": {"requires_execution": false, "enabled_builtin_kinds": [], "plugin_hints": []}});
            }
            current.run.input["requirement_plan"] = plan.clone();
            let prepared = planner.prepare_ai_step(&current).await.unwrap();
            if task {
                assert!(prepared.request.prefixed_input_items.iter().any(|item| item["content"].as_str().and_then(|text| serde_json::from_str::<Value>(text).ok())
                    == Some(json!({"type":"task_acceptance_contract","acceptance_report_version":1,"acceptance_criteria":["Compare evidence"],"expected_outputs":["Review"],"final_response_max_chars":400,"source_user_message":"Review the original three checklist items verbatim."}))));
            }
            assert!(prepared
                .request
                .prefixed_input_items
                .iter()
                .any(|item| item["role"] == "user"
                    && item["content"]
                        .as_str()
                        .and_then(|text| serde_json::from_str::<Value>(text).ok())
                        == Some(json!({"type": "requirement_plan_context", "plan": plan}))));
        }
    }
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

#[tokio::test]
async fn selected_plugin_is_sent_each_step_as_a_required_delivery_contract() {
    let planner = ControlPlaneLocalAiStepPlanner::task_execution(
        OwnerCheckingModelResolver,
        OwnerCheckingCapabilityResolver,
    );
    for checkpoint in [
        Value::Null,
        json!({"request_input_items": [], "conversation_memory": []}),
    ] {
        let mut task_claim = claim(checkpoint, None);
        task_claim.run.profile_key = TASK_EXECUTION_PROFILE_KEY.to_string();
        task_claim.run.owner_entity_type = "task".to_string();
        task_claim.run.owner_entity_id = "task-1".to_string();
        task_claim.run.input = json!({
            "prompt": "create the solution plan",
            "tool_options": {
                "requires_execution": true,
                "enabled_builtin_kinds": ["Notepad"],
                "plugin_hints": [{
                    "plugin_key": "chatos-solution-studio@chatos-marketplace",
                    "component_key": "solution-authoring-mcp",
                    "reason": "persist the traceable requirements and execution plan"
                }]
            }
        });

        let prepared = planner.prepare_ai_step(&task_claim).await.unwrap();
        let contract = prepared
            .request
            .prefixed_input_items
            .iter()
            .filter_map(|item| item["content"].as_str())
            .filter_map(|content| serde_json::from_str::<Value>(content).ok())
            .find(|content| content["type"] == "task_selected_plugin_contract")
            .expect("program-owned selected Plugin contract");
        assert_eq!(contract["delivery_route"], "selected_plugin_required");
        assert_eq!(
            contract["selected_plugins"][0]["plugin_key"],
            "chatos-solution-studio@chatos-marketplace"
        );
        assert!(contract["guidance"]
            .as_str()
            .is_some_and(|text| text.contains("cannot create or update notes")));
    }
}

#[tokio::test]
async fn task_retry_instructions_are_sent_as_fresh_program_owned_context() {
    let planner = ControlPlaneLocalAiStepPlanner::task_execution(
        OwnerCheckingModelResolver,
        OwnerCheckingCapabilityResolver,
    );
    let mut task_claim = claim(Value::Null, None);
    task_claim.run.profile_key = TASK_EXECUTION_PROFILE_KEY.to_string();
    task_claim.run.owner_entity_type = "task".to_string();
    task_claim.run.owner_entity_id = "task-1".to_string();
    task_claim.run.input = json!({
        "prompt": "review the solution",
        "retry_instructions": ["重新读取完整方案，不要复用上一次的截断结论"],
        "tool_options": {
            "requires_execution": false,
            "enabled_builtin_kinds": [],
            "plugin_hints": []
        }
    });

    let prepared = planner
        .prepare_ai_step(&task_claim)
        .await
        .expect("prepare retried Task step");

    let retry_context = prepared
        .request
        .prefixed_input_items
        .iter()
        .filter_map(|item| item["content"].as_str())
        .filter_map(|content| serde_json::from_str::<Value>(content).ok())
        .find(|content| content["type"] == "task_retry_context")
        .expect("program-owned retry context");
    assert_eq!(
        retry_context["instructions"],
        json!(["重新读取完整方案，不要复用上一次的截断结论"])
    );
    assert_eq!(retry_context["fresh_evidence_required"], true);
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
                        "plugin_key": {"type": "string", "enum": ["plugin-1"]},
                        "component_key": {"type": "string", "enum": ["plugin-1-mcp"]}
                    }, "required": ["plugin_key", "component_key"]}
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

#[tokio::test]
async fn main_chat_model_request_preserves_task_model_catalog_and_reasoning_policy() {
    struct TaskChoiceResolver(Vec<Value>);
    #[async_trait]
    impl LocalCapabilityResolver for TaskChoiceResolver {
        async fn resolve_capabilities(
            &self,
            _owner_user_id: &str,
            _profile_key: &str,
            _capability_policy_revision: &str,
        ) -> Result<ResolvedLocalCapabilities, String> {
            Ok(ResolvedLocalCapabilities {
                tools: self.0.clone(),
                ..ResolvedLocalCapabilities::default()
            })
        }
    }
    let model_schema = json!({
        "type": "string",
        "enum": ["model-simple", "model-complex"],
        "oneOf": [
            {"const": "model-simple", "title": "Luna - simple tasks"},
            {"const": "model-complex", "title": "Sol - complex tasks"}
        ],
        "x-enum-labels": ["Luna - simple tasks", "Sol - complex tasks"]
    });
    let thinking_schema = json!({
        "type": "string", "enum": ["low", "medium", "high"]
    });
    let mut controlled = Vec::new();
    let mut local = Vec::new();
    for (name, pointer) in [
        ("create_task", "/parameters/properties"),
        (
            "create_tasks_with_prerequisites",
            "/parameters/properties/tasks/items/properties",
        ),
    ] {
        let mut tool = json!({
            "name": name,
            "parameters": {"properties": {
                "tasks": {"items": {"properties": {}}}
            }}
        });
        let properties = tool.pointer_mut(pointer).unwrap().as_object_mut().unwrap();
        properties.insert("default_model_config_id".into(), model_schema.clone());
        properties.insert("thinking_level".into(), thinking_schema.clone());
        controlled.push(tool.clone());
        let properties = tool.pointer_mut(pointer).unwrap().as_object_mut().unwrap();
        properties.insert("default_model_config_id".into(), json!({"type": "string"}));
        properties.insert("thinking_level".into(), json!({"type": "string"}));
        properties.insert(
            "objective".into(),
            json!({"type": "string", "minLength": 1}),
        );
        local.push(tool);
    }
    let prepared = ControlPlaneLocalAiStepPlanner::main_chat(
        OwnerCheckingModelResolver,
        TaskChoiceResolver(controlled),
    )
    .with_local_tools(local.clone())
    .unwrap()
    .prepare_ai_step(&claim(Value::Null, None))
    .await
    .expect("prepare Main Chat model request");
    let merged = prepared.request.model_request.tools;
    assert_eq!(merged.len(), 2);
    for (tool, pointer) in merged.iter().zip([
        "/parameters/properties",
        "/parameters/properties/tasks/items/properties",
    ]) {
        let properties = tool.pointer(pointer).unwrap();
        assert_eq!(properties["default_model_config_id"], model_schema);
        assert_eq!(properties["thinking_level"], thinking_schema);
        assert_eq!(properties["objective"]["minLength"], 1);
    }
    // Legacy snapshots without choices must retain the local tool definitions.
    assert_eq!(merge_local_tools(vec![], &local, &[]).unwrap(), local);
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
            "capability_search",
            "task_run_process_record_process"
        ]
    );
}

#[test]
fn plugin_tasks_cannot_use_notepad_as_a_writable_fallback() {
    let tools = [
        "notepad_list_folders",
        "notepad_list_notes",
        "notepad_read_note",
        "notepad_create_note",
        "notepad_update_note",
        "capability_invoke",
    ]
    .map(|name| json!({"name": name}))
    .to_vec();
    let input = json!({"tool_options": {
        "requires_execution": true,
        "enabled_builtin_kinds": ["Notepad"],
        "plugin_hints": [{"plugin_key": "solution-studio", "component_key": "solution-authoring-mcp"}]
    }});

    let filtered = task_scoped_tools(tools, &input).expect("filter Plugin Task tools");
    assert_eq!(
        filtered.iter().filter_map(tool_name).collect::<Vec<_>>(),
        vec![
            "notepad_list_folders",
            "notepad_list_notes",
            "notepad_read_note",
            "capability_invoke",
        ]
    );
}

#[test]
fn unified_skill_controls_follow_effective_task_capabilities() {
    let tools = [
        "capability_search",
        "capability_describe",
        "capability_invoke",
        "agent_skill_activate",
        "agent_skill_read_resource",
    ]
    .map(|name| json!({"name": name}))
    .to_vec();
    for (execution, kinds, plugins, external, expected) in [
        (false, json!([]), json!([]), json!([]), false),
        (false, json!(["Notepad"]), json!([]), json!([]), false),
        (
            false,
            json!(["CodeMaintainerRead"]),
            json!([]),
            json!([]),
            true,
        ),
        (
            false,
            json!(["CodeMaintainerWrite", "TerminalController"]),
            json!([]),
            json!([]),
            false,
        ),
        (
            true,
            json!(["CodeMaintainerWrite"]),
            json!([]),
            json!([]),
            true,
        ),
        (
            true,
            json!(["TerminalController"]),
            json!([]),
            json!([]),
            true,
        ),
        (
            false,
            json!([]),
            json!([{"plugin_key": "plugin-1", "component_key": "plugin-1-mcp"}]),
            json!([]),
            true,
        ),
        (false, json!([]), json!([]), json!(["mcp-1"]), true),
    ] {
        let filtered = task_scoped_tools(
            tools.clone(),
            &json!({"tool_options": {
                "requires_execution": execution, "enabled_builtin_kinds": kinds,
                "plugin_hints": plugins, "external_mcp_config_ids": external
            }}),
        )
        .expect("filter unified controls");
        assert_eq!(filtered, if expected { tools.clone() } else { vec![] });
    }
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
            "project_write",
            "capability_search"
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
fn task_without_tool_options_is_rejected() {
    let tools = vec![
        json!({"name": "read_file"}),
        json!({"name": "execute_command"}),
    ];
    assert!(task_scoped_tools(tools, &json!({"prompt": "missing scope"})).is_err());
}

#[test]
fn failed_plugin_diagnosis_reaches_actual_function_call_output() {
    for profile in [MAIN_CHAT_PROFILE_KEY, TASK_EXECUTION_PROFILE_KEY] {
        let mut claim = claim(
            json!({"response": {
                "request_input_items": [{"role": "user", "content": "browse"}],
                "response_output_items": [{"type": "function_call", "call_id": "call-1", "name": "capability_invoke", "arguments": "{}"}]
            }}),
            Some(json!({"type": "tool_results", "invocations": [{
                "call_id": "call-1", "tool_name": "capability_invoke", "status": "failed",
                "error": "native_tool_reported_error",
                "result": {"content": {"content": [{"type": "text", "text": "no browser session is bound; call browser_session_open first"}]}, "is_error": true}
            }]})),
        );
        claim.run.profile_key = profile.to_string();
        let (items, _) = durable_step_input(&claim, "message").unwrap();
        let output = items
            .iter()
            .find(|item| item["type"] == "function_call_output")
            .unwrap();
        assert_eq!(output["call_id"], "call-1");
        assert_eq!(output["output"], "native_tool_reported_error\nno browser session is bound; call browser_session_open first");
    }
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
async fn main_chat_receives_the_program_owned_composer_capability_contract() {
    let planner = ControlPlaneLocalAiStepPlanner::main_chat(
        OwnerCheckingModelResolver,
        ManagedFollowupCapabilities,
    );
    let mut selected = claim(Value::Null, None);
    selected.run.input = json!({
        "message": "整理成正式文档",
        "explicit_capability_selection": {
            "kind": "plugin",
            "plugin_key": "document-plugin",
            "component_key": "document-mcp",
            "display_name": "Document"
        }
    });

    let prepared = planner
        .prepare_ai_step(&selected)
        .await
        .expect("prepare explicit composer selection");
    let contract = prepared
        .request
        .prefixed_input_items
        .iter()
        .find_map(|item| {
            let value: Value = serde_json::from_str(item["content"].as_str()?).ok()?;
            (value["type"] == "user_selected_capability_contract").then_some(value)
        })
        .expect("program-owned selection contract");
    assert_eq!(contract["delivery_route"], "selected_plugin_required");
    assert_eq!(contract["selection"]["plugin_key"], "document-plugin");
}

#[tokio::test]
async fn composer_selected_plugin_cannot_be_rerouted_into_requirement_survey() {
    let planner = ControlPlaneLocalAiStepPlanner::main_chat(
        OwnerCheckingModelResolver,
        ManagedFollowupCapabilities,
    )
    .with_local_tools(vec![
        json!({
            "type": "function",
            "name": "create_task",
            "parameters": {"type": "object", "properties": {}}
        }),
        json!({
            "type": "function",
            "name": "requirement_survey_create",
            "parameters": {"type": "object", "properties": {}}
        }),
        json!({
            "type": "function",
            "name": "requirement_plan_save",
            "parameters": {"type": "object", "properties": {}}
        }),
    ])
    .expect("local Main Chat tools");
    let mut selected = claim(Value::Null, None);
    selected.run.input = json!({
        "message": "帮我设计咖啡店活动网页首屏",
        "explicit_capability_selection": {
            "kind": "plugin",
            "plugin_key": "web-design-plugin",
            "component_key": "web-design-mcp",
            "display_name": "Web Design Studio"
        }
    });

    let prepared = planner
        .prepare_ai_step(&selected)
        .await
        .expect("prepare explicit Plugin selection");
    let tool_names = prepared
        .request
        .model_request
        .tools
        .iter()
        .filter_map(|tool| {
            tool.get("name")
                .and_then(Value::as_str)
                .or_else(|| tool.pointer("/function/name").and_then(Value::as_str))
        })
        .collect::<Vec<_>>();

    assert!(tool_names.contains(&"create_task"));
    assert!(!tool_names.contains(&"requirement_survey_create"));
    assert!(!tool_names.contains(&"requirement_plan_save"));
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
    let fact: Value = serde_json::from_str(guidance["content"].as_str().unwrap()).unwrap();
    assert_eq!(
        fact,
        json!({"type": "continued_work_accepted", "accepted": true})
    );
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

#[tokio::test]
async fn guidance_after_handoff_keeps_task_control_tools_and_latest_intent() {
    let planner = ControlPlaneLocalAiStepPlanner::main_chat(
        OwnerCheckingModelResolver,
        OwnerCheckingCapabilityResolver,
    )
    .with_local_tools(vec![json!({
        "type": "function", "name": "cancel_task", "parameters": {}
    })])
    .expect("task control tools");
    let prepared = planner.prepare_ai_step(&claim(
        json!({"response": {
            "request_input_items": [{"role": "user", "content": "publish it"}],
            "response_output_items": [{
                "type": "function_call", "call_id": "handoff",
                "name": "wait_for_task_completion", "arguments": "{}"
            }]
        }}),
        Some(json!({
            "type": "tool_results",
            "invocations": [{
                "call_id": "handoff", "tool_name": "wait_for_task_completion",
                "status": "succeeded", "result": {"accepted": true}
            }],
            "guidance": [{"message_id": "guidance-1", "message": "Stop, do not publish", "attachments": []}]
        })),
    )).await.expect("handle latest guidance instead of closing handoff");
    assert!(prepared
        .request
        .model_request
        .tools
        .iter()
        .any(|tool| tool["name"] == "cancel_task"));
    assert_eq!(
        prepared.request.current_input_items.last().unwrap()["content"],
        "Stop, do not publish"
    );
    assert_eq!(prepared.reason, "tool_results_with_guidance");
    // Behavioral policy is supplied by the managed capability snapshot, not Rust.
    assert!(prepared.request.model_request.instructions.is_none());
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

struct ManagedFollowupCapabilities;

#[tokio::test]
async fn runtime_clock_is_present_on_main_and_task_steps() {
    for is_task in [false, true] {
        let planner = if is_task {
            ControlPlaneLocalAiStepPlanner::task_execution(
                OwnerCheckingModelResolver,
                ManagedFollowupCapabilities,
            )
        } else {
            ControlPlaneLocalAiStepPlanner::main_chat(
                OwnerCheckingModelResolver,
                ManagedFollowupCapabilities,
            )
        };
        let mut run_claim = claim(Value::Null, None);
        run_claim.run.profile_key = if is_task {
            TASK_EXECUTION_PROFILE_KEY
        } else {
            MAIN_CHAT_PROFILE_KEY
        }
        .into();
        run_claim.run.input = json!({"message":"Check publication dates", "prompt":"Check publication dates", "current_date":"1999-01-01"});
        if is_task {
            run_claim.run.input["tool_options"] = json!({
                "requires_execution": false,
                "enabled_builtin_kinds": [],
                "plugin_hints": []
            });
        }
        let prepared = planner.prepare_ai_step(&run_claim).await.unwrap();
        let clock = prepared
            .request
            .prefixed_input_items
            .iter()
            .find_map(|item| {
                let data: Value = serde_json::from_str(item["content"].as_str()?).ok()?;
                (data["type"] == "runtime_clock_context").then_some(data)
            })
            .expect("runtime-owned date context");
        let instant =
            chrono::DateTime::parse_from_rfc3339(clock["current_time"].as_str().unwrap()).unwrap();
        assert!(
            (chrono::Utc::now() - instant.with_timezone(&chrono::Utc))
                .num_seconds()
                .abs()
                < 5
        );
        assert_eq!(
            clock["current_date"],
            instant.format("%Y-%m-%d").to_string()
        );
        assert_eq!(clock["utc_offset"], instant.format("%:z").to_string());
        assert_ne!(clock["current_date"], "1999-01-01");
    }
}

#[async_trait]
impl LocalCapabilityResolver for ManagedFollowupCapabilities {
    async fn resolve_capabilities(
        &self,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<ResolvedLocalCapabilities, String> {
        Ok(ResolvedLocalCapabilities {
            instructions: Some("Published managed follow-up policy, revision 7".into()),
            ..ResolvedLocalCapabilities::default()
        })
    }
}

#[tokio::test]
async fn managed_prompt_is_authoritative_and_pending_review_prevents_handoff_closure() {
    let planner = ControlPlaneLocalAiStepPlanner::main_chat(
        OwnerCheckingModelResolver,
        ManagedFollowupCapabilities,
    )
    .with_local_tools(vec![
        json!({"type": "function", "name": "resolve_task_followup", "parameters": {}}),
    ])
    .unwrap();
    let mut pending = claim(
        Value::Null,
        Some(json!({"type": "tool_results", "invocations": [{
            "tool_name": "wait_for_task_completion", "status": "succeeded", "result": {"accepted": true}
        }]})),
    );
    let review = json!({"type": "task_followup_review", "review_message_id": "latest", "pending": true, "tasks": [{"task_id": "old"}]});
    pending.run.input["task_followup_review"] = review.clone();
    let prepared = planner.prepare_ai_step(&pending).await.unwrap();
    assert_eq!(
        prepared.request.model_request.instructions.as_deref(),
        Some("Published managed follow-up policy, revision 7")
    );
    assert_eq!(
        prepared.request.model_request.tools[0]["name"],
        "resolve_task_followup"
    );
    assert!(prepared.request.prefixed_input_items.iter().any(|item| {
        item["content"]
            .as_str()
            .and_then(|text| serde_json::from_str::<Value>(text).ok())
            == Some(review.clone())
    }));
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

#[test]
fn survey_answers_resume_the_matching_tool_call_for_main_chat_and_task() {
    for profile in [MAIN_CHAT_PROFILE_KEY, TASK_EXECUTION_PROFILE_KEY] {
        let checkpoint = json!({"response": {
            "request_input_items": [{"role": "user", "content": "help define the project"}],
            "response_output_items": [{"type": "function_call", "name": "requirement_survey_create",
                "call_id": "survey-call-1", "arguments": "{}"}]
        }});
        let continuation = json!({"type": "requirement_survey_resolved", "survey_id": "survey-1",
            "answers": {"scope": "prototype", "editable": true}});
        let mut run_claim = claim(checkpoint, Some(continuation));
        run_claim.run.profile_key = profile.into();
        let (items, reason) = durable_step_input(&run_claim, "message").unwrap();
        assert_eq!(reason, "requirement_survey_resolved");
        let output = items.last().unwrap();
        assert_eq!(output["type"], "function_call_output");
        assert_eq!(output["call_id"], "survey-call-1");
        let value: Value = serde_json::from_str(output["output"].as_str().unwrap()).unwrap();
        assert_eq!(value["answers"]["scope"], "prototype");
        assert_eq!(value["answers"]["editable"], true);
        assert_eq!(items.len(), 3);
    }
    assert!(requirement_survey_output_item(&[], &json!({"answers": {}})).is_err());
    assert!(requirement_survey_output_item(
        &[json!({"type": "function_call",
        "name": "requirement_survey_create", "call_id": "call-1"})],
        &json!({})
    )
    .is_err());
}
