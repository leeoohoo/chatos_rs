// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::models::TaskMcpConfig;

#[test]
fn required_write_adds_required_read_with_same_source() {
    let config = TaskMcpConfig {
        enabled_builtin_kinds: Vec::new(),
        ..TaskMcpConfig::default()
    };
    let source = McpCapabilityRequirementSource::CallerContract(
        AgentMcpCaller::LocalConnectorClientAgent,
    );

    let resolution = resolve_mcp_config(TaskMcpResolutionInput {
        mcp_config: &config,
        task_profile: "default",
        schedule_mode: TaskScheduleMode::Manual,
        source_session_id: None,
        source_user_message_id: None,
        active_host_backends: &[],
        caller_requirements: &[McpCapabilityRequirement::new(
            BuiltinMcpKind::CodeMaintainerWrite,
            source,
        )],
    });

    assert!(resolution
        .required_builtin_kinds
        .contains(&RequiredBuiltinCapability {
            kind: BuiltinMcpKind::CodeMaintainerWrite,
            source,
        }));
    assert!(resolution
        .required_builtin_kinds
        .contains(&RequiredBuiltinCapability {
            kind: BuiltinMcpKind::CodeMaintainerRead,
            source,
        }));
    assert!(resolution
        .server_local_builtin_kinds
        .contains(&BuiltinMcpKind::CodeMaintainerRead));
    assert!(resolution
        .server_local_builtin_kinds
        .contains(&BuiltinMcpKind::CodeMaintainerWrite));
}

#[test]
fn required_capability_routes_to_active_host() {
    let config = TaskMcpConfig {
        enabled_builtin_kinds: Vec::new(),
        ..TaskMcpConfig::default()
    };
    let resolution = resolve_mcp_config(TaskMcpResolutionInput {
        mcp_config: &config,
        task_profile: "default",
        schedule_mode: TaskScheduleMode::Manual,
        source_session_id: None,
        source_user_message_id: None,
        active_host_backends: &[BuiltinHostBackend::LocalConnector],
        caller_requirements: &[McpCapabilityRequirement::new(
            BuiltinMcpKind::TerminalController,
            McpCapabilityRequirementSource::CallerContract(
                AgentMcpCaller::LocalConnectorClientAgent,
            ),
        )],
    });

    assert_eq!(
        hosted_builtin_kinds_for(&resolution, BuiltinHostBackend::LocalConnector),
        vec![BuiltinMcpKind::TerminalController]
    );
    assert!(!resolution
        .server_local_builtin_kinds
        .contains(&BuiltinMcpKind::TerminalController));
}

#[test]
fn task_runner_run_phase_requirements_are_caller_required() {
    let mut task = sample_task(TaskMcpConfig {
        enabled_builtin_kinds: Vec::new(),
        ..TaskMcpConfig::default()
    });
    task.schedule.mode = TaskScheduleMode::Manual;

    let resolution = resolve_task_mcp(&task, &[]);

    assert!(resolution.requested_builtin_kinds.is_empty());
    assert!(resolution
        .required_builtin_kinds
        .contains(&RequiredBuiltinCapability {
            kind: BuiltinMcpKind::TaskManager,
            source: McpCapabilityRequirementSource::CallerContract(
                AgentMcpCaller::TaskRunnerRunPhase,
            ),
        }));
    assert!(resolution
        .required_builtin_kinds
        .contains(&RequiredBuiltinCapability {
            kind: BuiltinMcpKind::AskUser,
            source: McpCapabilityRequirementSource::CallerContract(
                AgentMcpCaller::TaskRunnerRunPhase,
            ),
        }));
    assert!(resolution
        .server_local_builtin_kinds
        .contains(&BuiltinMcpKind::TaskManager));
    assert!(resolution
        .server_local_builtin_kinds
        .contains(&BuiltinMcpKind::AskUser));
}

#[test]
fn fixed_task_tools_are_not_reported_as_requested_capabilities() {
    let task = sample_task(TaskMcpConfig {
        enabled_builtin_kinds: vec![
            "CodeMaintainerRead".to_string(),
            "TaskManager".to_string(),
            "AskUser".to_string(),
        ],
        ..TaskMcpConfig::default()
    });

    let resolution = resolve_task_mcp(&task, &[]);

    assert_eq!(
        resolution.requested_builtin_kinds,
        vec![BuiltinMcpKind::CodeMaintainerRead]
    );
    assert!(resolution
        .required_builtin_kinds
        .contains(&RequiredBuiltinCapability {
            kind: BuiltinMcpKind::TaskManager,
            source: McpCapabilityRequirementSource::CallerContract(
                AgentMcpCaller::TaskRunnerRunPhase,
            ),
        }));
    assert!(resolution
        .required_builtin_kinds
        .contains(&RequiredBuiltinCapability {
            kind: BuiltinMcpKind::AskUser,
            source: McpCapabilityRequirementSource::CallerContract(
                AgentMcpCaller::TaskRunnerRunPhase,
            ),
        }));
}

#[test]
fn chatos_async_source_wins_over_run_phase_for_fixed_task_tools() {
    let mut task = sample_task(TaskMcpConfig {
        enabled_builtin_kinds: Vec::new(),
        ..TaskMcpConfig::default()
    });
    task.schedule.mode = TaskScheduleMode::ContactAsync;

    let resolution = resolve_task_mcp(&task, &[]);

    assert!(resolution
        .required_builtin_kinds
        .contains(&RequiredBuiltinCapability {
            kind: BuiltinMcpKind::TaskManager,
            source: McpCapabilityRequirementSource::CallerContract(
                AgentMcpCaller::ChatosAsyncPlanner,
            ),
        }));
    assert!(!resolution
        .required_builtin_kinds
        .contains(&RequiredBuiltinCapability {
            kind: BuiltinMcpKind::TaskManager,
            source: McpCapabilityRequirementSource::CallerContract(
                AgentMcpCaller::TaskRunnerRunPhase,
            ),
        }));
}

#[test]
fn plan_profile_requirements_are_fixed_and_host_routable() {
    let config = TaskMcpConfig {
        enabled_builtin_kinds: Vec::new(),
        ..TaskMcpConfig::default()
    };
    let resolution = resolve_mcp_config(TaskMcpResolutionInput {
        mcp_config: &config,
        task_profile: TASK_PROFILE_CHATOS_PLAN,
        schedule_mode: TaskScheduleMode::Manual,
        source_session_id: None,
        source_user_message_id: None,
        active_host_backends: &[BuiltinHostBackend::HarnessCode],
        caller_requirements: &[],
    });

    assert!(resolution.required_builtin_kinds.iter().any(|required| {
        required.kind == BuiltinMcpKind::ProjectManagement
            && required.source == McpCapabilityRequirementSource::TaskProfileChatosPlan
    }));
    assert_eq!(
        hosted_builtin_kinds_for(&resolution, BuiltinHostBackend::HarnessCode),
        vec![BuiltinMcpKind::CodeMaintainerRead]
    );
    assert!(resolution
        .server_local_builtin_kinds
        .contains(&BuiltinMcpKind::ProjectManagement));
    assert!(!resolution
        .server_local_builtin_kinds
        .contains(&BuiltinMcpKind::CodeMaintainerRead));
}

#[test]
fn requested_capabilities_are_recovered_from_legacy_host_headers() {
    let config = TaskMcpConfig {
        enabled_builtin_kinds: Vec::new(),
        ephemeral_http_servers: vec![crate::models::TaskEphemeralHttpMcpServer {
            name: "local_connector".to_string(),
            url: "http://127.0.0.1:39230/mcp".to_string(),
            headers: std::collections::BTreeMap::from([(
                LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER.to_string(),
                "CodeMaintainerRead,TerminalController".to_string(),
            )]),
            auth_mode: None,
        }],
        ..TaskMcpConfig::default()
    };

    let resolution = resolve_mcp_config(TaskMcpResolutionInput {
        mcp_config: &config,
        task_profile: "default",
        schedule_mode: TaskScheduleMode::Manual,
        source_session_id: None,
        source_user_message_id: None,
        active_host_backends: &[BuiltinHostBackend::LocalConnector],
        caller_requirements: &[],
    });

    assert!(resolution
        .requested_builtin_kinds
        .contains(&BuiltinMcpKind::CodeMaintainerRead));
    assert!(resolution
        .requested_builtin_kinds
        .contains(&BuiltinMcpKind::TerminalController));
    assert_eq!(
        hosted_builtin_kinds_for(&resolution, BuiltinHostBackend::LocalConnector),
        vec![
            BuiltinMcpKind::CodeMaintainerRead,
            BuiltinMcpKind::TerminalController,
        ]
    );
    assert!(resolution.server_local_builtin_kinds.is_empty());
}

fn sample_task(mcp_config: TaskMcpConfig) -> TaskRecord {
    let now = crate::models::now_rfc3339();
    TaskRecord {
        id: "task-1".to_string(),
        title: "task".to_string(),
        description: None,
        objective: "objective".to_string(),
        input_payload: None,
        status: crate::models::TaskStatus::Ready,
        priority: 0,
        tags: Vec::new(),
        default_model_config_id: None,
        memory_thread_id: "thread-1".to_string(),
        tenant_id: "tenant".to_string(),
        subject_id: "subject".to_string(),
        project_id: crate::models::PUBLIC_PROJECT_ID.to_string(),
        task_profile: crate::models::TASK_PROFILE_DEFAULT.to_string(),
        creator_user_id: None,
        creator_username: None,
        creator_display_name: None,
        owner_user_id: None,
        owner_username: None,
        owner_display_name: None,
        result_summary: None,
        process_log: None,
        last_run_id: None,
        schedule: crate::models::TaskScheduleConfig::default(),
        parent_task_id: None,
        source_run_id: None,
        source_session_id: None,
        source_turn_id: None,
        source_user_message_id: None,
        prerequisite_task_ids: Vec::new(),
        task_tool_state: crate::models::TaskToolState::default(),
        mcp_config,
        created_at: now.clone(),
        updated_at: now,
        deleted_at: None,
    }
}
