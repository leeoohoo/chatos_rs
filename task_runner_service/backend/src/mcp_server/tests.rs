use super::chatos_async_planner;
use super::support::{agent_tool_allowed, create_task_schema, task_mcp_config_schema};
use super::{McpRequestContext, McpToolProfile};
use crate::models::{
    CreateTaskRequest, TaskMcpConfig, TaskScheduleMode, TaskStatus, UpdateTaskRequest,
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
    assert!(properties.contains_key("enabled_builtin_kinds"));

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
}

#[test]
fn external_mcp_tools_hide_internal_process_recorder() {
    assert!(!agent_tool_allowed("record_task_process"));
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
        "update_task"
    ));
    assert!(chatos_async_planner::planner_agent_tool_allowed(
        "set_task_prerequisites"
    ));
    assert!(chatos_async_planner::planner_agent_tool_allowed(
        "get_task_dependency_graph"
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
}

#[test]
fn async_planner_tasks_require_model_and_builtin_kinds() {
    let missing_model = CreateTaskRequest {
        title: "task".to_string(),
        description: None,
        objective: "objective".to_string(),
        input_payload: None,
        status: None,
        priority: None,
        tags: None,
        default_model_config_id: None,
        tenant_id: None,
        subject_id: None,
        schedule: None,
        mcp_config: Some(TaskMcpConfig {
            enabled_builtin_kinds: vec!["CodeMaintainerRead".to_string()],
            ..TaskMcpConfig::default()
        }),
        prerequisite_task_ids: None,
    };
    assert!(chatos_async_planner::ensure_planner_required_fields(&missing_model).is_err());

    let missing_builtin_kinds = CreateTaskRequest {
        default_model_config_id: Some("model-1".to_string()),
        mcp_config: Some(TaskMcpConfig {
            enabled_builtin_kinds: Vec::new(),
            ..TaskMcpConfig::default()
        }),
        ..missing_model.clone()
    };
    assert!(chatos_async_planner::ensure_planner_required_fields(&missing_builtin_kinds).is_err());

    let valid = CreateTaskRequest {
        default_model_config_id: Some("model-1".to_string()),
        mcp_config: Some(TaskMcpConfig {
            enabled_builtin_kinds: vec!["CodeMaintainerWrite".to_string()],
            ..TaskMcpConfig::default()
        }),
        ..missing_model
    };
    assert!(chatos_async_planner::ensure_planner_required_fields(&valid).is_ok());
}

#[test]
fn async_planner_root_tasks_are_forced_to_contact_async_schedule() {
    let request = valid_planner_create_request();
    let planned =
        chatos_async_planner::planner_root_create_request(request).expect("planner request");
    assert_eq!(
        planned.schedule.expect("schedule").mode,
        TaskScheduleMode::ContactAsync
    );
}

#[test]
fn async_planner_prerequisite_tasks_are_forced_to_contact_async_schedule() {
    let request = valid_planner_create_request();
    let planned = chatos_async_planner::planner_prerequisite_create_request(request)
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
