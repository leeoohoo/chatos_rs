// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::mcp_server::support::{
    create_model_config_schema, create_project_execution_tasks_schema,
    create_tasks_with_prerequisites_schema, update_model_config_schema,
};
use crate::mcp_server::PROJECT_REQUIREMENT_EXECUTION_PLANNER_TOOL_PROFILE;
use crate::mcp_server::{reject_ai_runtime_config, support::remove_internal_task_fields};
use serde_json::Value;

#[test]
fn create_task_schema_hides_memory_scope_fields() {
    let schema = create_task_schema();
    let properties = schema
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("object properties");

    assert!(!properties.contains_key("tenant_id"));
    assert!(!properties.contains_key("subject_id"));
    assert!(!properties.contains_key("status"));
    assert!(!properties.contains_key("mcp_config"));
    assert!(properties.contains_key("default_model_config_id"));
    assert!(properties.contains_key("enabled_builtin_kinds"));
    assert!(properties.contains_key("external_mcp_config_ids"));
    assert!(!properties.contains_key("selected_skill_ids"));
    assert!(!properties.contains_key("plugin_device_id"));
    assert!(!properties.contains_key("plugin_workspace_id"));
    assert!(!properties.contains_key("selected_plugins"));
}

#[test]
fn model_config_thinking_level_schema_is_enum_choice() {
    let create_schema = create_model_config_schema();
    let update_schema = update_model_config_schema();

    for schema in [create_schema, update_schema] {
        let thinking_level = schema
            .pointer("/properties/thinking_level")
            .and_then(|value| value.as_object())
            .expect("thinking_level schema");
        let values = thinking_level
            .get("enum")
            .and_then(|value| value.as_array())
            .expect("thinking_level enum");

        assert_eq!(
            thinking_level.get("type").and_then(|value| value.as_str()),
            Some("string")
        );
        assert!(values.iter().any(|value| value.as_str() == Some("low")));
        assert!(values.iter().any(|value| value.as_str() == Some("xhigh")));
        assert!(values.iter().any(|value| value.as_str() == Some("auto")));
    }
}

#[test]
fn planner_task_creation_schemas_require_explicit_task_nature() {
    let create = create_task_schema();
    assert!(create.pointer("/properties/is_planning_task").is_some());

    let batch = create_tasks_with_prerequisites_schema();
    assert!(batch
        .pointer("/properties/tasks/items/properties/is_planning_task")
        .is_some());

    let project = create_project_execution_tasks_schema();
    assert!(project.pointer("/properties/execution_group_id").is_none());
    assert!(project
        .pointer("/properties/tasks/items/properties/is_planning_task")
        .is_none());
    assert!(project
        .pointer("/properties/tasks/items/properties/requires_execution")
        .is_none());
    let required = project
        .pointer("/properties/tasks/items/required")
        .and_then(|value| value.as_array())
        .expect("project execution task required fields");
    assert!(!required
        .iter()
        .any(|value| value.as_str() == Some("is_planning_task")));
}

#[test]
fn ai_task_input_cannot_supply_mcp_configuration() {
    let error = CreateTaskArgs {
        title: "task".to_string(),
        description: None,
        objective: "run selected service".to_string(),
        input_payload: None,
        priority: None,
        tags: None,
        default_model_config_id: None,
        requires_execution: Some(true),
        is_planning_task: Some(false),
        schedule: None,
        enabled_builtin_kinds: None,
        external_mcp_config_ids: None,
        plugin_device_id: None,
        plugin_workspace_id: None,
        selected_plugins: None,
        prerequisite_task_ids: None,
        mcp_config: Some(TaskMcpConfig {
            execution_service_id: Some("services-api".to_string()),
            ..TaskMcpConfig::default()
        }),
    }
    .into_request()
    .expect_err("AI MCP configuration must be rejected");

    assert!(error.contains("controlled by the program"));
}

#[test]
fn update_task_schema_hides_execution_status() {
    let schema = update_task_schema();
    let properties = schema
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("object properties");

    assert!(!properties.contains_key("status"));
    assert!(!properties.contains_key("mcp_config"));
    assert!(!properties.contains_key("plugin_config"));
}

#[test]
fn ai_task_update_rejects_program_managed_runtime_configuration() {
    let plugin_config = chatos_plugin_management_sdk::TaskPluginConfig {
        device_id: Some("device-1".to_string()),
        ..Default::default()
    };

    let error = reject_ai_runtime_config(None, Some(&plugin_config))
        .expect_err("AI Plugin routing update must fail closed");

    assert!(error.contains("controlled by the program"));
}

#[test]
fn agent_task_payload_hides_identity_and_runtime_routing_snapshots() {
    let mut payload = json!({
        "id": "task-1",
        "title": "business task",
        "project_id": "project-1",
        "owner_user_id": "owner-a",
        "source_session_id": "session-a",
        "default_model_config_id": "model-a",
        "plugin_config": { "device_id": "device-a" },
        "mcp_config": { "execution_service_id": "service-a" },
        "nested": {
            "tenant_id": "tenant-a",
            "subject_id": "subject-a"
        }
    });

    remove_internal_task_fields(&mut payload);

    assert_eq!(
        payload.pointer("/id").and_then(Value::as_str),
        Some("task-1")
    );
    for pointer in [
        "/project_id",
        "/owner_user_id",
        "/source_session_id",
        "/default_model_config_id",
        "/plugin_config",
        "/mcp_config",
        "/nested/tenant_id",
        "/nested/subject_id",
    ] {
        assert!(payload.pointer(pointer).is_none(), "leaked {pointer}");
    }
}

#[test]
fn external_mcp_tools_hide_internal_process_recorder() {
    assert!(!agent_tool_allowed("record_task_process"));
    assert!(!agent_tool_allowed("list_model_configs"));
    assert!(!agent_tool_allowed("get_model_config"));
}

#[test]
fn default_agent_hides_direct_history_status_tools() {
    assert!(!agent_tool_allowed("batch_update_task_status"));
    assert!(!agent_tool_allowed("retry_run"));
    assert!(agent_tool_allowed("start_task_run"));
    assert!(agent_tool_allowed("cancel_task"));
}

#[test]
fn create_task_args_preserve_agent_mcp_capability_selection() {
    let request = CreateTaskArgs {
        title: "task".to_string(),
        description: None,
        objective: "use external tools".to_string(),
        input_payload: None,
        priority: None,
        tags: None,
        default_model_config_id: None,
        requires_execution: None,
        is_planning_task: None,
        schedule: None,
        enabled_builtin_kinds: None,
        external_mcp_config_ids: Some(vec![
            " external-mcp-1 ".to_string(),
            String::new(),
            "external-mcp-1".to_string(),
        ]),
        plugin_device_id: None,
        plugin_workspace_id: None,
        selected_plugins: None,
        prerequisite_task_ids: None,
        mcp_config: None,
    }
    .into_request()
    .expect("Agent MCP selection");

    assert_eq!(
        request
            .mcp_config
            .expect("MCP request")
            .external_mcp_config_ids,
        vec![
            " external-mcp-1 ".to_string(),
            String::new(),
            "external-mcp-1".to_string(),
        ]
    );
}

#[test]
fn read_only_code_task_cannot_be_misclassified_as_requiring_execution() {
    let request = CreateTaskArgs {
        title: "review task".to_string(),
        description: None,
        objective: "read the project without changing files".to_string(),
        input_payload: None,
        priority: None,
        tags: None,
        default_model_config_id: None,
        requires_execution: Some(true),
        is_planning_task: Some(false),
        schedule: None,
        enabled_builtin_kinds: Some(vec!["CodeMaintainerRead".to_string()]),
        external_mcp_config_ids: None,
        plugin_device_id: None,
        plugin_workspace_id: None,
        selected_plugins: None,
        prerequisite_task_ids: None,
        mcp_config: None,
    }
    .into_request()
    .expect("read-only task request");

    assert_eq!(
        request.mcp_config.expect("MCP request").requires_execution,
        Some(false)
    );
}

#[test]
fn create_task_args_reject_ai_plugin_device_workspace_and_selection() {
    let error = CreateTaskArgs {
        title: "browser task".to_string(),
        description: None,
        objective: "control browser".to_string(),
        input_payload: None,
        priority: None,
        tags: None,
        default_model_config_id: None,
        requires_execution: Some(true),
        is_planning_task: Some(false),
        schedule: None,
        enabled_builtin_kinds: None,
        external_mcp_config_ids: None,
        plugin_device_id: Some(" device-1 ".to_string()),
        plugin_workspace_id: Some(" workspace-1 ".to_string()),
        selected_plugins: Some(vec![
            chatos_plugin_management_sdk::SelectedPluginRef {
                plugin_id: " plugin-browser ".to_string(),
                selected_skill_ids: Vec::new(),
                selected_command_ids: Vec::new(),
                selected_agent_ids: Vec::new(),
            },
            chatos_plugin_management_sdk::SelectedPluginRef {
                plugin_id: "plugin-browser".to_string(),
                selected_skill_ids: Vec::new(),
                selected_command_ids: Vec::new(),
                selected_agent_ids: Vec::new(),
            },
        ]),
        prerequisite_task_ids: None,
        mcp_config: None,
    }
    .into_request()
    .expect_err("AI Plugin routing must fail closed");

    assert!(error.contains("controlled by the program"));
}

#[test]
fn request_context_plugin_selection_is_applied_without_ai_plugin_input() {
    let mut request = CreateTaskArgs {
        title: "browser task".to_string(),
        description: None,
        objective: "control browser".to_string(),
        input_payload: None,
        priority: None,
        tags: None,
        default_model_config_id: None,
        requires_execution: Some(true),
        is_planning_task: Some(false),
        schedule: None,
        enabled_builtin_kinds: None,
        external_mcp_config_ids: None,
        plugin_device_id: None,
        plugin_workspace_id: None,
        selected_plugins: None,
        prerequisite_task_ids: None,
        mcp_config: None,
    }
    .into_request()
    .expect("model request");
    let context = McpRequestContext {
        plugin_config_override: Some(chatos_plugin_management_sdk::TaskPluginConfig {
            device_id: Some("user-device".to_string()),
            workspace_id: Some("user-workspace".to_string()),
            selected_plugins: vec![chatos_plugin_management_sdk::SelectedPluginRef {
                plugin_id: "user-plugin".to_string(),
                selected_skill_ids: Vec::new(),
                selected_command_ids: vec!["review".to_string()],
                selected_agent_ids: Vec::new(),
            }],
            command_invocations: vec![chatos_plugin_management_sdk::PluginCommandInvocation {
                plugin_id: "user-plugin".to_string(),
                command_id: "review".to_string(),
                arguments: Some("src/lib.rs".to_string()),
            }],
        }),
        ..McpRequestContext::default()
    };

    context.enforce_plugin_config(&mut request);

    assert_eq!(
        request.plugin_config.device_id.as_deref(),
        Some("user-device")
    );
    assert_eq!(
        request.plugin_config.workspace_id.as_deref(),
        Some("user-workspace")
    );
    assert_eq!(request.plugin_config.selected_plugins.len(), 1);
    assert_eq!(
        request.plugin_config.selected_plugins[0].plugin_id,
        "user-plugin"
    );
    assert_eq!(
        request.plugin_config.command_invocations,
        vec![chatos_plugin_management_sdk::PluginCommandInvocation {
            plugin_id: "user-plugin".to_string(),
            command_id: "review".to_string(),
            arguments: Some("src/lib.rs".to_string()),
        }]
    );
}

#[test]
fn mcp_model_list_is_strictly_scoped_to_current_owner() {
    let current_user = admin_user("user-1");
    let models = vec![
        model_config("own-enabled", "user-1", true),
        model_config("other-enabled", "user-2", true),
        model_config("own-disabled", "user-1", false),
    ];

    let visible = model_configs_for_user(models, &current_user);

    assert_eq!(visible.len(), 1);
    assert_eq!(
        visible[0].get("id").and_then(|value| value.as_str()),
        Some("own-enabled")
    );
    assert_eq!(
        visible[0].get("api_key").and_then(|value| value.as_str()),
        Some("")
    );
}

#[test]
fn mcp_tool_schema_exposes_only_current_owner_enabled_model_choices() {
    let current_user = admin_user("user-1");
    let models = vec![
        model_config("own-enabled", "user-1", true),
        model_config("other-enabled", "user-2", true),
        model_config("own-disabled", "user-1", false),
    ];
    let visible_models = filter_model_configs_for_user(models, &current_user);
    let mut tools = vec![json!({
        "name": "create_task",
        "inputSchema": create_task_schema(),
    })];

    enrich_tool_schemas_with_model_configs(&mut tools, &visible_models);
    let model_schema = tools[0]
        .pointer("/inputSchema/properties/default_model_config_id")
        .expect("model selection schema");
    let enum_values = model_schema
        .get("enum")
        .and_then(serde_json::Value::as_array)
        .expect("model enum");
    assert_eq!(enum_values, &vec![json!("own-enabled")]);
    let choices = model_schema
        .get("oneOf")
        .and_then(serde_json::Value::as_array)
        .expect("model choices");
    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0].get("const"), Some(&json!("own-enabled")));
    assert!(choices[0]
        .get("title")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| value.contains("own-enabled")));
}

#[test]
fn batch_create_schema_exposes_model_selection_for_each_task() {
    let current_user = admin_user("user-1");
    let visible_models =
        filter_model_configs_for_user(vec![model_config("model-a", "user-1", true)], &current_user);
    let mut tools = vec![json!({
        "name": "create_tasks_with_prerequisites",
        "inputSchema": create_tasks_with_prerequisites_schema(),
    })];

    enrich_tool_schemas_with_model_configs(&mut tools, &visible_models);

    assert_eq!(
        tools[0].pointer(
            "/inputSchema/properties/tasks/items/properties/default_model_config_id/enum/0"
        ),
        Some(&json!("model-a"))
    );
}

#[test]
fn task_creation_schema_exposes_agent_bound_mcp_choices() {
    let mut tools = vec![json!({
        "name": "create_task",
        "inputSchema": create_task_schema(),
    })];
    super::super::support::enrich_tool_schemas_with_task_mcp_choices(
        &mut tools,
        &[super::super::support::TaskMcpSchemaChoice {
            value: "CodeMaintainerWrite".to_string(),
            title: "Code write [execution task]".to_string(),
        }],
        &[super::super::support::TaskMcpSchemaChoice {
            value: "postgres-mcp".to_string(),
            title: "PostgreSQL (postgres-mcp) [execution task]".to_string(),
        }],
    );

    assert_eq!(
        tools[0].pointer("/inputSchema/properties/enabled_builtin_kinds/items/enum/0"),
        Some(&json!("CodeMaintainerWrite"))
    );
    assert_eq!(
        tools[0].pointer("/inputSchema/properties/external_mcp_config_ids/items/enum/0"),
        Some(&json!("postgres-mcp"))
    );
}

#[test]
fn async_planner_profile_exposes_only_planning_tools() {
    assert!(chatos_async_planner::planner_agent_tool_allowed(
        "list_tasks"
    ));
    assert!(chatos_async_planner::planner_agent_tool_allowed("get_task"));
    assert!(chatos_async_planner::planner_agent_tool_allowed(
        "create_task"
    ));
    assert!(chatos_async_planner::planner_agent_tool_allowed(
        "create_tasks_with_prerequisites"
    ));
    assert!(!chatos_async_planner::planner_agent_tool_allowed(
        "list_mcp_builtin_catalog"
    ));
    assert!(!chatos_async_planner::planner_agent_tool_allowed(
        "list_available_skills"
    ));
    assert!(!chatos_async_planner::planner_agent_tool_allowed(
        "list_available_plugins"
    ));
    assert!(chatos_async_planner::planner_agent_tool_allowed(
        "cancel_task"
    ));
    assert!(chatos_async_planner::planner_agent_tool_allowed(
        "get_task_dependency_graph"
    ));
    assert!(!chatos_async_planner::planner_agent_tool_allowed(
        "delete_task"
    ));
    assert!(!chatos_async_planner::planner_agent_tool_allowed(
        "batch_delete_tasks"
    ));
    assert!(!chatos_async_planner::planner_agent_tool_allowed(
        "batch_update_task_status"
    ));
    assert!(!chatos_async_planner::planner_agent_tool_allowed(
        "start_task_run"
    ));
    assert!(!chatos_async_planner::planner_agent_tool_allowed(
        "list_runs"
    ));
    assert!(!chatos_async_planner::planner_agent_tool_allowed("get_run"));
    assert!(!chatos_async_planner::planner_agent_tool_allowed(
        "list_run_events"
    ));
    assert!(!chatos_async_planner::planner_agent_tool_allowed(
        "get_task_stats"
    ));
    assert!(!chatos_async_planner::planner_agent_tool_allowed(
        "update_task"
    ));
    assert!(!chatos_async_planner::planner_agent_tool_allowed(
        "set_task_prerequisites"
    ));
}

#[tokio::test]
async fn provider_descriptor_exposes_all_chatos_planner_profile_tools() {
    let (service, _, _) = test_mcp_service().await;
    let descriptor = service.provider_descriptor();
    let tool_names = descriptor
        .tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(serde_json::Value::as_str))
        .collect::<Vec<_>>();

    assert_eq!(tool_names.len(), 8);
    for expected in [
        "list_tasks",
        "get_task",
        "create_task",
        "create_tasks_with_prerequisites",
        "create_project_execution_tasks",
        "cancel_task",
        "wait_for_task_completion",
        "get_task_dependency_graph",
    ] {
        assert!(tool_names.contains(&expected), "missing {expected}");
    }
    for hidden in [
        "get_task_stats",
        "list_available_plugins",
        "list_available_skills",
        "update_task",
        "set_task_prerequisites",
        "list_model_configs",
        "start_task_run",
        "list_runs",
    ] {
        assert!(!tool_names.contains(&hidden), "unexpected {hidden}");
    }
}

#[test]
fn async_planner_update_task_cannot_change_status() {
    let patch = UpdateTaskRequest {
        status: Some(TaskStatus::Ready),
        ..UpdateTaskRequest::default()
    };
    assert!(chatos_async_planner::planner_update_task_request(patch).is_err());

    let patch = UpdateTaskRequest {
        objective: Some("updated objective".to_string()),
        ..UpdateTaskRequest::default()
    };
    assert!(chatos_async_planner::planner_update_task_request(patch).is_ok());
}

#[test]
fn async_planner_preserves_only_execution_intent_before_programmatic_resolution() {
    let request = CreateTaskRequest {
        title: "task".to_string(),
        description: None,
        objective: "objective".to_string(),
        input_payload: None,
        status: None,
        priority: None,
        tags: None,
        default_model_config_id: None,
        project_id: None,
        task_profile: None,
        tenant_id: None,
        subject_id: None,
        schedule: None,
        plugin_config: Default::default(),
        mcp_config: Some(TaskMcpRequestConfig {
            requires_execution: Some(false),
            ..TaskMcpRequestConfig::default()
        }),
        prerequisite_task_ids: None,
    };
    assert!(chatos_async_planner::ensure_planner_required_fields(&request).is_ok());
    let planned =
        chatos_async_planner::planner_root_create_request(request, &McpRequestContext::default())
            .expect("planner request");
    assert_eq!(
        planned.mcp_config.expect("mcp config").requires_execution,
        Some(false)
    );
}

#[test]
fn async_planner_schema_exposes_mcp_selection_without_runtime_routing() {
    let mut tools = vec![json!({
        "name": "create_task",
        "inputSchema": create_task_schema(),
    })];

    chatos_async_planner::enrich_tool_schemas_for_async_planner(&mut tools, &[]);

    let input_schema = tools[0].get("inputSchema").expect("input schema");
    assert!(input_schema.get("anyOf").is_none());
    assert!(input_schema
        .pointer("/properties/enabled_builtin_kinds")
        .is_some());
    assert!(input_schema
        .pointer("/properties/external_mcp_config_ids")
        .is_some());
    assert!(input_schema
        .pointer("/properties/plugin_device_id")
        .is_none());
    assert!(input_schema
        .pointer("/properties/plugin_workspace_id")
        .is_none());
    assert!(input_schema
        .pointer("/properties/selected_plugins")
        .is_none());
}

#[test]
fn async_planner_batch_schema_exposes_per_task_mcp_selection() {
    let mut tools = vec![json!({
        "name": "create_tasks_with_prerequisites",
        "inputSchema": super::super::support::create_tasks_with_prerequisites_schema(),
    })];

    chatos_async_planner::enrich_tool_schemas_for_async_planner(&mut tools, &[]);

    let input_schema = tools[0].get("inputSchema").expect("input schema");
    assert!(input_schema
        .pointer("/properties/tasks/items/anyOf")
        .is_none());
    assert!(input_schema
        .pointer("/properties/tasks/items/properties/enabled_builtin_kinds")
        .is_some());
    assert!(input_schema
        .pointer("/properties/tasks/items/properties/external_mcp_config_ids")
        .is_some());
    assert!(input_schema
        .pointer("/properties/tasks/items/properties/plugin_device_id")
        .is_none());
    assert!(input_schema
        .pointer("/properties/tasks/items/properties/plugin_workspace_id")
        .is_none());
    assert!(input_schema
        .pointer("/properties/tasks/items/properties/selected_plugins")
        .is_none());
}

#[test]
fn async_planner_update_schema_does_not_expose_mcp_configuration() {
    let mut tools = vec![json!({
        "name": "update_task",
        "inputSchema": json!({
            "type": "object",
            "properties": {
                "patch": super::super::support::update_task_schema()
            }
        }),
    })];

    chatos_async_planner::enrich_tool_schemas_for_async_planner(&mut tools, &[]);

    let input_schema = tools[0].get("inputSchema").expect("input schema");
    let properties = input_schema
        .pointer("/properties/patch/properties")
        .and_then(|value| value.as_object())
        .expect("patch properties");
    assert!(!properties.contains_key("status"));
    assert!(!properties.contains_key("mcp_config"));
    assert!(!properties.contains_key("plugin_config"));
}

#[test]
fn async_planner_root_tasks_are_forced_to_contact_async_schedule() {
    let request = valid_planner_create_request();
    let planned =
        chatos_async_planner::planner_root_create_request(request, &McpRequestContext::default())
            .expect("planner request");
    assert_eq!(
        planned.schedule.expect("schedule").mode,
        TaskScheduleMode::ContactAsync
    );
}

#[test]
fn async_planner_prerequisite_tasks_are_forced_to_contact_async_schedule() {
    let request = valid_planner_create_request();
    let planned = chatos_async_planner::planner_prerequisite_create_request(
        request,
        &McpRequestContext::default(),
    )
    .expect("planner request");
    assert_eq!(
        planned.schedule.expect("schedule").mode,
        TaskScheduleMode::ContactAsync
    );
}

#[test]
fn mcp_request_context_infers_async_planner_from_chatos_message_context() {
    let context = McpRequestContext {
        source_session_id: Some("session-1".to_string()),
        source_user_message_id: Some("message-1".to_string()),
        ..McpRequestContext::default()
    };
    assert_eq!(context.tool_profile(), McpToolProfile::ChatosAsyncPlanner);

    let missing_user_message = McpRequestContext {
        source_session_id: Some("session-1".to_string()),
        source_turn_id: Some("turn-1".to_string()),
        ..McpRequestContext::default()
    };
    assert_eq!(missing_user_message.tool_profile(), McpToolProfile::Default);
}

#[test]
fn mcp_request_context_normalizes_legacy_public_project_scope() {
    let context = McpRequestContext {
        project_id: Some("0".to_string()),
        ..McpRequestContext::default()
    };

    assert_eq!(
        context.project_scope_id().as_deref(),
        Some(PUBLIC_PROJECT_ID)
    );
}

#[test]
fn mcp_request_context_detects_chatos_plan_task_profile() {
    let context = McpRequestContext {
        task_profile: Some(TASK_PROFILE_CHATOS_PLAN.to_string()),
        ..McpRequestContext::default()
    };
    assert!(context.is_chatos_plan_task_profile());
    assert_eq!(context.requested_task_profile(), TASK_PROFILE_CHATOS_PLAN);

    let context = McpRequestContext {
        chatos_plan_mode: true,
        ..McpRequestContext::default()
    };
    assert!(context.is_chatos_plan_task_profile());
    assert_eq!(context.requested_task_profile(), TASK_PROFILE_CHATOS_PLAN);
}

#[test]
fn chatos_plan_context_assigns_child_profile_from_task_nature() {
    let context = McpRequestContext {
        task_profile: Some(TASK_PROFILE_CHATOS_PLAN.to_string()),
        ..McpRequestContext::default()
    };

    assert_eq!(
        context.child_task_profile(Some(true)).as_deref(),
        Some(TASK_PROFILE_CHATOS_PLAN)
    );
    assert_eq!(
        context.child_task_profile(Some(false)).as_deref(),
        Some(TASK_PROFILE_DEFAULT)
    );
    assert_eq!(
        context.child_task_profile(None).as_deref(),
        Some(TASK_PROFILE_CHATOS_PLAN)
    );
    assert_eq!(
        context.child_task_profile(None).as_deref(),
        Some(TASK_PROFILE_CHATOS_PLAN)
    );
}

#[test]
fn project_execution_planner_always_creates_ordinary_execution_tasks() {
    let context = McpRequestContext {
        tool_profile: Some(PROJECT_REQUIREMENT_EXECUTION_PLANNER_TOOL_PROFILE.to_string()),
        ..McpRequestContext::default()
    };

    assert_eq!(
        context.child_task_profile(Some(true)).as_deref(),
        Some(TASK_PROFILE_DEFAULT)
    );
    assert_eq!(
        context.child_task_profile(Some(false)).as_deref(),
        Some(TASK_PROFILE_DEFAULT)
    );
    assert_eq!(
        context.child_task_profile(None).as_deref(),
        Some(TASK_PROFILE_DEFAULT)
    );
}
