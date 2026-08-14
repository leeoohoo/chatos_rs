// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_mcp_management_sdk::{
    ExecutionPlane, HarnessBranchTarget, ProjectExecutionContext, RuntimeWorkspaceRouteTarget,
    SandboxProviderKind, WorkspaceExecutionTarget, WorkspaceProviderKind,
};
use chatos_plugin_management_sdk::{
    AgentBindingRecord, BindingConditions, McpRecord, McpRuntime, ResolvedAgentCapabilities,
    ResolvedMcp, ResourceMetadata, ResourceSecurity, SelectedPluginRef,
};

fn request() -> CreateRuntimeSessionRequest {
    CreateRuntimeSessionRequest {
        tenant_id: "tenant-1".to_string(),
        owner_user_id: "user-1".to_string(),
        owner_role: None,
        agent_key: SystemAgentKey::TaskRunnerRunPhase.as_str().to_string(),
        project_id: "project-1".to_string(),
        run_id: Some("run-1".to_string()),
        turn_id: None,
        task_id: Some("task-1".to_string()),
        task_profile: Some("implementation".to_string()),
        source_session_id: None,
        source_user_message_id: None,
        contact_agent_id: None,
        default_model_config_id: None,
        tool_result_max_chars: Some(40_000),
        expected_project_task_ids: Vec::new(),
        requested_mcp_ids: None,
        selected_plugins: Vec::new(),
        plugin_command_invocations: Vec::new(),
        locale: None,
        workspace_route: Some(RuntimeWorkspaceRouteTarget::LocalConnector),
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

fn resolved_mcp(resource_id: &str, required: bool) -> ResolvedMcp {
    ResolvedMcp {
        resource: McpRecord {
            id: resource_id.to_string(),
            owner_user_id: "system".to_string(),
            owner_kind: "system".to_string(),
            visibility: "system_private".to_string(),
            source_kind: "system_seed".to_string(),
            name: resource_id.to_string(),
            display_name: resource_id.to_string(),
            description: None,
            enabled: true,
            runtime: McpRuntime::default(),
            security: ResourceSecurity::default(),
            metadata: ResourceMetadata::default(),
            plugin_component: Default::default(),
            created_by: "system".to_string(),
            updated_by: "system".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        },
        binding: AgentBindingRecord {
            id: format!("binding-{resource_id}"),
            agent_key: SystemAgentKey::TaskRunnerRunPhase.as_str().to_string(),
            binding_scope: "global_default".to_string(),
            owner_user_id: None,
            resource_kind: "mcp".to_string(),
            resource_id: resource_id.to_string(),
            enabled: true,
            required,
            priority: 100,
            conditions: BindingConditions::default(),
            component_allowlist: Vec::new(),
            tool_allowlist: Vec::new(),
            tool_blocklist: Vec::new(),
            created_by: "system".to_string(),
            updated_by: "system".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        },
        available: true,
        status: "ready".to_string(),
        reason: None,
        tool_snapshot: Vec::new(),
    }
}

fn capabilities_for_scope_test() -> ResolvedAgentCapabilities {
    ResolvedAgentCapabilities {
        agent_key: SystemAgentKey::TaskRunnerRunPhase.as_str().to_string(),
        owner_user_id: "user-1".to_string(),
        policy_revision: "policy-1".to_string(),
        generated_at: "now".to_string(),
        agent_enabled: true,
        mcps: vec![
            resolved_mcp("required-core", true),
            resolved_mcp("builtin_browser_tools", false),
            resolved_mcp("builtin_web_tools", false),
        ],
        skills: Vec::new(),
        plugins: Vec::new(),
        local_connector_requirements: Vec::new(),
    }
}

#[test]
fn requested_mcp_scope_keeps_required_resources_and_only_selected_optionals() {
    let mut capabilities = capabilities_for_scope_test();

    apply_requested_mcp_scope(
        &mut capabilities,
        Some(&["builtin_browser_tools".to_string()]),
    )
    .expect("selected browser scope");

    assert_eq!(
        capabilities
            .mcps
            .iter()
            .map(|resolved| resolved.resource.id.as_str())
            .collect::<Vec<_>>(),
        vec!["required-core", "builtin_browser_tools"]
    );
    assert!(capabilities
        .mcps
        .iter()
        .all(|resolved| resolved.binding.required));
}

#[test]
fn empty_requested_mcp_scope_removes_all_optional_resources() {
    let mut capabilities = capabilities_for_scope_test();

    apply_requested_mcp_scope(&mut capabilities, Some(&[])).expect("empty optional scope");

    assert_eq!(capabilities.mcps.len(), 1);
    assert_eq!(capabilities.mcps[0].resource.id, "required-core");
}

#[test]
fn requested_mcp_scope_rejects_resources_outside_agent_policy() {
    let mut capabilities = capabilities_for_scope_test();

    let error = apply_requested_mcp_scope(&mut capabilities, Some(&["not-configured".to_string()]))
        .expect_err("unknown resource must fail closed");

    assert!(format!("{error:?}").contains("not-configured"));
}

#[test]
fn capability_runtime_provider_uses_project_execution_locality_not_backend_name() {
    let mut project_context = context();

    project_context.workspace_provider = WorkspaceProviderKind::Harness;
    assert_eq!(capability_runtime_provider(&project_context), "cloud");

    project_context.workspace_provider = WorkspaceProviderKind::CloudSandbox;
    assert_eq!(capability_runtime_provider(&project_context), "cloud");

    project_context.workspace_provider = WorkspaceProviderKind::CloudStorage;
    assert_eq!(capability_runtime_provider(&project_context), "cloud");

    project_context.workspace_provider = WorkspaceProviderKind::LocalConnector;
    assert_eq!(
        capability_runtime_provider(&project_context),
        "local_connector"
    );
}

#[test]
fn local_connector_route_requires_authoritative_local_workspace() {
    validate_context_overrides(&request(), &context()).unwrap();
    let mut invalid_context = context();
    invalid_context.workspace_provider = WorkspaceProviderKind::Harness;
    assert!(validate_context_overrides(&request(), &invalid_context).is_err());
}

#[test]
fn cloud_sandbox_workspace_authorizes_runtime_sandbox_target() {
    let mut request = request();
    request.workspace_route = Some(RuntimeWorkspaceRouteTarget::CloudSandbox {
        target: SandboxExecutionTarget {
            provider: SandboxProviderKind::Cloud,
            pairing_id: None,
            sandbox_id: "sandbox-1".to_string(),
            lease_id: "lease-1".to_string(),
            is_environment: false,
            service_id: None,
        },
    });
    let mut context = context();
    context.workspace_provider = WorkspaceProviderKind::CloudSandbox;
    context.workspace = None;
    context.sandbox_provider = SandboxProviderKind::Cloud;
    context.source_type = Some("cloud".to_string());
    validate_context_overrides(&request, &context).unwrap();
}

#[test]
fn harness_route_requires_a_cloud_project() {
    let mut request = request();
    request.workspace_route = Some(RuntimeWorkspaceRouteTarget::Harness {
        branch: HarnessBranchTarget::Run {
            branch_id: "branch-1".to_string(),
            branch_ref: "chatos/runs/run-1".to_string(),
            base_branch: "main".to_string(),
            base_commit: "base-commit".to_string(),
        },
    });
    let mut context = context();
    context.workspace_provider = WorkspaceProviderKind::Harness;
    context.workspace = None;
    context.source_type = Some("cloud".to_string());
    validate_context_overrides(&request, &context).expect("cloud Harness route must be accepted");

    context.source_type = Some("local_connector".to_string());
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
        parse_agent_key(SystemAgentKey::TaskRunnerRunPhase.as_str()).unwrap(),
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
        agent_key: SystemAgentKey::TaskRunnerRunPhase.as_str().to_string(),
        owner_user_id: "user-1".to_string(),
        policy_revision: "policy-1".to_string(),
        generated_at: "now".to_string(),
        agent_enabled: true,
        mcps: Vec::new(),
        skills: Vec::new(),
        plugins: Vec::new(),
        local_connector_requirements: Vec::new(),
    };
    validate_capability_identity(
        &capabilities,
        SystemAgentKey::TaskRunnerRunPhase.as_str(),
        "user-1",
    )
    .unwrap();
    assert!(validate_capability_identity(
        &capabilities,
        SystemAgentKey::TaskRunnerPlanPhase.as_str(),
        "user-1",
    )
    .is_err());
    assert!(validate_capability_identity(
        &capabilities,
        SystemAgentKey::TaskRunnerRunPhase.as_str(),
        "another-user",
    )
    .is_err());
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
fn selected_plugin_scope_rejects_empty_duplicate_unknown_and_agent_selection() {
    let mut capabilities = capabilities_for_scope_test();
    let empty = SelectedPluginRef {
        plugin_id: " ".to_string(),
        selected_skill_ids: Vec::new(),
        selected_command_ids: Vec::new(),
        selected_agent_ids: Vec::new(),
    };
    assert!(apply_selected_plugin_scope(&mut capabilities, &[empty]).is_err());

    let selected = SelectedPluginRef {
        plugin_id: "missing-plugin".to_string(),
        selected_skill_ids: Vec::new(),
        selected_command_ids: Vec::new(),
        selected_agent_ids: Vec::new(),
    };
    assert!(apply_selected_plugin_scope(
        &mut capabilities_for_scope_test(),
        &[selected.clone(), selected.clone()],
    )
    .is_err());
    assert!(apply_selected_plugin_scope(&mut capabilities_for_scope_test(), &[selected]).is_err());

    let agent = SelectedPluginRef {
        plugin_id: "missing-plugin".to_string(),
        selected_skill_ids: Vec::new(),
        selected_command_ids: Vec::new(),
        selected_agent_ids: vec!["reviewer".to_string()],
    };
    assert!(apply_selected_plugin_scope(&mut capabilities_for_scope_test(), &[agent]).is_err());
}

#[test]
fn plugin_command_invocations_are_selected_unique_and_size_bounded() {
    let selected = vec![SelectedPluginRef {
        plugin_id: "plugin-review".to_string(),
        selected_skill_ids: Vec::new(),
        selected_command_ids: vec!["review".to_string()],
        selected_agent_ids: Vec::new(),
    }];
    let valid = chatos_plugin_management_sdk::PluginCommandInvocation {
        plugin_id: " plugin-review ".to_string(),
        command_id: " review ".to_string(),
        arguments: Some(" src/lib.rs ".to_string()),
    };
    let normalized = validate_plugin_command_invocations(&selected, std::slice::from_ref(&valid))
        .expect("selected command invocation");
    assert_eq!(
        normalized.get(&("plugin-review".to_string(), "review".to_string())),
        Some(&Some("src/lib.rs".to_string()))
    );
    assert!(validate_plugin_command_invocations(&selected, &[valid.clone(), valid]).is_err());

    let unknown = chatos_plugin_management_sdk::PluginCommandInvocation {
        plugin_id: "plugin-review".to_string(),
        command_id: "unknown".to_string(),
        arguments: None,
    };
    assert!(validate_plugin_command_invocations(&selected, &[unknown]).is_err());
    let oversized = chatos_plugin_management_sdk::PluginCommandInvocation {
        plugin_id: "plugin-review".to_string(),
        command_id: "review".to_string(),
        arguments: Some("x".repeat(16 * 1024 + 1)),
    };
    assert!(validate_plugin_command_invocations(&selected, &[oversized]).is_err());
}

#[test]
fn cloud_sandbox_route_pins_all_workspace_tools_to_the_same_lease() {
    let mut routes = vec![
        system_route(SystemMcpKey::CodeMaintainerRead),
        system_route(SystemMcpKey::CodeMaintainerWrite),
        system_route(SystemMcpKey::TerminalController),
    ];
    let target = SandboxExecutionTarget {
        provider: SandboxProviderKind::Cloud,
        pairing_id: None,
        sandbox_id: "sandbox-1".to_string(),
        lease_id: "lease-1".to_string(),
        is_environment: false,
        service_id: None,
    };
    let route = RuntimeWorkspaceRouteTarget::CloudSandbox { target };

    bind_runtime_workspace_routes(routes.as_mut_slice(), Some(&route));

    assert!(routes.iter().all(|resolved| {
        resolved.provider_kind == McpProviderKind::CloudSandbox
            && resolved.provider_ref.as_deref() == Some("sandbox:sandbox-1/lease:lease-1")
    }));
}

#[test]
fn local_connector_route_pins_all_workspace_tools_without_exposing_runtime_mode() {
    let mut routes = vec![
        system_route(SystemMcpKey::CodeMaintainerRead),
        system_route(SystemMcpKey::CodeMaintainerWrite),
        system_route(SystemMcpKey::TerminalController),
    ];
    bind_runtime_workspace_routes(
        routes.as_mut_slice(),
        Some(&RuntimeWorkspaceRouteTarget::LocalConnector),
    );

    for route in &routes {
        assert_eq!(route.provider_kind, McpProviderKind::LocalConnector);
    }
}

#[test]
fn harness_route_binds_read_write_and_rejects_terminal() {
    let mut routes = vec![
        system_route(SystemMcpKey::CodeMaintainerRead),
        system_route(SystemMcpKey::CodeMaintainerWrite),
        system_route(SystemMcpKey::TerminalController),
    ];
    let route = RuntimeWorkspaceRouteTarget::Harness {
        branch: HarnessBranchTarget::Run {
            branch_id: "branch-1".to_string(),
            branch_ref: "chatos/runs/run-1".to_string(),
            base_branch: "main".to_string(),
            base_commit: "base-commit".to_string(),
        },
    };

    bind_runtime_workspace_routes(routes.as_mut_slice(), Some(&route));

    assert_eq!(routes[0].provider_kind, McpProviderKind::Harness);
    assert_eq!(routes[1].provider_kind, McpProviderKind::Harness);
    assert_eq!(routes[2].provider_kind, McpProviderKind::Unavailable);
    assert!(routes[2]
        .reason
        .contains("requires a Task Runner cloud sandbox"));
}

#[test]
fn harness_default_branch_is_read_only() {
    let mut routes = vec![
        system_route(SystemMcpKey::CodeMaintainerRead),
        system_route(SystemMcpKey::CodeMaintainerWrite),
    ];
    let route = RuntimeWorkspaceRouteTarget::Harness {
        branch: HarnessBranchTarget::Default {
            branch_ref: "main".to_string(),
        },
    };

    bind_runtime_workspace_routes(routes.as_mut_slice(), Some(&route));

    assert_eq!(routes[0].provider_kind, McpProviderKind::Harness);
    assert_eq!(routes[1].provider_kind, McpProviderKind::Unavailable);
    assert!(routes[1].reason.contains("Task Run branch"));
}

#[test]
fn cloud_sandbox_route_rejects_local_connector_targets_during_normalization() {
    let route = RuntimeWorkspaceRouteTarget::CloudSandbox {
        target: SandboxExecutionTarget {
            provider: SandboxProviderKind::LocalConnector,
            pairing_id: Some("pairing-1".to_string()),
            sandbox_id: "sandbox-1".to_string(),
            lease_id: "lease-1".to_string(),
            is_environment: false,
            service_id: None,
        },
    };

    assert!(normalize_runtime_workspace_route(Some(route)).is_err());
}

#[test]
fn cloud_sandbox_images_are_bound_without_a_runtime_sandbox_target() {
    let mut routes = vec![sandbox_images_route(McpProviderKind::CloudSandbox)];
    let mut context = context();
    context.sandbox_provider = SandboxProviderKind::Cloud;

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
fn workspace_route_binding_does_not_overwrite_sandbox_images() {
    let mut routes = vec![sandbox_images_route(McpProviderKind::CloudSandbox)];
    routes[0].provider_ref =
        Some(crate::providers::sandbox_images_cloud_provider_ref().to_string());
    bind_runtime_workspace_routes(
        routes.as_mut_slice(),
        Some(&RuntimeWorkspaceRouteTarget::LocalConnector),
    );

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
