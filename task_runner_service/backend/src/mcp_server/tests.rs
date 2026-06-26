use super::chatos_async_planner;
use super::support::{
    agent_tool_allowed, create_task_schema, enrich_tool_schemas_with_model_configs,
    filter_model_configs_for_user, model_configs_for_user, normalize_mcp_builtin_kind_names,
    task_mcp_config_schema,
};
use super::{CreateTaskArgs, McpRequestContext, McpToolProfile, TaskRunnerMcpService};
use crate::ask_user_prompt_service::AskUserPromptService;
use crate::auth::CurrentUser;
use crate::config::{AppConfig, StoreMode};
use crate::models::{
    ChatosSyncedModelConfigRequest, CreateTaskProjectRequest, CreateTaskRequest, ModelConfigRecord,
    TaskMcpConfig, TaskScheduleMode, TaskSourceContext, TaskStatus, UpdateTaskRequest, UserRole,
    PUBLIC_PROJECT_ID, TASK_PROFILE_CHATOS_PLAN, TASK_PROFILE_DEFAULT,
};
use crate::services::{
    ExternalMcpConfigService, McpCatalogService, ModelConfigService, RunService,
    TaskProjectService, TaskService,
};
use crate::store::AppStore;
use serde_json::json;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

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
    assert!(!properties.contains_key("default_model_config_id"));
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
fn task_mcp_config_schema_hides_host_passthrough_fields() {
    let schema = task_mcp_config_schema();
    let properties = schema
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("object properties");

    assert!(!properties.contains_key("workspace_dir"));
    assert!(!properties.contains_key("default_remote_server_id"));
    assert!(properties.contains_key("enabled_builtin_kinds"));
    assert!(properties.contains_key("external_mcp_config_ids"));
}

#[test]
fn normalizes_code_maintainer_write_with_required_read_kind() {
    let normalized = normalize_mcp_builtin_kind_names(vec!["CodeMaintainerWrite".to_string()])
        .expect("normalized kinds");
    assert_eq!(
        normalized,
        vec![
            "CodeMaintainerRead".to_string(),
            "CodeMaintainerWrite".to_string(),
        ]
    );
}

#[test]
fn external_mcp_tools_hide_internal_process_recorder() {
    assert!(!agent_tool_allowed("record_task_process"));
    assert!(!agent_tool_allowed("list_model_configs"));
    assert!(!agent_tool_allowed("get_model_config"));
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
        schedule: None,
        enabled_builtin_kinds: None,
        external_mcp_config_ids: Some(vec![
            " external-mcp-1 ".to_string(),
            String::new(),
            "external-mcp-1".to_string(),
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
fn mcp_tool_schema_does_not_expose_model_config_ids() {
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
    let properties = tools[0]
        .pointer("/inputSchema/properties")
        .and_then(|value| value.as_object())
        .expect("properties");

    assert!(!properties.contains_key("default_model_config_id"));
}

#[test]
fn async_planner_profile_exposes_only_planning_tools() {
    assert!(chatos_async_planner::planner_agent_tool_allowed(
        "list_tasks"
    ));
    assert!(chatos_async_planner::planner_agent_tool_allowed("get_task"));
    assert!(chatos_async_planner::planner_agent_tool_allowed(
        "get_task_stats"
    ));
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
        "update_task"
    ));
    assert!(chatos_async_planner::planner_agent_tool_allowed(
        "set_task_prerequisites"
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
            enabled_builtin_kinds: vec!["CodeMaintainerRead".to_string()],
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
    assert!(config
        .enabled_builtin_kinds
        .contains(&"TaskManager".to_string()));
    assert_eq!(
        config.external_mcp_config_ids,
        vec!["external-mcp-1".to_string()]
    );
}

#[test]
fn async_planner_tasks_allow_free_mcp_combinations_and_auto_task_manager() {
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
    assert!(planned_builtin_kinds.contains(&"TaskManager".to_string()));

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
    assert_eq!(
        planned_external_mcp.enabled_builtin_kinds,
        vec!["TaskManager".to_string(), "AskUser".to_string()]
    );

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
        vec!["TaskManager".to_string(), "AskUser".to_string()]
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
    assert!(planned_combined_mcp
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
        "inputSchema": super::support::create_tasks_with_prerequisites_schema(),
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
                "patch": super::support::update_task_schema()
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

#[tokio::test]
async fn chatos_plan_profile_requires_concrete_project_scope() {
    let (mcp_service, _, _) = test_mcp_service().await;
    let current_user = agent_user("owner-a");

    let response = mcp_service
        .handle_jsonrpc(
            super::JsonRpcRequest {
                jsonrpc: Some("2.0".to_string()),
                id: Some(json!("req-1")),
                method: "tools/list".to_string(),
                params: json!({}),
            },
            current_user,
            McpRequestContext {
                task_profile: Some(TASK_PROFILE_CHATOS_PLAN.to_string()),
                ..McpRequestContext::default()
            },
        )
        .await;

    assert_eq!(
        response.error.as_ref().map(|error| error.message.as_str()),
        Some("Chatos Plan mode requires concrete project_id")
    );
}

#[tokio::test]
async fn list_tasks_uses_passthrough_project_context_filter() {
    let (mcp_service, task_service, project_service) = test_mcp_service().await;
    let current_user = agent_user("owner-a");
    let project = project_service
        .create_project(
            CreateTaskProjectRequest {
                name: "Project A".to_string(),
                root_path: None,
                git_url: None,
                description: None,
            },
            &current_user,
        )
        .await
        .expect("create project");
    let project_task = task_service
        .create_task(
            test_create_task_request("project task"),
            Some(&current_user),
            Some(TaskSourceContext {
                project_id: Some(project.id.clone()),
                ..TaskSourceContext::default()
            }),
        )
        .await
        .expect("create project task");
    let public_task = task_service
        .create_task(
            test_create_task_request("public task"),
            Some(&current_user),
            None,
        )
        .await
        .expect("create public task");

    let project_result = mcp_service
        .call_tool(
            "list_tasks",
            json!({}),
            &current_user,
            &McpRequestContext {
                project_id: Some(project.id.clone()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect("list project tasks");
    let project_task_ids = structured_task_ids(&project_result);
    assert_eq!(project_task_ids, vec![project_task.id.clone()]);

    let public_result = mcp_service
        .call_tool(
            "list_tasks",
            json!({}),
            &current_user,
            &McpRequestContext {
                project_id: Some(PUBLIC_PROJECT_ID.to_string()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect("list public tasks");
    let public_task_ids = structured_task_ids(&public_result);
    assert_eq!(public_task_ids, vec![public_task.id]);
}

#[tokio::test]
async fn list_tasks_in_chatos_plan_profile_only_returns_plan_tasks() {
    let (mcp_service, task_service, project_service) = test_mcp_service().await;
    let current_user = agent_user("owner-a");
    let project = project_service
        .create_project(
            CreateTaskProjectRequest {
                name: "Project A".to_string(),
                root_path: None,
                git_url: None,
                description: None,
            },
            &current_user,
        )
        .await
        .expect("create project");
    let default_task = task_service
        .create_task(
            CreateTaskRequest {
                project_id: Some(project.id.clone()),
                task_profile: Some(TASK_PROFILE_DEFAULT.to_string()),
                ..test_create_task_request("default task")
            },
            Some(&current_user),
            None,
        )
        .await
        .expect("create default task");
    let plan_task = task_service
        .create_task(
            CreateTaskRequest {
                project_id: Some(project.id.clone()),
                task_profile: Some(TASK_PROFILE_CHATOS_PLAN.to_string()),
                ..test_create_task_request("plan task")
            },
            Some(&current_user),
            None,
        )
        .await
        .expect("create plan task");

    let result = mcp_service
        .call_tool(
            "list_tasks",
            json!({}),
            &current_user,
            &McpRequestContext {
                project_id: Some(project.id.clone()),
                task_profile: Some(TASK_PROFILE_CHATOS_PLAN.to_string()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect("list plan tasks");
    let task_ids = structured_task_ids(&result);

    assert_eq!(task_ids, vec![plan_task.id]);
    assert_ne!(task_ids, vec![default_task.id]);
}

#[tokio::test]
async fn get_task_in_chatos_plan_profile_rejects_default_task_id() {
    let (mcp_service, task_service, project_service) = test_mcp_service().await;
    let current_user = agent_user("owner-a");
    let project = project_service
        .create_project(
            CreateTaskProjectRequest {
                name: "Project A".to_string(),
                root_path: None,
                git_url: None,
                description: None,
            },
            &current_user,
        )
        .await
        .expect("create project");
    let default_task = task_service
        .create_task(
            CreateTaskRequest {
                project_id: Some(project.id.clone()),
                task_profile: Some(TASK_PROFILE_DEFAULT.to_string()),
                ..test_create_task_request("default task")
            },
            Some(&current_user),
            None,
        )
        .await
        .expect("create default task");

    let err = mcp_service
        .call_tool(
            "get_task",
            json!({
                "task_id": default_task.id,
            }),
            &current_user,
            &McpRequestContext {
                project_id: Some(project.id.clone()),
                task_profile: Some(TASK_PROFILE_CHATOS_PLAN.to_string()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect_err("plan profile should reject default task");

    assert_eq!(err, "当前 agent 无权访问该任务");
}

#[tokio::test]
async fn get_task_dependency_graph_in_chatos_plan_profile_rejects_default_task_id() {
    let (mcp_service, task_service, project_service) = test_mcp_service().await;
    let current_user = agent_user("owner-a");
    let project = project_service
        .create_project(
            CreateTaskProjectRequest {
                name: "Project A".to_string(),
                root_path: None,
                git_url: None,
                description: None,
            },
            &current_user,
        )
        .await
        .expect("create project");
    let default_task = task_service
        .create_task(
            CreateTaskRequest {
                project_id: Some(project.id.clone()),
                task_profile: Some(TASK_PROFILE_DEFAULT.to_string()),
                ..test_create_task_request("default task")
            },
            Some(&current_user),
            None,
        )
        .await
        .expect("create default task");

    let err = mcp_service
        .call_tool(
            "get_task_dependency_graph",
            json!({
                "task_id": default_task.id,
            }),
            &current_user,
            &McpRequestContext {
                project_id: Some(project.id.clone()),
                task_profile: Some(TASK_PROFILE_CHATOS_PLAN.to_string()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect_err("plan profile should reject default task graph");

    assert_eq!(err, "当前 agent 无权访问该任务");
}

#[tokio::test]
async fn set_task_prerequisites_in_chatos_plan_profile_rejects_default_task_id() {
    let (mcp_service, task_service, project_service) = test_mcp_service().await;
    let current_user = agent_user("owner-a");
    let project = project_service
        .create_project(
            CreateTaskProjectRequest {
                name: "Project A".to_string(),
                root_path: None,
                git_url: None,
                description: None,
            },
            &current_user,
        )
        .await
        .expect("create project");
    let default_task = task_service
        .create_task(
            CreateTaskRequest {
                project_id: Some(project.id.clone()),
                task_profile: Some(TASK_PROFILE_DEFAULT.to_string()),
                ..test_create_task_request("default task")
            },
            Some(&current_user),
            None,
        )
        .await
        .expect("create default task");

    let err = mcp_service
        .call_tool(
            "set_task_prerequisites",
            json!({
                "task_id": default_task.id,
                "prerequisite_task_ids": [],
            }),
            &current_user,
            &McpRequestContext {
                project_id: Some(project.id.clone()),
                task_profile: Some(TASK_PROFILE_CHATOS_PLAN.to_string()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect_err("plan profile should reject default task prerequisite updates");

    assert_eq!(err, "当前 agent 无权访问该任务");
}

#[tokio::test]
async fn cancel_task_in_chatos_plan_profile_rejects_default_task_id() {
    let (mcp_service, task_service, project_service) = test_mcp_service().await;
    let current_user = agent_user("owner-a");
    let project = project_service
        .create_project(
            CreateTaskProjectRequest {
                name: "Project A".to_string(),
                root_path: None,
                git_url: None,
                description: None,
            },
            &current_user,
        )
        .await
        .expect("create project");
    let default_task = task_service
        .create_task(
            CreateTaskRequest {
                project_id: Some(project.id.clone()),
                task_profile: Some(TASK_PROFILE_DEFAULT.to_string()),
                ..test_create_task_request("default task")
            },
            Some(&current_user),
            None,
        )
        .await
        .expect("create default task");

    let err = mcp_service
        .call_tool(
            "cancel_task",
            json!({
                "task_id": default_task.id,
                "reason": "no longer needed",
            }),
            &current_user,
            &McpRequestContext {
                project_id: Some(project.id.clone()),
                task_profile: Some(TASK_PROFILE_CHATOS_PLAN.to_string()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect_err("plan profile should reject default task cancellation");

    assert_eq!(err, "当前 agent 无权访问该任务");
}

#[tokio::test]
async fn create_task_in_chatos_plan_profile_persists_plan_task_profile() {
    let (mcp_service, task_service, project_service) = test_mcp_service().await;
    let current_user = agent_user("owner-a");
    let _model = mcp_service
        .model_config_service
        .upsert_chatos_model_config(ChatosSyncedModelConfigRequest {
            id: "model-1".to_string(),
            owner_user_id: Some("owner-a".to_string()),
            name: "Task Model".to_string(),
            provider: "openai".to_string(),
            base_url: "https://api.example.test/v1".to_string(),
            api_key: "test-key".to_string(),
            model: "gpt-test".to_string(),
            usage_scenario: Some("task planning".to_string()),
            thinking_level: None,
            enabled: Some(true),
            supports_responses: Some(true),
        })
        .await
        .expect("create model config");
    let project = project_service
        .create_project(
            CreateTaskProjectRequest {
                name: "Project A".to_string(),
                root_path: None,
                git_url: None,
                description: None,
            },
            &current_user,
        )
        .await
        .expect("create project");

    let result = mcp_service
        .call_tool(
            "create_task",
            json!({
                "title": "plan task",
                "objective": "define implementation plan",
                "default_model_config_id": "model-1",
            }),
            &current_user,
            &McpRequestContext {
                project_id: Some(project.id.clone()),
                source_session_id: Some("session-1".to_string()),
                source_user_message_id: Some("message-1".to_string()),
                task_profile: Some(TASK_PROFILE_CHATOS_PLAN.to_string()),
                builtin_prompt_locale: Some("en-US".to_string()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect("create plan task");

    let task_id = result
        .get("_structured_result")
        .and_then(|value| value.get("id"))
        .and_then(|value| value.as_str())
        .expect("task id");
    let task = task_service
        .get_task(task_id)
        .await
        .expect("get task")
        .expect("task");

    assert_eq!(task.task_profile, TASK_PROFILE_CHATOS_PLAN);
    assert_ne!(task.status, TaskStatus::Ready);
    assert_eq!(task.schedule.mode, TaskScheduleMode::ContactAsync);
    assert!(task.schedule.next_run_at.is_none());
    assert!(task.schedule.last_scheduled_at.is_some());
    assert!(task.mcp_config.enabled);
    assert_eq!(task.mcp_config.builtin_prompt_locale, "en-US");
    let runs = mcp_service
        .run_service
        .list_runs(Some(task_id))
        .await
        .expect("list runs");
    assert_eq!(runs.len(), 1);
}

#[tokio::test]
async fn create_tasks_with_prerequisites_in_chatos_plan_profile_persist_plan_task_profile() {
    let (mcp_service, task_service, project_service) = test_mcp_service().await;
    let current_user = agent_user("owner-a");
    let _model = mcp_service
        .model_config_service
        .upsert_chatos_model_config(ChatosSyncedModelConfigRequest {
            id: "model-1".to_string(),
            owner_user_id: Some("owner-a".to_string()),
            name: "Task Model".to_string(),
            provider: "openai".to_string(),
            base_url: "https://api.example.test/v1".to_string(),
            api_key: "test-key".to_string(),
            model: "gpt-test".to_string(),
            usage_scenario: Some("task planning".to_string()),
            thinking_level: None,
            enabled: Some(true),
            supports_responses: Some(true),
        })
        .await
        .expect("create model config");
    let project = project_service
        .create_project(
            CreateTaskProjectRequest {
                name: "Project A".to_string(),
                root_path: None,
                git_url: None,
                description: None,
            },
            &current_user,
        )
        .await
        .expect("create project");

    let result = mcp_service
        .call_tool(
            "create_tasks_with_prerequisites",
            json!({
                "tasks": [
                    {
                        "client_ref": "root",
                        "title": "root task",
                        "objective": "define implementation plan",
                        "default_model_config_id": "model-1"
                    },
                    {
                        "client_ref": "child",
                        "title": "child task",
                        "objective": "detail follow-up",
                        "default_model_config_id": "model-1",
                        "prerequisite_refs": ["root"]
                    }
                ]
            }),
            &current_user,
            &McpRequestContext {
                project_id: Some(project.id.clone()),
                source_session_id: Some("session-1".to_string()),
                source_user_message_id: Some("message-1".to_string()),
                task_profile: Some(TASK_PROFILE_CHATOS_PLAN.to_string()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect("create plan task graph");

    let created_tasks = result
        .get("_structured_result")
        .and_then(|value| value.get("created_tasks"))
        .and_then(|value| value.as_array())
        .expect("created tasks");
    let task_ids = created_tasks
        .iter()
        .map(|task| {
            task.get("task_id")
                .and_then(|value| value.as_str())
                .expect("task id")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(task_ids.len(), 2);
    let child_task_id = created_tasks
        .iter()
        .find(|task| task.get("client_ref").and_then(|value| value.as_str()) == Some("child"))
        .and_then(|task| task.get("task_id"))
        .and_then(|value| value.as_str())
        .expect("child task id");
    let auto_started_runs = result
        .get("_structured_result")
        .and_then(|value| value.get("auto_started_runs"))
        .and_then(|value| value.as_array())
        .expect("auto started runs");
    assert_eq!(auto_started_runs.len(), 1);
    assert_eq!(
        auto_started_runs[0]
            .get("task_id")
            .and_then(|value| value.as_str()),
        Some(child_task_id)
    );

    for task_id in task_ids {
        let task = task_service
            .get_task(task_id.as_str())
            .await
            .expect("get task")
            .expect("task");
        assert_eq!(task.task_profile, TASK_PROFILE_CHATOS_PLAN);
        assert_eq!(task.project_id, project.id);
        assert!(task.schedule.next_run_at.is_none());
    }
}

#[tokio::test]
async fn chatos_async_reuse_is_scoped_by_task_profile() {
    let (mcp_service, task_service, project_service) = test_mcp_service().await;
    let current_user = agent_user("owner-a");
    let _model = mcp_service
        .model_config_service
        .upsert_chatos_model_config(ChatosSyncedModelConfigRequest {
            id: "model-1".to_string(),
            owner_user_id: Some("owner-a".to_string()),
            name: "Task Model".to_string(),
            provider: "openai".to_string(),
            base_url: "https://api.example.test/v1".to_string(),
            api_key: "test-key".to_string(),
            model: "gpt-test".to_string(),
            usage_scenario: Some("task planning".to_string()),
            thinking_level: None,
            enabled: Some(true),
            supports_responses: Some(true),
        })
        .await
        .expect("create model config");
    let project = project_service
        .create_project(
            CreateTaskProjectRequest {
                name: "Project A".to_string(),
                root_path: None,
                git_url: None,
                description: None,
            },
            &current_user,
        )
        .await
        .expect("create project");
    let source_context = TaskSourceContext {
        project_id: Some(project.id.clone()),
        source_session_id: Some("session-1".to_string()),
        source_user_message_id: Some("message-1".to_string()),
        ..TaskSourceContext::default()
    };
    let default_task = task_service
        .create_task(
            CreateTaskRequest {
                project_id: Some(project.id.clone()),
                task_profile: Some(TASK_PROFILE_DEFAULT.to_string()),
                default_model_config_id: Some("model-1".to_string()),
                ..test_create_task_request("default task")
            },
            Some(&current_user),
            Some(source_context),
        )
        .await
        .expect("create default task");

    let plan_result = mcp_service
        .call_tool(
            "create_task",
            json!({
                "title": "plan task",
                "objective": "define implementation plan",
                "default_model_config_id": "model-1",
            }),
            &current_user,
            &McpRequestContext {
                project_id: Some(project.id.clone()),
                source_session_id: Some("session-1".to_string()),
                source_user_message_id: Some("message-1".to_string()),
                tool_profile: Some("chatos_async_planner".to_string()),
                task_profile: Some(TASK_PROFILE_CHATOS_PLAN.to_string()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect("create plan task");

    let created_plan_task_id = plan_result
        .get("_structured_result")
        .and_then(|value| value.get("id"))
        .and_then(|value| value.as_str())
        .expect("plan task id")
        .to_string();
    assert_ne!(created_plan_task_id, default_task.id);

    let reused_plan_result = mcp_service
        .call_tool(
            "create_task",
            json!({
                "title": "plan task",
                "objective": "define implementation plan",
                "default_model_config_id": "model-1",
            }),
            &current_user,
            &McpRequestContext {
                project_id: Some(project.id.clone()),
                source_session_id: Some("session-1".to_string()),
                source_user_message_id: Some("message-1".to_string()),
                tool_profile: Some("chatos_async_planner".to_string()),
                task_profile: Some(TASK_PROFILE_CHATOS_PLAN.to_string()),
                ..McpRequestContext::default()
            },
        )
        .await
        .expect("reuse plan task");
    let reused_plan_task_id = reused_plan_result
        .get("_structured_result")
        .and_then(|value| value.get("id"))
        .and_then(|value| value.as_str())
        .expect("reused plan task id");

    assert_eq!(reused_plan_task_id, created_plan_task_id);
}

fn valid_planner_create_request() -> CreateTaskRequest {
    CreateTaskRequest {
        title: "task".to_string(),
        description: None,
        objective: "objective".to_string(),
        input_payload: None,
        status: None,
        priority: None,
        tags: None,
        default_model_config_id: Some("model-1".to_string()),
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
    }
}

async fn test_mcp_service() -> (TaskRunnerMcpService, TaskService, TaskProjectService) {
    let config = test_config();
    let store = AppStore::new(&config).await.expect("store");
    let task_service = TaskService::new(config.clone(), store.clone());
    let model_config_service = ModelConfigService::new(store.clone());
    let external_mcp_config_service = ExternalMcpConfigService::new(store.clone());
    let ask_user_prompt_service = AskUserPromptService::new(store.clone());
    let run_service = RunService::new(config, store.clone(), ask_user_prompt_service.clone());
    let mcp_catalog_service =
        McpCatalogService::new(task_service.clone(), ask_user_prompt_service.clone());
    let task_project_service = TaskProjectService::new(store);
    (
        TaskRunnerMcpService::new(
            task_service.clone(),
            model_config_service,
            external_mcp_config_service,
            run_service,
            ask_user_prompt_service,
            mcp_catalog_service,
        ),
        task_service,
        task_project_service,
    )
}

fn test_config() -> AppConfig {
    AppConfig {
        host: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: 0,
        store_mode: StoreMode::Memory,
        database_url: "memory://mcp-project-scope-test".to_string(),
        memory_engine_base_url: None,
        memory_engine_source_id: "task".to_string(),
        memory_engine_operator_token: None,
        default_tenant_id: "tenant".to_string(),
        default_subject_id: "subject".to_string(),
        default_workspace_dir: ".".to_string(),
        memory_timeout: Duration::from_millis(1000),
        execution_timeout: Duration::from_millis(1000),
        scheduler_poll_interval: Duration::from_millis(1000),
        auto_memory_summary: false,
        default_task_execution_max_iterations: 1,
        default_tool_result_model_max_chars: 1000,
        default_tool_results_model_total_max_chars: 2000,
        chatos_callback_url: None,
        chatos_callback_secret: None,
        internal_api_secret: None,
        callback_timeout: Duration::from_millis(1000),
        admin_username: "admin".to_string(),
        admin_password: "admin".to_string(),
        admin_display_name: "Admin".to_string(),
        user_service_base_url: "http://127.0.0.1:39190".to_string(),
        user_service_request_timeout: Duration::from_millis(5000),
        project_service_base_url: None,
        project_service_sync_secret: None,
        project_service_request_timeout: Duration::from_millis(5000),
    }
}

fn test_create_task_request(title: &str) -> CreateTaskRequest {
    CreateTaskRequest {
        title: title.to_string(),
        description: None,
        objective: format!("do {title}"),
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
        mcp_config: None,
        prerequisite_task_ids: None,
    }
}

fn structured_task_ids(value: &serde_json::Value) -> Vec<String> {
    value
        .get("_structured_result")
        .and_then(|value| value.as_array())
        .expect("structured task array")
        .iter()
        .map(|task| {
            task.get("id")
                .and_then(|value| value.as_str())
                .expect("task id")
                .to_string()
        })
        .collect()
}

fn admin_user(owner_user_id: &str) -> CurrentUser {
    CurrentUser {
        id: owner_user_id.to_string(),
        username: format!("{owner_user_id}-name"),
        display_name: format!("{owner_user_id} name"),
        role: UserRole::Admin,
        owner_user_id: Some(owner_user_id.to_string()),
        owner_username: Some(format!("{owner_user_id}-name")),
        owner_display_name: Some(format!("{owner_user_id} name")),
    }
}

fn agent_user(owner_user_id: &str) -> CurrentUser {
    CurrentUser {
        id: format!("agent-{owner_user_id}"),
        username: format!("agent-{owner_user_id}"),
        display_name: format!("Agent {owner_user_id}"),
        role: UserRole::Agent,
        owner_user_id: Some(owner_user_id.to_string()),
        owner_username: Some(format!("{owner_user_id}-name")),
        owner_display_name: Some(format!("{owner_user_id} name")),
    }
}

fn model_config(id: &str, owner_user_id: &str, enabled: bool) -> ModelConfigRecord {
    ModelConfigRecord {
        id: id.to_string(),
        owner_user_id: Some(owner_user_id.to_string()),
        owner_username: Some(format!("{owner_user_id}-name")),
        owner_display_name: Some(format!("{owner_user_id} name")),
        name: id.to_string(),
        provider: "openai".to_string(),
        base_url: "https://api.example.test/v1".to_string(),
        api_key: format!("{id}-key"),
        model: format!("{id}-model"),
        usage_scenario: Some(format!("{id} usage")),
        temperature: None,
        max_output_tokens: None,
        thinking_level: None,
        supports_responses: true,
        instructions: None,
        request_cwd: None,
        include_prompt_cache_retention: false,
        request_body_limit_bytes: None,
        enabled,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    }
}
