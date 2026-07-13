// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::{
    now_rfc3339, TaskEphemeralHttpMcpServer, TaskMcpConfig, TaskRecord, TaskScheduleConfig,
    TaskStatus, TASK_MCP_HTTP_AUTH_LOCAL_CONNECTOR_INTERNAL,
    TASK_MCP_HTTP_AUTH_PROJECT_SERVICE_SYNC, TASK_PROFILE_CHATOS_PLAN, TASK_PROFILE_DEFAULT,
};

use super::{
    ensure_workspace_is_inside_base, runtime_selected_builtin_kinds, selected_builtin_kinds,
    LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER,
};
use chatos_mcp_runtime::BuiltinMcpKind;

#[test]
fn empty_builtin_selection_stays_empty() {
    let config = TaskMcpConfig {
        enabled_builtin_kinds: Vec::new(),
        ..TaskMcpConfig::default()
    };

    assert!(selected_builtin_kinds(&config).is_empty());
}

#[test]
fn default_config_has_no_optional_builtin_selection() {
    let config = TaskMcpConfig::default();

    assert!(selected_builtin_kinds(&config).is_empty());
}

#[test]
fn plan_task_builtin_selection_uses_fixed_allowlist() {
    let task = sample_task(
        TASK_PROFILE_CHATOS_PLAN,
        vec![
            "CodeMaintainerWrite".to_string(),
            "AgentBuilder".to_string(),
        ],
    );

    let selected = runtime_selected_builtin_kinds(&task);

    assert!(selected.contains(&BuiltinMcpKind::CodeMaintainerRead));
    assert!(selected.contains(&BuiltinMcpKind::TaskManager));
    assert!(selected.contains(&BuiltinMcpKind::ProjectManagement));
    assert!(selected.contains(&BuiltinMcpKind::BrowserTools));
    assert!(!selected.contains(&BuiltinMcpKind::CodeMaintainerWrite));
    assert!(!selected.contains(&BuiltinMcpKind::AgentBuilder));
}

#[test]
fn default_task_builtin_selection_keeps_requested_kinds() {
    let task = sample_task(
        TASK_PROFILE_DEFAULT,
        vec!["CodeMaintainerWrite".to_string()],
    );

    let selected = runtime_selected_builtin_kinds(&task);

    assert!(selected.contains(&BuiltinMcpKind::CodeMaintainerWrite));
    assert!(selected.contains(&BuiltinMcpKind::CodeMaintainerRead));
}

#[test]
fn contact_async_task_adds_required_task_manager_and_ask_user_at_runtime() {
    let mut task = sample_task(TASK_PROFILE_DEFAULT, Vec::new());
    task.schedule.mode = crate::models::TaskScheduleMode::ContactAsync;

    let selected = runtime_selected_builtin_kinds(&task);

    assert!(task.mcp_config.enabled_builtin_kinds.is_empty());
    assert!(selected.contains(&BuiltinMcpKind::TaskManager));
    assert!(selected.contains(&BuiltinMcpKind::AskUser));
}

#[test]
fn local_connector_task_removes_server_local_builtin_kinds() {
    let mut task = sample_task(
        TASK_PROFILE_DEFAULT,
        vec![
            "CodeMaintainerWrite".to_string(),
            "TerminalController".to_string(),
            "BrowserTools".to_string(),
            "WebTools".to_string(),
        ],
    );
    task.mcp_config
        .ephemeral_http_servers
        .push(local_connector_server());

    let selected = runtime_selected_builtin_kinds(&task);

    assert!(!selected.contains(&BuiltinMcpKind::CodeMaintainerRead));
    assert!(!selected.contains(&BuiltinMcpKind::CodeMaintainerWrite));
    assert!(!selected.contains(&BuiltinMcpKind::TerminalController));
    assert!(!selected.contains(&BuiltinMcpKind::BrowserTools));
    assert!(selected.contains(&BuiltinMcpKind::WebTools));
}

#[test]
fn local_connector_plan_task_removes_fixed_server_local_builtin_kinds() {
    let mut task = sample_task(TASK_PROFILE_CHATOS_PLAN, Vec::new());
    task.mcp_config
        .ephemeral_http_servers
        .push(local_connector_server());

    let selected = runtime_selected_builtin_kinds(&task);

    assert!(!selected.contains(&BuiltinMcpKind::CodeMaintainerRead));
    assert!(!selected.contains(&BuiltinMcpKind::CodeMaintainerWrite));
    assert!(!selected.contains(&BuiltinMcpKind::TerminalController));
    assert!(!selected.contains(&BuiltinMcpKind::BrowserTools));
    assert!(selected.contains(&BuiltinMcpKind::TaskManager));
    assert!(selected.contains(&BuiltinMcpKind::ProjectManagement));
}

#[test]
fn harness_code_task_removes_server_local_code_builtin_kinds() {
    let mut task = sample_task(
        TASK_PROFILE_DEFAULT,
        vec![
            "CodeMaintainerWrite".to_string(),
            "TerminalController".to_string(),
            "WebTools".to_string(),
        ],
    );
    task.mcp_config
        .ephemeral_http_servers
        .push(harness_code_server());

    let selected = runtime_selected_builtin_kinds(&task);

    assert!(!selected.contains(&BuiltinMcpKind::CodeMaintainerRead));
    assert!(!selected.contains(&BuiltinMcpKind::CodeMaintainerWrite));
    assert!(selected.contains(&BuiltinMcpKind::TerminalController));
    assert!(selected.contains(&BuiltinMcpKind::WebTools));
}

#[test]
fn harness_code_plan_task_removes_fixed_server_local_code_builtin_kinds() {
    let mut task = sample_task(TASK_PROFILE_CHATOS_PLAN, Vec::new());
    task.mcp_config
        .ephemeral_http_servers
        .push(harness_code_server());

    let selected = runtime_selected_builtin_kinds(&task);

    assert!(!selected.contains(&BuiltinMcpKind::CodeMaintainerRead));
    assert!(!selected.contains(&BuiltinMcpKind::CodeMaintainerWrite));
    assert!(selected.contains(&BuiltinMcpKind::TerminalController));
    assert!(selected.contains(&BuiltinMcpKind::BrowserTools));
    assert!(selected.contains(&BuiltinMcpKind::TaskManager));
    assert!(selected.contains(&BuiltinMcpKind::ProjectManagement));
}

#[test]
fn local_connector_runtime_routing_keeps_requested_config_and_payload() {
    let mut task = sample_task(
        TASK_PROFILE_DEFAULT,
        vec![
            "CodeMaintainerRead".to_string(),
            "TerminalController".to_string(),
            "BrowserTools".to_string(),
            "TaskManager".to_string(),
        ],
    );
    task.input_payload = Some(serde_json::json!({ "source": "test" }));

    let changed = super::apply_local_connector_runtime_routing_to_task(
        &mut task,
        "local://connector/device-1/workspace-1/apps/web",
        false,
    );

    assert!(changed);
    assert_eq!(
        task.input_payload,
        Some(serde_json::json!({ "source": "test" }))
    );
    assert_eq!(
        task.mcp_config.enabled_builtin_kinds,
        vec![
            "CodeMaintainerRead".to_string(),
            "TerminalController".to_string(),
            "BrowserTools".to_string(),
            "TaskManager".to_string(),
        ]
    );
    let server = task
        .mcp_config
        .ephemeral_http_servers
        .first()
        .expect("local connector server");
    assert_eq!(server.name, "local_connector");
    assert_eq!(
        server.auth_mode.as_deref(),
        Some(TASK_MCP_HTTP_AUTH_LOCAL_CONNECTOR_INTERNAL)
    );
    assert_eq!(
        server
            .headers
            .get(LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER)
            .map(String::as_str),
        Some("CodeMaintainerRead,TerminalController,BrowserTools")
    );
    assert!(server
        .url
        .contains("/api/local-connectors/relay/device-1/mcp"));
    assert!(server.url.contains("workspace_id=workspace-1"));
    assert!(server.url.contains("cwd=apps%2Fweb"));
}

#[test]
fn local_connector_routing_passes_only_selected_local_capabilities() {
    let mut task = sample_task(
        TASK_PROFILE_DEFAULT,
        vec!["BrowserTools".to_string(), "TaskManager".to_string()],
    );

    let changed = super::apply_local_connector_runtime_routing_to_task(
        &mut task,
        "local://connector/device-1/workspace-1/apps/web",
        false,
    );

    assert!(changed);
    assert_eq!(
        task.mcp_config.enabled_builtin_kinds,
        vec!["BrowserTools".to_string(), "TaskManager".to_string()]
    );
    let server = task
        .mcp_config
        .ephemeral_http_servers
        .first()
        .expect("local connector server");
    assert_eq!(
        server
            .headers
            .get(LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER)
            .map(String::as_str),
        Some("BrowserTools")
    );
    assert!(super::local_connector_server_enables_builtin_kind(
        server,
        BuiltinMcpKind::BrowserTools
    ));
    assert!(!super::local_connector_server_enables_builtin_kind(
        server,
        BuiltinMcpKind::TerminalController
    ));
    assert!(!super::local_connector_server_enables_builtin_kind(
        &local_connector_server(),
        BuiltinMcpKind::BrowserTools
    ));
}

#[test]
fn selected_skill_keeps_local_workspace_routing_without_builtin_tools() {
    let mut task = sample_task(TASK_PROFILE_DEFAULT, Vec::new());
    task.mcp_config.selected_skill_ids = vec!["internal_skill_visualize".to_string()];

    let changed = super::apply_local_connector_runtime_routing_to_task(
        &mut task,
        "local://connector/device-1/workspace-1/apps/web",
        false,
    );

    assert!(changed);
    let server = task
        .mcp_config
        .ephemeral_http_servers
        .first()
        .expect("local connector server");
    assert_eq!(server.name, "local_connector");
    assert!(server
        .headers
        .get(LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER)
        .is_none());
    assert!(server.url.contains("workspace_id=workspace-1"));
}

#[test]
fn local_connector_plan_routing_routes_profile_required_capabilities() {
    let mut task = sample_task(TASK_PROFILE_CHATOS_PLAN, Vec::new());

    let changed = super::apply_local_connector_runtime_routing_to_task(
        &mut task,
        "local://connector/device-1/workspace-1/apps/web",
        false,
    );

    assert!(changed);
    assert!(task.input_payload.is_none());
    let server = task
        .mcp_config
        .ephemeral_http_servers
        .first()
        .expect("local connector server");
    assert_eq!(
        server
            .headers
            .get(LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER)
            .map(String::as_str),
        Some("CodeMaintainerRead,TerminalController,BrowserTools")
    );
}

#[test]
fn local_connector_plan_routing_merges_profile_and_selected_capabilities() {
    let mut task = sample_task(TASK_PROFILE_CHATOS_PLAN, vec!["BrowserTools".to_string()]);

    let changed = super::apply_local_connector_runtime_routing_to_task(
        &mut task,
        "local://connector/device-1/workspace-1/apps/web",
        false,
    );

    assert!(changed);
    let server = task
        .mcp_config
        .ephemeral_http_servers
        .first()
        .expect("local connector server");
    assert_eq!(
        server
            .headers
            .get(LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER)
            .map(String::as_str),
        Some("CodeMaintainerRead,TerminalController,BrowserTools")
    );
    assert!(super::local_connector_server_enables_builtin_kind(
        server,
        BuiltinMcpKind::BrowserTools
    ));
    assert!(super::local_connector_server_enables_builtin_kind(
        server,
        BuiltinMcpKind::TerminalController
    ));
    assert!(super::local_connector_server_enables_builtin_kind(
        server,
        BuiltinMcpKind::CodeMaintainerRead
    ));
}

#[test]
fn workspace_base_check_accepts_relative_child_under_relative_base() {
    assert!(ensure_workspace_is_inside_base(".", ".\\users\\subject\\workspaces\\default").is_ok());
}

#[test]
fn workspace_base_check_rejects_relative_parent_escape() {
    let err = ensure_workspace_is_inside_base(".", "..\\outside")
        .expect_err("parent traversal should be outside workspace base");

    assert!(err.contains("workspace dir is outside"));
}

#[test]
fn normalized_config_preserves_explicit_selection_for_policy_validation() {
    let config = TaskMcpConfig {
        enabled_builtin_kinds: vec![
            "ProjectManagement".to_string(),
            "TaskManager".to_string(),
            "AskUser".to_string(),
            "CodeMaintainerWrite".to_string(),
        ],
        ..TaskMcpConfig::default()
    };

    let sanitized = super::sanitize_task_mcp_config(config);

    assert_eq!(
        sanitized.enabled_builtin_kinds,
        vec![
            "ProjectManagement".to_string(),
            "TaskManager".to_string(),
            "AskUser".to_string(),
            "CodeMaintainerWrite".to_string(),
        ]
    );
}

fn sample_task(task_profile: &str, enabled_builtin_kinds: Vec<String>) -> TaskRecord {
    let now = now_rfc3339();
    TaskRecord {
        id: "task-1".to_string(),
        title: "task".to_string(),
        description: None,
        objective: "objective".to_string(),
        input_payload: None,
        status: TaskStatus::Ready,
        priority: 0,
        tags: Vec::new(),
        default_model_config_id: None,
        memory_thread_id: "memory-1".to_string(),
        tenant_id: "tenant".to_string(),
        subject_id: "subject".to_string(),
        project_id: "project-1".to_string(),
        task_profile: task_profile.to_string(),
        creator_user_id: None,
        creator_username: None,
        creator_display_name: None,
        owner_user_id: Some("owner-1".to_string()),
        owner_username: Some("owner".to_string()),
        owner_display_name: Some("Owner".to_string()),
        result_summary: None,
        process_log: None,
        last_run_id: None,
        schedule: TaskScheduleConfig::default(),
        parent_task_id: None,
        source_run_id: None,
        source_session_id: None,
        source_turn_id: None,
        source_user_message_id: None,
        prerequisite_task_ids: Vec::new(),
        task_tool_state: Default::default(),
        mcp_config: TaskMcpConfig {
            enabled_builtin_kinds,
            ..TaskMcpConfig::default()
        },
        created_at: now.clone(),
        updated_at: now,
        deleted_at: None,
    }
}

fn local_connector_server() -> TaskEphemeralHttpMcpServer {
    TaskEphemeralHttpMcpServer {
        name: "local_connector".to_string(),
        url: "http://127.0.0.1:39230/internal/mcp".to_string(),
        headers: Default::default(),
        auth_mode: Some(TASK_MCP_HTTP_AUTH_LOCAL_CONNECTOR_INTERNAL.to_string()),
    }
}

fn harness_code_server() -> TaskEphemeralHttpMcpServer {
    let mut headers = std::collections::BTreeMap::new();
    headers.insert(
        super::HARNESS_CODE_ENABLED_BUILTIN_KINDS_HEADER.to_string(),
        "CodeMaintainerRead,CodeMaintainerWrite".to_string(),
    );
    TaskEphemeralHttpMcpServer {
        name: "harness_code".to_string(),
        url: "http://127.0.0.1:39210/api/chatos-sync/projects/project-1/harness/mcp".to_string(),
        headers,
        auth_mode: Some(TASK_MCP_HTTP_AUTH_PROJECT_SERVICE_SYNC.to_string()),
    }
}
