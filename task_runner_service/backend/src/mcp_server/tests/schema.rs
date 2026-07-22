// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::mcp_server::support::{
    create_model_config_schema, create_project_execution_tasks_schema,
    create_tasks_with_prerequisites_schema, update_model_config_schema,
};

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

    let kind_enum = properties
        .get("enabled_builtin_kinds")
        .and_then(|value| value.get("items"))
        .and_then(|value| value.get("enum"))
        .and_then(|value| value.as_array())
        .expect("enabled_builtin_kinds enum");
    assert!(kind_enum
        .iter()
        .any(|value| value.as_str() == Some("WebTools")));
    assert!(kind_enum
        .iter()
        .any(|value| value.as_str() == Some("RemoteConnectionController")));
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
fn task_mcp_config_schema_hides_host_passthrough_fields() {
    let schema = task_mcp_config_schema();
    let properties = schema
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("object properties");

    assert!(!properties.contains_key("workspace_dir"));
    assert!(!properties.contains_key("default_remote_server_id"));
    assert!(!properties.contains_key("execution_service_id"));
    assert!(properties.contains_key("enabled_builtin_kinds"));
    assert!(properties.contains_key("external_mcp_config_ids"));
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
    assert!(project
        .pointer("/properties/tasks/items/properties/is_planning_task")
        .is_some());
    let required = project
        .pointer("/properties/tasks/items/required")
        .and_then(|value| value.as_array())
        .expect("project execution task required fields");
    assert!(required
        .iter()
        .any(|value| value.as_str() == Some("is_planning_task")));
}

#[test]
fn ai_task_input_cannot_select_execution_service() {
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
        selected_skill_ids: None,
        prerequisite_task_ids: None,
        mcp_config: Some(TaskMcpConfig {
            execution_service_id: Some("services-api".to_string()),
            ..TaskMcpConfig::default()
        }),
    }
    .into_request()
    .expect_err("AI execution service selection must be rejected");

    assert!(error.contains("cannot be selected by AI"));
}

#[test]
fn update_task_schema_hides_execution_status() {
    let schema = update_task_schema();
    let properties = schema
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("object properties");

    assert!(!properties.contains_key("status"));
}

#[test]
fn normalization_keeps_only_the_ai_explicit_builtin_selection() {
    let normalized = normalize_mcp_builtin_kind_names(vec!["CodeMaintainerWrite".to_string()])
        .expect("normalized kinds");
    assert_eq!(normalized, vec!["CodeMaintainerWrite".to_string()]);
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
fn create_task_args_preserve_external_mcp_ids_without_implicit_builtin_selection() {
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
        selected_skill_ids: Some(vec![
            " internal_skill_remotion ".to_string(),
            "internal_skill_remotion".to_string(),
        ]),
        prerequisite_task_ids: None,
        mcp_config: None,
    }
    .into_request()
    .expect("create task request");

    let mcp_config = request.mcp_config.expect("mcp config");
    assert!(mcp_config.enabled);
    assert!(mcp_config.enabled_builtin_kinds.is_empty());
    assert_eq!(
        mcp_config.external_mcp_config_ids,
        vec!["external-mcp-1".to_string()]
    );
    assert_eq!(
        mcp_config.selected_skill_ids,
        vec!["internal_skill_remotion".to_string()]
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
    assert!(chatos_async_planner::planner_agent_tool_allowed(
        "list_mcp_builtin_catalog"
    ));
    assert!(chatos_async_planner::planner_agent_tool_allowed(
        "list_external_mcp_configs"
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
async fn provider_descriptor_exposes_only_chatos_planner_tools() {
    let (service, _, _) = test_mcp_service().await;
    let descriptor = service.provider_descriptor();
    let tool_names = descriptor
        .tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(serde_json::Value::as_str))
        .collect::<Vec<_>>();

    assert_eq!(tool_names.len(), 10);
    for expected in [
        "list_tasks",
        "get_task",
        "create_task",
        "list_mcp_builtin_catalog",
        "list_external_mcp_configs",
        "list_available_skills",
        "create_tasks_with_prerequisites",
        "cancel_task",
        "wait_for_task_completion",
        "get_task_dependency_graph",
    ] {
        assert!(tool_names.contains(&expected), "missing {expected}");
    }
    for hidden in [
        "get_task_stats",
        "create_project_execution_tasks",
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

    let patch = UpdateTaskRequest {
        mcp_config: Some(TaskMcpConfig {
            enabled: false,
            enabled_builtin_kinds: vec![
                "CodeMaintainerRead".to_string(),
                "TaskManager".to_string(),
            ],
            external_mcp_config_ids: vec!["external-mcp-1".to_string()],
            ..TaskMcpConfig::default()
        }),
        ..UpdateTaskRequest::default()
    };
    let patch = chatos_async_planner::planner_update_task_request(patch).expect("planner patch");
    let config = patch.mcp_config.expect("mcp config");
    assert!(config.enabled);
    assert!(config
        .enabled_builtin_kinds
        .contains(&"CodeMaintainerRead".to_string()));
    assert!(!config
        .enabled_builtin_kinds
        .contains(&"TaskManager".to_string()));
    assert_eq!(
        config.external_mcp_config_ids,
        vec!["external-mcp-1".to_string()]
    );
}

#[test]
fn async_planner_tasks_keep_fixed_mcp_out_of_stored_selection() {
    let builtin_without_model_id = CreateTaskRequest {
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
        mcp_config: Some(TaskMcpConfig {
            enabled_builtin_kinds: vec!["CodeMaintainerRead".to_string()],
            ..TaskMcpConfig::default()
        }),
        prerequisite_task_ids: None,
    };
    assert!(
        chatos_async_planner::ensure_planner_required_fields(&builtin_without_model_id).is_ok()
    );
    let planned_builtin = chatos_async_planner::planner_root_create_request(
        builtin_without_model_id.clone(),
        &McpRequestContext::default(),
    )
    .expect("planner request");
    let planned_builtin_kinds = planned_builtin
        .mcp_config
        .expect("mcp config")
        .enabled_builtin_kinds;
    assert!(planned_builtin_kinds.contains(&"CodeMaintainerRead".to_string()));
    assert!(!planned_builtin_kinds.contains(&"TaskManager".to_string()));

    let external_without_model_id = CreateTaskRequest {
        mcp_config: Some(TaskMcpConfig {
            enabled_builtin_kinds: Vec::new(),
            external_mcp_config_ids: vec!["external-mcp-1".to_string()],
            ..TaskMcpConfig::default()
        }),
        ..builtin_without_model_id.clone()
    };
    assert!(
        chatos_async_planner::ensure_planner_required_fields(&external_without_model_id).is_ok()
    );
    let planned_external = chatos_async_planner::planner_root_create_request(
        external_without_model_id,
        &McpRequestContext::default(),
    )
    .expect("planner request");
    let planned_external_mcp = planned_external.mcp_config.expect("mcp config");
    assert_eq!(
        planned_external_mcp.external_mcp_config_ids,
        vec!["external-mcp-1".to_string()]
    );
    assert!(planned_external_mcp.enabled_builtin_kinds.is_empty());

    let no_explicit_tool_source = CreateTaskRequest {
        default_model_config_id: Some("model-1".to_string()),
        mcp_config: None,
        ..builtin_without_model_id.clone()
    };
    assert!(chatos_async_planner::ensure_planner_required_fields(&no_explicit_tool_source).is_ok());
    let planned_default = chatos_async_planner::planner_root_create_request(
        no_explicit_tool_source,
        &McpRequestContext::default(),
    )
    .expect("planner request");
    assert_eq!(
        planned_default
            .mcp_config
            .expect("mcp config")
            .enabled_builtin_kinds,
        Vec::<String>::new()
    );

    let combined = CreateTaskRequest {
        default_model_config_id: Some("model-1".to_string()),
        mcp_config: Some(TaskMcpConfig {
            enabled_builtin_kinds: vec!["CodeMaintainerWrite".to_string()],
            external_mcp_config_ids: vec!["external-mcp-2".to_string()],
            ..TaskMcpConfig::default()
        }),
        ..builtin_without_model_id
    };
    assert!(chatos_async_planner::ensure_planner_required_fields(&combined).is_ok());
    let planned_combined =
        chatos_async_planner::planner_root_create_request(combined, &McpRequestContext::default())
            .expect("planner request");
    let planned_combined_mcp = planned_combined.mcp_config.expect("mcp config");
    assert!(planned_combined_mcp
        .enabled_builtin_kinds
        .contains(&"CodeMaintainerWrite".to_string()));
    assert!(!planned_combined_mcp
        .enabled_builtin_kinds
        .contains(&"TaskManager".to_string()));
    assert_eq!(
        planned_combined_mcp.external_mcp_config_ids,
        vec!["external-mcp-2".to_string()]
    );
}

#[test]
fn async_planner_schema_hides_task_manager_from_builtin_selection() {
    let mut tools = vec![json!({
        "name": "create_task",
        "inputSchema": create_task_schema(),
    })];

    chatos_async_planner::enrich_tool_schemas_for_async_planner(&mut tools, &[]);

    let input_schema = tools[0].get("inputSchema").expect("input schema");
    assert!(input_schema.get("anyOf").is_none());
    let kind_enum = input_schema
        .pointer("/properties/enabled_builtin_kinds/items/enum")
        .and_then(|value| value.as_array())
        .expect("enabled_builtin_kinds enum");
    assert!(kind_enum
        .iter()
        .any(|value| value.as_str() == Some("CodeMaintainerRead")));
    assert!(!kind_enum
        .iter()
        .any(|value| value.as_str() == Some("TaskManager")));
}

#[test]
fn async_planner_batch_schema_hides_task_manager_from_builtin_selection() {
    let mut tools = vec![json!({
        "name": "create_tasks_with_prerequisites",
        "inputSchema": super::super::support::create_tasks_with_prerequisites_schema(),
    })];

    chatos_async_planner::enrich_tool_schemas_for_async_planner(&mut tools, &[]);

    let input_schema = tools[0].get("inputSchema").expect("input schema");
    assert!(input_schema
        .pointer("/properties/tasks/items/anyOf")
        .is_none());
    let kind_enum = input_schema
        .pointer("/properties/tasks/items/properties/enabled_builtin_kinds/items/enum")
        .and_then(|value| value.as_array())
        .expect("enabled_builtin_kinds enum");
    assert!(kind_enum
        .iter()
        .any(|value| value.as_str() == Some("TerminalController")));
    assert!(!kind_enum
        .iter()
        .any(|value| value.as_str() == Some("TaskManager")));
}

#[test]
fn async_planner_update_schema_hides_task_manager_from_builtin_selection() {
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
    let mcp_properties = input_schema
        .pointer("/properties/patch/properties/mcp_config/properties")
        .and_then(|value| value.as_object())
        .expect("mcp properties");
    assert!(!mcp_properties.contains_key("enabled"));
    assert!(!mcp_properties.contains_key("init_mode"));
    let kind_enum = input_schema
        .pointer(
            "/properties/patch/properties/mcp_config/properties/enabled_builtin_kinds/items/enum",
        )
        .and_then(|value| value.as_array())
        .expect("enabled_builtin_kinds enum");
    assert!(kind_enum
        .iter()
        .any(|value| value.as_str() == Some("BrowserTools")));
    assert!(!kind_enum
        .iter()
        .any(|value| value.as_str() == Some("TaskManager")));
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
        context
            .child_task_profile(Some(true), Some(false))
            .as_deref(),
        Some(TASK_PROFILE_CHATOS_PLAN)
    );
    assert_eq!(
        context
            .child_task_profile(Some(false), Some(true))
            .as_deref(),
        Some(TASK_PROFILE_DEFAULT)
    );
    assert_eq!(
        context.child_task_profile(None, Some(false)).as_deref(),
        Some(TASK_PROFILE_CHATOS_PLAN)
    );
    assert_eq!(
        context.child_task_profile(None, Some(true)).as_deref(),
        Some(TASK_PROFILE_CHATOS_PLAN)
    );
}
