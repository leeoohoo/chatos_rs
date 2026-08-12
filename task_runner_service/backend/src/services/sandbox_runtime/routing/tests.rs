// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::services::project_management_api_client::{
    ProjectRuntimeEnvironmentMcpPolicy, ProjectRuntimeEnvironmentSettings,
};

fn image(
    service_role: &str,
    attachment: &str,
    filesystem: bool,
    terminal: bool,
) -> ProjectRuntimeEnvironmentImage {
    ProjectRuntimeEnvironmentImage {
        environment_key: "services/api".to_string(),
        service_id: "services-api".to_string(),
        display_name: "API".to_string(),
        service_role: service_role.to_string(),
        mcp_policy: ProjectRuntimeEnvironmentMcpPolicy {
            managed_by: "system".to_string(),
            attachment: attachment.to_string(),
            filesystem,
            terminal,
        },
        image_id: Some("image-1".to_string()),
        image_ref: None,
        image_provider: "cloud_sandbox_manager".to_string(),
        status: "ready".to_string(),
        dockerfile: Some("FROM alpine\n".to_string()),
        env_vars: serde_json::json!({}),
    }
}

#[test]
fn only_system_managed_workspace_targets_are_routable() {
    assert!(runtime_image_is_program_managed_target(&image(
        "workspace",
        "workspace_gateway_target",
        true,
        true,
    )));
    assert!(!runtime_image_is_program_managed_target(&image(
        "application",
        "project_gateway_target",
        true,
        true,
    )));
    assert!(!runtime_image_is_program_managed_target(&image(
        "application",
        "none",
        false,
        false,
    )));
}

#[test]
fn project_workspace_is_used_without_task_level_selection() {
    let mut workspace = image("workspace", "workspace_gateway_target", true, true);
    workspace.environment_key = "workspace".to_string();
    workspace.service_id = "workspace".to_string();
    workspace.display_name = "Project Workspace".to_string();
    workspace.dockerfile = None;
    let mut api = image("application", "none", false, false);
    api.image_id = None;
    let runtime = ProjectSandboxRuntimeSettings {
        environment: ProjectRuntimeEnvironmentSettings {
            sandbox_enabled: true,
            status: "ready".to_string(),
            not_runnable_reason: None,
            execution_service_id: Some("workspace".to_string()),
            env_vars: serde_json::json!({}),
            generated_config_files: Vec::new(),
        },
        images: vec![api, workspace],
    };

    assert_eq!(
        sandbox_image_id_for_runtime(&runtime, "cloud_sandbox_manager", None)
            .expect("project workspace image"),
        Some("image-1".to_string())
    );
}

#[test]
fn environment_plan_contains_only_workspace_and_dependencies() {
    let mut workspace = image("workspace", "workspace_gateway_target", true, true);
    workspace.environment_key = "workspace".to_string();
    workspace.service_id = "workspace".to_string();
    workspace.display_name = "Project Workspace".to_string();
    workspace.dockerfile = None;
    let mut application = image("application", "none", false, false);
    application.image_id = None;
    let mut artifact = image("artifact", "none", false, false);
    artifact.service_id = "web-prototype".to_string();
    artifact.image_id = None;
    let mut dependency = image("dependency", "none", false, false);
    dependency.environment_key = "postgresql".to_string();
    dependency.service_id = "postgresql".to_string();
    dependency.image_id = None;
    dependency.image_ref = Some("postgres:16-alpine".to_string());
    dependency.dockerfile = None;
    let runtime = ProjectSandboxRuntimeSettings {
        environment: ProjectRuntimeEnvironmentSettings {
            sandbox_enabled: true,
            status: "ready".to_string(),
            not_runnable_reason: None,
            execution_service_id: Some("workspace".to_string()),
            env_vars: serde_json::json!({}),
            generated_config_files: Vec::new(),
        },
        images: vec![workspace, application, artifact, dependency],
    };
    let mut task = task();
    task.mcp_config.requires_execution = true;

    let plan = sandbox_environment_plan_for_task(&task, &runtime, "cloud_sandbox_manager")
        .expect("workspace plan")
        .expect("environment topology");
    assert_eq!(plan.execution_service_id, "workspace");
    assert_eq!(
        plan.services
            .iter()
            .map(|service| service.service_id.as_str())
            .collect::<Vec<_>>(),
        vec!["workspace", "postgresql"]
    );
}

#[test]
fn runtime_topology_v2_feature_flag_defaults_on_and_can_fail_back() {
    assert!(runtime_topology_v2_enabled_from_value(None));
    assert!(runtime_topology_v2_enabled_from_value(Some("true")));
    assert!(!runtime_topology_v2_enabled_from_value(Some("false")));
    assert!(!runtime_topology_v2_enabled_from_value(Some("0")));
}

#[test]
fn project_environment_never_falls_back_to_a_base_sandbox() {
    let mut task = task();
    task.mcp_config.requires_execution = true;
    let mut route = SandboxTaskRoute {
        base_url: "http://sandbox.example".to_string(),
        auth: None,
        image_id: Some("dev-node24".to_string()),
        environment_plan: Some(SandboxEnvironmentPlan {
            execution_service_id: "workspace".to_string(),
            services: Vec::new(),
            generated_config_files: Vec::new(),
        }),
        provider: "cloud_sandbox_manager".to_string(),
        local_connector_pairing_id: None,
        policy: task.mcp_config.sandbox_policy_request(),
    };

    assert!(!sandbox_environment_fallback_allowed(&task, &route));

    task.mcp_config.execution_service_id = Some("workspace".to_string());
    assert!(!sandbox_environment_fallback_allowed(&task, &route));

    task.mcp_config.execution_service_id = Some("api".to_string());
    assert!(!sandbox_environment_fallback_allowed(&task, &route));

    task.mcp_config.execution_service_id = Some("workspace".to_string());
    route.provider = "local_connector".to_string();
    assert!(!sandbox_environment_fallback_allowed(&task, &route));
}

#[test]
fn local_project_workspace_identity_comes_only_from_the_managed_logical_root() {
    let mut project = project();
    project.source_type = Some("local_connector".to_string());
    project.root_path = Some("local://connector/device-1/workspace-1/apps/backend".to_string());
    let project_ref = local_connector_project_ref(&project)
        .expect("valid logical root")
        .expect("local project");
    assert_eq!(project_ref.device_id, "device-1");
    assert_eq!(project_ref.workspace_id, "workspace-1");
    assert_eq!(project_ref.relative_path.as_deref(), Some("apps/backend"));

    project.root_path = Some("/Users/example/private-project".to_string());
    assert!(local_connector_project_ref(&project).is_err());
}

#[test]
fn local_pairing_policy_caps_task_permissions_and_discards_extra_roots() {
    let mut task = task();
    task.mcp_config.sandbox_mode = Some(SandboxBackendKind::LocalProcess);
    task.mcp_config.permission_profile_id = Some(PermissionProfileId::FullAccess);
    task.mcp_config.approval_policy = Some(ApprovalPolicy::OnRequest);
    task.mcp_config.approval_reviewer = Some(ApprovalReviewer::AutoReview);
    task.mcp_config.additional_writable_roots = vec!["/tmp/escape".to_string()];
    let pairing = LocalConnectorSandboxPairing {
        id: "pairing-1".to_string(),
        device_id: "device-1".to_string(),
        workspace_id: "workspace-1".to_string(),
        enabled: true,
        sandbox_mode: "docker".to_string(),
        sandbox_readiness: "ready".to_string(),
        permission_profile_id: "workspace_write".to_string(),
        approval_policy: "on_request".to_string(),
        approval_reviewer: "user".to_string(),
        policy_revision: Some("pairing-revision-1".to_string()),
    };

    let effective =
        local_connector_policy_for_pairing(&task, &pairing).expect("pairing policy must resolve");

    assert_eq!(effective.sandbox_mode, Some(SandboxBackendKind::Docker));
    assert_eq!(
        effective.permission_profile_id,
        Some(PermissionProfileId::WorkspaceWrite)
    );
    assert_eq!(effective.approval_policy, Some(ApprovalPolicy::OnRequest));
    assert_eq!(effective.approval_reviewer, Some(ApprovalReviewer::User));
    assert_eq!(
        effective.policy_revision.as_deref(),
        Some("pairing-revision-1")
    );
    assert!(effective.additional_writable_roots.is_empty());
}

#[test]
fn local_process_execution_does_not_require_a_workspace_image() {
    let runtime = ProjectSandboxRuntimeSettings {
        environment: ProjectRuntimeEnvironmentSettings {
            sandbox_enabled: true,
            status: "pending".to_string(),
            not_runnable_reason: None,
            execution_service_id: None,
            env_vars: serde_json::json!({}),
            generated_config_files: Vec::new(),
        },
        images: Vec::new(),
    };
    let mut task = task();
    task.mcp_config.requires_execution = true;
    let local_process_image =
        local_sandbox_image_id_for_task(&task, &runtime, Some(SandboxBackendKind::LocalProcess))
            .expect("native local execution must not require a Docker image");
    let docker_error =
        local_sandbox_image_id_for_task(&task, &runtime, Some(SandboxBackendKind::Docker))
            .expect_err("Docker execution must still require a managed workspace image");

    assert_eq!(local_process_image, None);
    assert!(docker_error.contains("no ready program-managed local workspace image"));
}

#[test]
fn execution_without_a_ready_project_workspace_is_rejected() {
    let runtime = ProjectSandboxRuntimeSettings {
        environment: ProjectRuntimeEnvironmentSettings {
            sandbox_enabled: true,
            status: "pending".to_string(),
            not_runnable_reason: None,
            execution_service_id: None,
            env_vars: serde_json::json!({}),
            generated_config_files: Vec::new(),
        },
        images: Vec::new(),
    };
    let mut task = task();
    task.mcp_config.requires_execution = true;

    let plan_error = sandbox_environment_plan_for_task(&task, &runtime, "cloud_sandbox_manager")
        .expect_err("pending project runtime must block execution");
    assert!(plan_error.contains("status=pending"));
    let image_error =
        sandbox_image_id_for_task(&task, &runtime, "cloud_sandbox_manager", "dev-java21")
            .expect_err("pending project runtime must not use a base image");
    assert!(image_error.contains("status=pending"));
}

#[test]
fn execution_with_not_runnable_reason_is_rejected_even_if_status_is_ready() {
    let runtime = ProjectSandboxRuntimeSettings {
        environment: ProjectRuntimeEnvironmentSettings {
            sandbox_enabled: true,
            status: "ready".to_string(),
            not_runnable_reason: Some("项目内容尚未生成".to_string()),
            execution_service_id: Some("workspace".to_string()),
            env_vars: serde_json::json!({}),
            generated_config_files: Vec::new(),
        },
        images: vec![image("workspace", "workspace_gateway_target", true, true)],
    };
    let mut task = task();
    task.mcp_config.requires_execution = true;

    let error = sandbox_image_id_for_task(&task, &runtime, "cloud_sandbox_manager", "dev-java21")
        .expect_err("not runnable project must block execution");
    assert!(error.contains("not runnable"));
    assert!(error.contains("项目内容尚未生成"));
}

#[test]
fn file_only_task_can_use_the_program_base_image() {
    let runtime = ProjectSandboxRuntimeSettings {
        environment: ProjectRuntimeEnvironmentSettings {
            sandbox_enabled: true,
            status: "pending".to_string(),
            not_runnable_reason: None,
            execution_service_id: None,
            env_vars: serde_json::json!({}),
            generated_config_files: Vec::new(),
        },
        images: Vec::new(),
    };
    let mut task = task();
    task.mcp_config.requires_execution = false;

    assert!(
        sandbox_environment_plan_for_task(&task, &runtime, "cloud_sandbox_manager")
            .expect("file-only task route")
            .is_none()
    );
    assert_eq!(
        sandbox_image_id_for_task(&task, &runtime, "cloud_sandbox_manager", "dev-java21")
            .expect("program base image"),
        Some("dev-java21".to_string())
    );
}

#[test]
fn explicit_business_service_execution_is_rejected() {
    let runtime = ProjectSandboxRuntimeSettings {
        environment: ProjectRuntimeEnvironmentSettings {
            sandbox_enabled: true,
            status: "ready".to_string(),
            not_runnable_reason: None,
            execution_service_id: None,
            env_vars: serde_json::json!({}),
            generated_config_files: Vec::new(),
        },
        images: Vec::new(),
    };
    let mut task = task();
    task.mcp_config.requires_execution = true;
    task.mcp_config.execution_service_id = Some("services-api".to_string());

    let error = sandbox_environment_plan_for_task(&task, &runtime, "cloud_sandbox_manager")
        .expect_err("business service must not become execution target");
    assert!(error.contains("services-api"));
    assert!(error.contains("not the ready project workspace"));
}

#[test]
fn dependency_services_do_not_serialize_an_mcp_policy() {
    let mut workspace = image("workspace", "workspace_gateway_target", true, true);
    workspace.environment_key = "workspace".to_string();
    workspace.service_id = "workspace".to_string();
    workspace.display_name = "Project Workspace".to_string();
    workspace.dockerfile = None;
    let mut dependency = image("dependency", "none", false, false);
    dependency.environment_key = "postgresql".to_string();
    dependency.service_id = "postgresql".to_string();
    dependency.display_name = "PostgreSQL".to_string();
    dependency.image_id = None;
    dependency.image_ref = Some("postgres:16-alpine".to_string());
    dependency.dockerfile = None;
    let runtime = ProjectSandboxRuntimeSettings {
        environment: ProjectRuntimeEnvironmentSettings {
            sandbox_enabled: true,
            status: "ready".to_string(),
            not_runnable_reason: None,
            execution_service_id: Some("workspace".to_string()),
            env_vars: serde_json::json!({}),
            generated_config_files: Vec::new(),
        },
        images: vec![workspace, dependency],
    };
    let mut task = task();
    task.mcp_config.requires_execution = true;
    task.mcp_config.execution_service_id = Some("workspace".to_string());

    let plan = sandbox_environment_plan_for_task(&task, &runtime, "cloud_sandbox_manager")
        .expect("environment plan")
        .expect("environment topology");
    let services = serde_json::to_value(plan.services).expect("serialize services");
    let dependency = services
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.get("service_role").and_then(Value::as_str) == Some("dependency"))
        })
        .expect("dependency service");
    assert!(dependency.get("mcp_policy").is_none());
}

#[test]
fn service_environment_templates_resolve_from_project_values() {
    let global = json_object_to_string_map(&serde_json::json!({
        "POSTGRES_DB": "mdm_service",
        "POSTGRES_USER": "mdm_service",
        "POSTGRES_PASSWORD": "generated-secret",
    }));
    let environment = merged_environment(
        &global,
        &serde_json::json!({
            "POSTGRES_DB": "${POSTGRES_DB:-app}",
            "POSTGRES_USER": "${POSTGRES_USER}",
            "POSTGRES_PASSWORD": "${POSTGRES_PASSWORD}",
            "DATABASE_URL": "postgresql://${POSTGRES_USER}:${POSTGRES_PASSWORD}@postgresql:5432/${POSTGRES_DB}",
            "UNRESOLVED": "${MISSING_VALUE}",
        }),
    );

    assert_eq!(
        environment.get("POSTGRES_DB").map(String::as_str),
        Some("mdm_service")
    );
    assert_eq!(
        environment.get("POSTGRES_USER").map(String::as_str),
        Some("mdm_service")
    );
    assert_eq!(
        environment.get("POSTGRES_PASSWORD").map(String::as_str),
        Some("generated-secret")
    );
    assert_eq!(
        environment.get("DATABASE_URL").map(String::as_str),
        Some("postgresql://mdm_service:generated-secret@postgresql:5432/mdm_service")
    );
    assert_eq!(
        environment.get("UNRESOLVED").map(String::as_str),
        Some("${MISSING_VALUE}")
    );
}

#[test]
fn global_environment_templates_resolve_from_project_values() {
    let global = json_object_to_string_map(&serde_json::json!({
        "POSTGRES_DB": "starabyss_online",
        "POSTGRES_USER": "starabyss",
        "POSTGRES_PASSWORD": "generated-project-secret",
        "DATABASE_URL": "postgresql://${POSTGRES_USER}:${POSTGRES_PASSWORD}@postgresql:5432/${POSTGRES_DB}",
    }));

    let environment = merged_environment(&global, &serde_json::json!({}));

    assert_eq!(
        environment.get("DATABASE_URL").map(String::as_str),
        Some("postgresql://starabyss:generated-project-secret@postgresql:5432/starabyss_online")
    );
}

fn task() -> TaskRecord {
    TaskRecord {
        id: "task-1".to_string(),
        title: "Task".to_string(),
        description: None,
        objective: "Test sandbox routing".to_string(),
        input_payload: None,
        status: crate::models::TaskStatus::Ready,
        priority: 0,
        tags: Vec::new(),
        default_model_config_id: None,
        memory_thread_id: "task-task-1".to_string(),
        tenant_id: "tenant-1".to_string(),
        subject_id: "user-1".to_string(),
        project_id: "project-1".to_string(),
        task_profile: crate::models::default_task_profile(),
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
        plugin_config: Default::default(),
        mcp_config: crate::models::TaskMcpConfig::default(),
        created_at: "2026-07-19T00:00:00Z".to_string(),
        updated_at: "2026-07-19T00:00:00Z".to_string(),
        deleted_at: None,
    }
}

fn project() -> TaskProjectRecord {
    TaskProjectRecord {
        id: "project-1".to_string(),
        owner_user_id: Some("user-1".to_string()),
        owner_username: None,
        owner_display_name: None,
        name: "Project".to_string(),
        root_path: None,
        git_url: None,
        source_type: Some("cloud".to_string()),
        cloud_import_source: None,
        import_status: None,
        source_git_url: None,
        harness_space_identifier: None,
        harness_repo_identifier: None,
        harness_repo_path: None,
        harness_git_url: None,
        harness_git_ssh_url: None,
        harness_default_branch: None,
        harness_provision_status: None,
        harness_provision_error: None,
        harness_provisioned_at: None,
        description: None,
        status: crate::models::TaskProjectStatus::Active,
        created_at: "2026-07-19T00:00:00Z".to_string(),
        updated_at: "2026-07-19T00:00:00Z".to_string(),
        archived_at: None,
    }
}
