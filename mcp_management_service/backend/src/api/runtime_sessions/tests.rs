// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_mcp_management_sdk::{
    ExecutionPlane, ProjectExecutionContext, SandboxProviderKind, WorkspaceExecutionTarget,
    WorkspaceProviderKind,
};

fn request() -> CreateRuntimeSessionRequest {
    CreateRuntimeSessionRequest {
        owner_user_id: "user-1".to_string(),
        agent_key: "task_runner_run_phase".to_string(),
        project_id: "project-1".to_string(),
        run_id: Some("run-1".to_string()),
        turn_id: None,
        task_id: Some("task-1".to_string()),
        task_profile: Some("implementation".to_string()),
        source_session_id: None,
        source_user_message_id: None,
        contact_agent_id: None,
        default_model_config_id: None,
        expected_project_task_ids: Vec::new(),
        locale: None,
        requested_device_id: Some("device-1".to_string()),
        requested_sandbox_provider: None,
        sandbox_target: None,
    }
}

fn context() -> ProjectExecutionContext {
    ProjectExecutionContext {
        project_id: "project-1".to_string(),
        owner_user_id: "user-1".to_string(),
        execution_plane: ExecutionPlane::Cloud,
        workspace_provider: WorkspaceProviderKind::LocalConnector,
        workspace: Some(WorkspaceExecutionTarget {
            device_id: Some("device-1".to_string()),
            workspace_id: "workspace-1".to_string(),
            relative_root: None,
        }),
        sandbox_provider: SandboxProviderKind::None,
        sandbox_pairing_id: None,
        source_type: Some("local_connector".to_string()),
        revision: "revision-1".to_string(),
    }
}

#[test]
fn context_override_must_match_authoritative_device() {
    validate_context_overrides(&request(), &context()).unwrap();
    let mut invalid = request();
    invalid.requested_device_id = Some("another-device".to_string());
    assert!(validate_context_overrides(&invalid, &context()).is_err());
}

#[test]
fn cloud_sandbox_workspace_authorizes_runtime_sandbox_target() {
    let mut request = request();
    request.requested_device_id = None;
    request.requested_sandbox_provider = Some(SandboxProviderKind::Cloud);
    request.sandbox_target = Some(SandboxExecutionTarget {
        provider: SandboxProviderKind::Cloud,
        pairing_id: None,
        sandbox_id: "sandbox-1".to_string(),
        lease_id: "lease-1".to_string(),
        is_environment: false,
        service_id: None,
    });
    let mut context = context();
    context.workspace_provider = WorkspaceProviderKind::CloudSandbox;
    context.workspace = None;
    validate_context_overrides(&request, &context).unwrap();
}

#[test]
fn local_sandbox_target_requires_the_authoritative_pairing() {
    let mut request = request();
    request.requested_sandbox_provider = Some(SandboxProviderKind::LocalConnector);
    request.sandbox_target = Some(SandboxExecutionTarget {
        provider: SandboxProviderKind::LocalConnector,
        pairing_id: Some("pairing-1".to_string()),
        sandbox_id: "sandbox-1".to_string(),
        lease_id: "lease-1".to_string(),
        is_environment: false,
        service_id: None,
    });
    let mut context = context();
    context.sandbox_provider = SandboxProviderKind::LocalConnector;
    context.sandbox_pairing_id = Some("pairing-1".to_string());
    validate_context_overrides(&request, &context).expect("the exact pairing must be accepted");

    context.sandbox_pairing_id = Some("pairing-2".to_string());
    assert!(validate_context_overrides(&request, &context).is_err());
}

#[test]
fn sandbox_target_provider_shape_fails_closed() {
    let local_without_pairing = SandboxExecutionTarget {
        provider: SandboxProviderKind::LocalConnector,
        pairing_id: None,
        sandbox_id: "sandbox-1".to_string(),
        lease_id: "lease-1".to_string(),
        is_environment: false,
        service_id: None,
    };
    assert!(normalize_sandbox_target(Some(local_without_pairing)).is_err());

    let cloud_with_pairing = SandboxExecutionTarget {
        provider: SandboxProviderKind::Cloud,
        pairing_id: Some("pairing-1".to_string()),
        sandbox_id: "sandbox-1".to_string(),
        lease_id: "lease-1".to_string(),
        is_environment: false,
        service_id: None,
    };
    assert!(normalize_sandbox_target(Some(cloud_with_pairing)).is_err());
}

#[test]
fn only_registered_system_agent_keys_are_accepted() {
    assert_eq!(
        parse_agent_key("task_runner_run_phase").unwrap(),
        SystemAgentKey::TaskRunnerRunPhase
    );
    assert!(parse_agent_key("arbitrary-agent").is_err());
}

#[test]
fn local_only_agents_cannot_create_managed_runtime_sessions() {
    let error = parse_agent_key("local_connector_command_approval_agent")
        .expect_err("local-only Agent must never enter MCP Management");

    assert!(format!("{error:?}").contains("does not use the managed MCP Tool Plane"));
}

#[test]
fn retired_local_task_runner_agents_cannot_create_runtime_sessions() {
    assert!(parse_agent_key("task_runner_local_plan_phase").is_err());
    assert!(parse_agent_key("task_runner_local_run_phase").is_err());
}

#[test]
fn task_process_log_session_requires_exact_run_task_and_agent_scope() {
    let route = system_route(SystemMcpKey::TaskProcessLog);
    let request = request();
    validate_task_runner_provider_context(
        SystemAgentKey::TaskRunnerRunPhase,
        &request,
        &[],
        std::slice::from_ref(&route),
    )
    .expect("bound Task Runner run should be accepted");

    let mut missing_run = request.clone();
    missing_run.run_id = None;
    let error = validate_task_runner_provider_context(
        SystemAgentKey::TaskRunnerRunPhase,
        &missing_run,
        &[],
        std::slice::from_ref(&route),
    )
    .expect_err("run binding is required");
    assert!(format!("{error:?}").contains("run_id"));

    assert!(validate_task_runner_provider_context(
        SystemAgentKey::ChatosConversationAgent,
        &request,
        &[],
        &[route],
    )
    .is_err());
}

#[tokio::test]
async fn ask_user_route_is_pinned_to_the_agent_host_and_requires_task_run_scope() {
    let mut routes = vec![system_route(SystemMcpKey::AskUser)];
    bind_agent_callback_routes(routes.as_mut_slice(), SystemAgentKey::TaskRunnerRunPhase);
    assert_eq!(routes[0].provider_ref.as_deref(), Some("task-runner"));
    let state = AppState::new(crate::config::AppConfig::test())
        .await
        .expect("test state");
    assert!(state.providers.supports(&routes[0]));

    validate_task_runner_provider_context(
        SystemAgentKey::TaskRunnerRunPhase,
        &request(),
        &[],
        routes.as_slice(),
    )
    .expect("bound Task Runner Ask User route should be accepted");

    let mut missing_task = request();
    missing_task.task_id = None;
    let error = validate_task_runner_provider_context(
        SystemAgentKey::TaskRunnerRunPhase,
        &missing_task,
        &[],
        routes.as_slice(),
    )
    .expect_err("task binding is required");
    assert!(format!("{error:?}").contains("task_id"));

    bind_agent_callback_routes(
        routes.as_mut_slice(),
        SystemAgentKey::ChatosConversationAgent,
    );
    assert_eq!(routes[0].provider_ref.as_deref(), Some("chatos"));
    assert!(state.providers.supports(&routes[0]));

    let mut chatos_request = request();
    chatos_request.agent_key = SystemAgentKey::ChatosConversationAgent.as_str().to_string();
    chatos_request.run_id = None;
    chatos_request.task_id = None;
    chatos_request.turn_id = Some("turn-1".to_string());
    chatos_request.source_session_id = Some("conversation-1".to_string());
    chatos_request.source_user_message_id = Some("message-1".to_string());
    validate_task_runner_provider_context(
        SystemAgentKey::ChatosConversationAgent,
        &chatos_request,
        &[],
        routes.as_slice(),
    )
    .expect("bound ChatOS Ask User route should be accepted");

    chatos_request.turn_id = None;
    let error = validate_task_runner_provider_context(
        SystemAgentKey::ChatosConversationAgent,
        &chatos_request,
        &[],
        routes.as_slice(),
    )
    .expect_err("ChatOS turn binding is required");
    assert!(format!("{error:?}").contains("turn_id"));
}

#[test]
fn memory_reader_routes_are_pinned_to_the_runtime_contact_agent() {
    let mut routes = vec![
        system_route(SystemMcpKey::MemorySkillReader),
        system_route(SystemMcpKey::MemoryCommandReader),
        system_route(SystemMcpKey::MemoryPluginReader),
    ];

    bind_chatos_memory_routes(
        routes.as_mut_slice(),
        SystemAgentKey::ChatosConversationAgent,
        Some(" contact-agent-1 "),
        Some("conversation-1"),
    );

    assert!(routes.iter().all(|route| {
        route.provider_kind == McpProviderKind::InternalService
            && route.provider_ref.as_deref() == Some("chatos:memory:contact-agent-1")
            && !route.cancel_supported
    }));
}

#[test]
fn memory_reader_routes_are_unavailable_without_a_bound_contact_agent() {
    let mut routes = vec![system_route(SystemMcpKey::MemorySkillReader)];

    bind_chatos_memory_routes(
        routes.as_mut_slice(),
        SystemAgentKey::ChatosConversationAgent,
        None,
        Some("conversation-1"),
    );

    assert_eq!(routes[0].provider_kind, McpProviderKind::Unavailable);
    assert_eq!(routes[0].provider_ref, None);
    assert!(!routes[0].allow_writes);
}

#[test]
fn task_runner_service_session_requires_chatos_source_scope() {
    let route = system_route(SystemMcpKey::TaskRunnerService);
    let mut request = request();
    request.agent_key = SystemAgentKey::ChatosConversationAgent.as_str().to_string();
    assert!(validate_task_runner_provider_context(
        SystemAgentKey::ChatosConversationAgent,
        &request,
        &[],
        std::slice::from_ref(&route),
    )
    .is_err());

    request.source_session_id = Some("conversation-1".to_string());
    request.source_user_message_id = Some("message-1".to_string());
    validate_task_runner_provider_context(
        SystemAgentKey::ChatosConversationAgent,
        &request,
        &[],
        std::slice::from_ref(&route),
    )
    .expect("complete Chatos source binding should be accepted");

    let error = validate_task_runner_provider_context(
        SystemAgentKey::ProjectRequirementExecutionPlannerAgent,
        &request,
        &[],
        &[route],
    )
    .expect_err("project execution scope is required");
    assert!(format!("{error:?}").contains("expected_project_task_ids"));
}

#[test]
fn capability_response_must_match_the_requested_identity() {
    let capabilities = chatos_plugin_management_sdk::ResolvedAgentCapabilities {
        agent_key: "task_runner_run_phase".to_string(),
        owner_user_id: "user-1".to_string(),
        policy_revision: "policy-1".to_string(),
        generated_at: "now".to_string(),
        agent_enabled: true,
        mcps: Vec::new(),
        skills: Vec::new(),
        plugins: Vec::new(),
        local_connector_requirements: Vec::new(),
    };
    validate_capability_identity(&capabilities, "task_runner_run_phase", "user-1").unwrap();
    assert!(
        validate_capability_identity(&capabilities, "task_runner_plan_phase", "user-1").is_err()
    );
    assert!(
        validate_capability_identity(&capabilities, "task_runner_run_phase", "another-user")
            .is_err()
    );
}

#[test]
fn required_route_without_registered_provider_adapter_is_blocked() {
    let required_resource_ids = HashSet::from(["required-mcp".to_string()]);
    let routes = vec![chatos_mcp_management_sdk::ResolvedMcpRoute {
        resource_id: "required-mcp".to_string(),
        server_name: "required".to_string(),
        provider_kind: chatos_mcp_management_sdk::McpProviderKind::ExternalHttp,
        provider_ref: Some("mcp-resource:required-mcp".to_string()),
        tool_namespace: "required".to_string(),
        allow_writes: false,
        retry_class: chatos_mcp_management_sdk::McpRetryClass::NoRetry,
        cancel_supported: false,
        reason: "test".to_string(),
    }];
    assert_eq!(
        required_routes_without_provider_adapter(&required_resource_ids, &routes, |_| false),
        vec!["required-mcp"]
    );
}

#[test]
fn cloud_sandbox_routes_are_bound_to_opaque_runtime_target() {
    let mut routes = vec![chatos_mcp_management_sdk::ResolvedMcpRoute {
        resource_id: "builtin_code_maintainer_read".to_string(),
        server_name: "code_maintainer_read".to_string(),
        provider_kind: McpProviderKind::CloudSandbox,
        provider_ref: Some("project:project-1".to_string()),
        tool_namespace: "code_maintainer_read".to_string(),
        allow_writes: false,
        retry_class: chatos_mcp_management_sdk::McpRetryClass::IdempotentRead,
        cancel_supported: true,
        reason: "test".to_string(),
    }];
    let target = SandboxExecutionTarget {
        provider: SandboxProviderKind::Cloud,
        pairing_id: None,
        sandbox_id: "sandbox-1".to_string(),
        lease_id: "lease-1".to_string(),
        is_environment: false,
        service_id: None,
    };
    bind_runtime_sandbox_routes(routes.as_mut_slice(), Some(&target));
    assert_eq!(
        routes[0].provider_ref.as_deref(),
        Some("sandbox:sandbox-1/lease:lease-1")
    );
}

#[test]
fn local_runtime_sandbox_rebinds_only_workspace_tools_to_the_exact_lease() {
    let mut routes = vec![
        system_route(SystemMcpKey::CodeMaintainerRead),
        system_route(SystemMcpKey::CodeMaintainerWrite),
        system_route(SystemMcpKey::TerminalController),
        system_route(SystemMcpKey::BrowserTools),
    ];
    for route in routes.iter_mut() {
        route.provider_kind = McpProviderKind::LocalConnector;
        route.provider_ref = Some("device:device-1/workspace:workspace-1".to_string());
    }
    let target = SandboxExecutionTarget {
        provider: SandboxProviderKind::LocalConnector,
        pairing_id: Some("pairing-1".to_string()),
        sandbox_id: "sandbox-1".to_string(),
        lease_id: "lease-1".to_string(),
        is_environment: false,
        service_id: None,
    };

    bind_runtime_sandbox_routes(routes.as_mut_slice(), Some(&target));

    for route in &routes[..3] {
        assert_eq!(route.provider_kind, McpProviderKind::LocalConnector);
        assert_eq!(
            route.provider_ref.as_deref(),
            Some("sandbox-pairing:pairing-1/sandbox:sandbox-1/lease:lease-1")
        );
    }
    assert_eq!(
        routes[3].provider_ref.as_deref(),
        Some("device:device-1/workspace:workspace-1")
    );
}

#[test]
fn local_workspace_routes_remain_device_bound_without_a_sandbox_target() {
    let mut routes = vec![system_route(SystemMcpKey::CodeMaintainerRead)];
    routes[0].provider_kind = McpProviderKind::LocalConnector;
    routes[0].provider_ref = Some("device:device-1/workspace:workspace-1".to_string());

    bind_runtime_sandbox_routes(routes.as_mut_slice(), None);

    assert_eq!(routes[0].provider_kind, McpProviderKind::LocalConnector);
    assert_eq!(
        routes[0].provider_ref.as_deref(),
        Some("device:device-1/workspace:workspace-1")
    );
}

#[test]
fn cloud_sandbox_images_are_bound_without_a_runtime_sandbox_target() {
    let mut routes = vec![sandbox_images_route(McpProviderKind::CloudSandbox)];
    let mut context = context();
    context.sandbox_provider = SandboxProviderKind::Cloud;

    bind_runtime_sandbox_routes(routes.as_mut_slice(), None);
    bind_sandbox_image_routes(routes.as_mut_slice(), &context);

    assert_eq!(routes[0].provider_kind, McpProviderKind::CloudSandbox);
    assert_eq!(
        routes[0].provider_ref.as_deref(),
        Some(crate::providers::sandbox_images_cloud_provider_ref())
    );
    assert!(!routes[0].cancel_supported);
}

#[test]
fn local_sandbox_images_are_bound_to_the_authoritative_pairing() {
    let mut routes = vec![sandbox_images_route(McpProviderKind::LocalConnector)];
    let mut context = context();
    context.sandbox_provider = SandboxProviderKind::LocalConnector;
    context.sandbox_pairing_id = Some(" pairing-1 ".to_string());

    bind_sandbox_image_routes(routes.as_mut_slice(), &context);

    assert_eq!(routes[0].provider_kind, McpProviderKind::LocalConnector);
    assert_eq!(
        routes[0].provider_ref.as_deref(),
        Some("sandbox-images:local:pairing-1")
    );
    assert!(!routes[0].cancel_supported);
}

#[test]
fn local_sandbox_images_are_unavailable_without_a_pairing() {
    let mut routes = vec![sandbox_images_route(McpProviderKind::LocalConnector)];
    let mut context = context();
    context.sandbox_provider = SandboxProviderKind::LocalConnector;

    bind_sandbox_image_routes(routes.as_mut_slice(), &context);

    assert_eq!(routes[0].provider_kind, McpProviderKind::Unavailable);
    assert_eq!(routes[0].provider_ref, None);
    assert!(!routes[0].allow_writes);
    assert!(!routes[0].cancel_supported);
}

#[test]
fn cloud_sandbox_binding_does_not_overwrite_sandbox_images() {
    let mut routes = vec![sandbox_images_route(McpProviderKind::CloudSandbox)];
    routes[0].provider_ref =
        Some(crate::providers::sandbox_images_cloud_provider_ref().to_string());
    let target = SandboxExecutionTarget {
        provider: SandboxProviderKind::Cloud,
        pairing_id: None,
        sandbox_id: "sandbox-1".to_string(),
        lease_id: "lease-1".to_string(),
        is_environment: false,
        service_id: None,
    };

    bind_runtime_sandbox_routes(routes.as_mut_slice(), Some(&target));

    assert_eq!(
        routes[0].provider_ref.as_deref(),
        Some(crate::providers::sandbox_images_cloud_provider_ref())
    );
}

fn sandbox_images_route(provider_kind: McpProviderKind) -> ResolvedMcpRoute {
    let mut route = system_route(SystemMcpKey::SandboxImages);
    route.provider_kind = provider_kind;
    route.provider_ref = Some("project:project-1".to_string());
    route.cancel_supported = true;
    route
}

fn system_route(key: SystemMcpKey) -> ResolvedMcpRoute {
    let descriptor = chatos_mcp::system_mcp_descriptor(key);
    ResolvedMcpRoute {
        resource_id: descriptor.resource_id.to_string(),
        server_name: descriptor.server_name.to_string(),
        provider_kind: McpProviderKind::InternalService,
        provider_ref: Some(descriptor.owner_service.to_string()),
        tool_namespace: descriptor.server_name.to_string(),
        allow_writes: descriptor.allow_writes,
        retry_class: chatos_mcp_management_sdk::McpRetryClass::NoRetry,
        cancel_supported: false,
        reason: "test".to_string(),
    }
}
