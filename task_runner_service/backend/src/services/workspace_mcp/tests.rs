// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::{
    now_rfc3339, TaskEphemeralHttpMcpServer, TaskMcpConfig, TaskRecord, TaskScheduleConfig,
    TaskStatus, TASK_MCP_HTTP_AUTH_PROJECT_SERVICE_SYNC, TASK_PROFILE_CHATOS_PLAN,
    TASK_PROFILE_DEFAULT,
};

use super::{
    ensure_workspace_is_inside_base, runtime_selected_builtin_kinds, selected_builtin_kinds,
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
fn local_connector_roots_are_rejected_by_cloud_routing() {
    assert!(super::is_local_connector_project_root(
        "local://connector/device-1/workspace-1/apps/web"
    ));
    assert!(!super::is_local_connector_project_root(
        "harness://project/project-1"
    ));
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
