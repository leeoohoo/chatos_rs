// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(test)]
mod tests {
    use super::*;

    fn variable(
        project_value: Option<&str>,
        project_value_suitable: bool,
        recommended_value: Option<&str>,
        user_value: Option<&str>,
    ) -> ProjectRuntimeEnvironmentVariableRecord {
        ProjectRuntimeEnvironmentVariableRecord {
            name: "SERVICE_HOST".to_string(),
            project_value: project_value.map(ToOwned::to_owned),
            project_value_suitable,
            recommended_value: recommended_value.map(ToOwned::to_owned),
            user_value: user_value.map(ToOwned::to_owned),
            effective_value: None,
            effective_source: RuntimeEnvironmentVariableSource::None,
            description: None,
            recommendation_reason: None,
            required: true,
            secret: false,
        }
    }

    fn runtime_image(
        environment_key: &str,
        environment_type: &str,
        dockerfile: Option<&str>,
        image_ref: Option<&str>,
    ) -> ProjectRuntimeEnvironmentImageRecord {
        ProjectRuntimeEnvironmentImageRecord {
            id: format!("image-{environment_key}"),
            project_id: "project-1".to_string(),
            environment_key: environment_key.to_string(),
            environment_type: environment_type.to_string(),
            display_name: environment_key.to_string(),
            service_id: String::new(),
            service_role: RuntimeServiceRole::Unknown,
            source_root: ".".to_string(),
            component_kind: environment_type.to_string(),
            startup_command: None,
            test_command: None,
            depends_on: Vec::new(),
            auto_start: false,
            mcp_policy: ProgramManagedMcpPolicy::default(),
            image_id: None,
            image_ref: image_ref.map(ToOwned::to_owned),
            image_provider: RuntimeEnvironmentProvider::CloudSandboxManager,
            features: empty_array(),
            ports: empty_array(),
            env_vars: empty_object(),
            dockerfile: dockerfile.map(ToOwned::to_owned),
            custom_build_script: None,
            status: "planned".to_string(),
            error: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }

    fn project(source_type: ProjectSourceType, root_path: Option<&str>) -> ProjectRecord {
        ProjectRecord {
            id: "project-1".to_string(),
            creator_user_id: None,
            creator_username: None,
            creator_display_name: None,
            owner_user_id: Some("user-1".to_string()),
            owner_username: None,
            owner_display_name: None,
            name: "Project".to_string(),
            root_path: root_path.map(ToOwned::to_owned),
            git_url: None,
            source_type,
            execution_plane: ProjectExecutionPlane::Cloud,
            cloud_import_source: CloudImportSource::Empty,
            import_status: ProjectImportStatus::Ready,
            source_git_url: None,
            harness_space_identifier: Some("space-1".to_string()),
            harness_repo_identifier: Some("repo-1".to_string()),
            harness_repo_path: None,
            harness_git_url: None,
            harness_git_ssh_url: None,
            harness_default_branch: Some("main".to_string()),
            harness_provision_status: None,
            harness_provision_error: None,
            harness_provisioned_at: None,
            import_error: None,
            import_started_at: None,
            import_finished_at: None,
            description: None,
            status: ProjectStatus::Active,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
            archived_at: None,
        }
    }

    #[test]
    fn program_policy_allows_only_workspace_as_execution_target() {
        let mut application = runtime_image("api", "application", Some("FROM node:24\n"), None);
        assert!(apply_program_managed_image_policy(&mut application));
        assert_eq!(application.service_role, RuntimeServiceRole::Application);
        assert_eq!(application.mcp_policy, ProgramManagedMcpPolicy::default());
        assert_eq!(application.service_id, "api");

        let mut workspace = runtime_image("workspace", "workspace", None, None);
        assert!(apply_program_managed_image_policy(&mut workspace));
        assert_eq!(workspace.service_role, RuntimeServiceRole::Workspace);
        assert_eq!(
            workspace.mcp_policy,
            ProgramManagedMcpPolicy::workspace_target()
        );

        let mut redis = runtime_image(
            "redis",
            "service",
            Some("FROM redis:7-alpine\n"),
            Some("redis:7-alpine"),
        );
        assert!(apply_program_managed_image_policy(&mut redis));
        assert_eq!(redis.service_role, RuntimeServiceRole::Dependency);
        assert_eq!(redis.mcp_policy, ProgramManagedMcpPolicy::default());

        let mut unverified = runtime_image("api", "application", None, None);
        assert!(apply_program_managed_image_policy(&mut unverified));
        assert_eq!(unverified.service_id, "api");
        assert_eq!(unverified.service_role, RuntimeServiceRole::Unknown);
        assert_eq!(unverified.mcp_policy, ProgramManagedMcpPolicy::default());
    }

    #[test]
    fn empty_project_workspace_uses_a_runnable_build_and_test_baseline() {
        assert_eq!(
            workspace_runtime_features(&[], &empty_object()),
            vec!["node@24".to_string(), "python@3.11".to_string()]
        );
    }

    #[test]
    fn explanatory_scan_summary_cannot_invent_a_runtime() {
        assert_eq!(
            workspace_runtime_features(
                &[],
                &serde_json::json!({
                    "workspace_empty": true,
                    "runtimes": [],
                    "environment_variable_scan": {
                        "summary": "未发现 Spring、Cargo.toml 或 Node.js 配置"
                    }
                }),
            ),
            vec!["node@24".to_string(), "python@3.11".to_string()]
        );
    }

    #[test]
    fn workspace_runtime_features_ignore_stale_workspace_selections() {
        let mut backend = runtime_image(
            "backend",
            "application",
            Some("FROM rust:1.85-bookworm\n"),
            None,
        );
        apply_program_managed_image_policy(&mut backend);
        backend.features = serde_json::json!(["rust@1.85", "cargo"]);

        let mut workspace = runtime_image("workspace", "workspace", None, None);
        apply_program_managed_image_policy(&mut workspace);
        workspace.features = serde_json::json!(["rust@1.85", "go@1.26"]);

        assert_eq!(
            workspace_runtime_features(
                &[backend, workspace],
                &serde_json::json!({"analysis_requirement": "run cargo build"}),
            ),
            vec!["rust@1.85".to_string()]
        );
    }

    #[test]
    fn standalone_go_build_requirement_selects_go_runtime() {
        assert_eq!(
            workspace_runtime_features(
                &[],
                &serde_json::json!({"analysis_requirement": "run go build ./..."}),
            ),
            vec!["go".to_string()]
        );
    }

    #[test]
    fn project_boundary_creates_one_workspace_and_keeps_applications_peer_equal() {
        let mut environment = ProjectRuntimeEnvironmentRecord {
            project_id: "project-1".to_string(),
            status: ProjectRuntimeEnvironmentStatus::Ready,
            sandbox_enabled: true,
            sandbox_provider: RuntimeEnvironmentProvider::CloudSandboxManager,
            file_provider: RuntimeEnvironmentProvider::Harness,
            analysis_summary: None,
            not_runnable_reason: None,
            execution_service_id: None,
            detected_stack: empty_object(),
            required_services: empty_array(),
            env_vars: empty_object(),
            environment_variables: Vec::new(),
            generated_config_files: Vec::new(),
            last_agent_run_id: None,
            last_error: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        };
        let mdm = runtime_image(
            "mdm-service",
            "application",
            Some("FROM python:3.14"),
            None,
        );
        let prototype = runtime_image(
            "web-prototype",
            "application",
            Some("FROM nginx:alpine\nCOPY . /usr/share/nginx/html/"),
            None,
        );
        let mut images = vec![prototype, mdm];
        let project = project(ProjectSourceType::Cloud, None);

        assert!(enforce_project_runtime_boundary(
            &project,
            &mut environment,
            &mut images,
        ));
        assert_eq!(
            environment.execution_service_id.as_deref(),
            Some("workspace")
        );
        assert_eq!(
            images
                .iter()
                .find(|image| image.service_id == "mdm-service")
                .expect("mdm service")
                .mcp_policy,
            ProgramManagedMcpPolicy::default(),
        );
        assert_eq!(
            images
                .iter()
                .find(|image| image.service_id == "web-prototype")
                .expect("prototype")
                .service_role,
            RuntimeServiceRole::Artifact,
        );
        assert_eq!(
            images
                .iter()
                .find(|image| image.service_role == RuntimeServiceRole::Workspace)
                .expect("workspace")
                .mcp_policy,
            ProgramManagedMcpPolicy::workspace_target(),
        );
    }

    #[test]
    fn project_boundary_repairs_ready_environment_with_not_runnable_reason() {
        let mut environment = ProjectRuntimeEnvironmentRecord {
            project_id: "project-1".to_string(),
            status: ProjectRuntimeEnvironmentStatus::Ready,
            sandbox_enabled: true,
            sandbox_provider: RuntimeEnvironmentProvider::CloudSandboxManager,
            file_provider: RuntimeEnvironmentProvider::Harness,
            analysis_summary: Some("运行环境已就绪".to_string()),
            not_runnable_reason: Some("项目内容尚未生成".to_string()),
            execution_service_id: Some("workspace".to_string()),
            detected_stack: empty_object(),
            required_services: empty_array(),
            env_vars: empty_object(),
            environment_variables: Vec::new(),
            generated_config_files: Vec::new(),
            last_agent_run_id: None,
            last_error: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        };
        let mut workspace = runtime_image("workspace", "workspace", None, None);
        apply_program_managed_image_policy(&mut workspace);
        workspace.image_id = Some("dev-node24".to_string());
        workspace.status = "ready".to_string();
        let mut images = vec![workspace];

        assert!(enforce_project_runtime_boundary(
            &project(ProjectSourceType::Cloud, None),
            &mut environment,
            &mut images,
        ));
        assert_eq!(
            environment.status,
            ProjectRuntimeEnvironmentStatus::NotRunnable
        );
        assert_eq!(
            environment.analysis_summary.as_deref(),
            Some("项目内容尚未生成")
        );
        assert!(environment.execution_service_id.is_none());
        assert!(images.is_empty());
    }

    #[test]
    fn effective_value_follows_user_project_recommendation_precedence() {
        let mut record = variable(Some("project-host"), true, Some("sandbox-host"), None);
        refresh_environment_variable_record(&mut record);
        assert_eq!(record.effective_value.as_deref(), Some("project-host"));
        assert_eq!(
            record.effective_source,
            RuntimeEnvironmentVariableSource::Project
        );

        record.user_value = Some("user-host".to_string());
        refresh_environment_variable_record(&mut record);
        assert_eq!(record.effective_value.as_deref(), Some("user-host"));
        assert_eq!(
            record.effective_source,
            RuntimeEnvironmentVariableSource::User
        );
    }

    #[test]
    fn unsuitable_project_value_uses_ai_recommendation() {
        let mut record = variable(Some("127.0.0.1"), false, Some("redis"), None);
        refresh_environment_variable_record(&mut record);
        assert_eq!(record.effective_value.as_deref(), Some("redis"));
        assert_eq!(
            record.effective_source,
            RuntimeEnvironmentVariableSource::AiRecommended
        );
    }

    #[test]
    fn replacing_user_overrides_preserves_detected_sources() {
        let mut environment = ProjectRuntimeEnvironmentRecord {
            project_id: "project-1".to_string(),
            status: ProjectRuntimeEnvironmentStatus::PendingConfiguration,
            sandbox_enabled: true,
            sandbox_provider: RuntimeEnvironmentProvider::LocalConnector,
            file_provider: RuntimeEnvironmentProvider::LocalConnector,
            analysis_summary: None,
            not_runnable_reason: None,
            execution_service_id: None,
            detected_stack: empty_object(),
            required_services: empty_array(),
            env_vars: empty_object(),
            environment_variables: vec![variable(Some("127.0.0.1"), false, Some("redis"), None)],
            generated_config_files: Vec::new(),
            last_agent_run_id: None,
            last_error: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        };
        apply_environment_variable_overrides(
            &mut environment,
            vec![ProjectRuntimeEnvironmentVariableOverride {
                name: "service_host".to_string(),
                value: "custom-host".to_string(),
            }],
        )
        .expect("override");
        let record = &environment.environment_variables[0];
        assert_eq!(record.project_value.as_deref(), Some("127.0.0.1"));
        assert_eq!(record.recommended_value.as_deref(), Some("redis"));
        assert_eq!(record.user_value.as_deref(), Some("custom-host"));
        assert_eq!(environment.env_vars["SERVICE_HOST"], "custom-host");
    }

    #[test]
    fn legacy_routing_summary_is_replaced_with_program_generated_technical_summary() {
        let mut environment = ProjectRuntimeEnvironmentRecord {
            project_id: "project-1".to_string(),
            status: ProjectRuntimeEnvironmentStatus::PendingImageBuild,
            sandbox_enabled: true,
            sandbox_provider: RuntimeEnvironmentProvider::CloudSandboxManager,
            file_provider: RuntimeEnvironmentProvider::Harness,
            analysis_summary: Some(
                "云端项目只通过 Harness MCP 读取文件，并只使用云端 Sandbox Manager。".to_string(),
            ),
            not_runnable_reason: None,
            execution_service_id: None,
            detected_stack: empty_object(),
            required_services: empty_array(),
            env_vars: empty_object(),
            environment_variables: Vec::new(),
            generated_config_files: Vec::new(),
            last_agent_run_id: None,
            last_error: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        };
        let mut application = runtime_image("api", "application", Some("FROM node:24"), None);
        apply_program_managed_image_policy(&mut application);
        let mut dependency = runtime_image("redis", "service", None, Some("redis:7-alpine"));
        apply_program_managed_image_policy(&mut dependency);

        assert!(replace_legacy_internal_routing_summary(
            &mut environment,
            &[application, dependency],
        ));
        let summary = environment.analysis_summary.expect("technical summary");
        assert!(summary.contains("1 个平等应用组件"));
        assert!(summary.contains("1 个依赖服务"));
        assert!(!summary.contains("Harness"));
        assert!(!summary.contains("Sandbox Manager"));
    }

    #[test]
    fn legacy_application_image_summary_is_replaced_with_workspace_summary() {
        let mut environment = ProjectRuntimeEnvironmentRecord {
            project_id: "project-1".to_string(),
            status: ProjectRuntimeEnvironmentStatus::PendingImageBuild,
            sandbox_enabled: true,
            sandbox_provider: RuntimeEnvironmentProvider::CloudSandboxManager,
            file_provider: RuntimeEnvironmentProvider::Harness,
            analysis_summary: Some(
                "已识别 2 个应用组件和 1 个依赖服务，生成 3 个环境配置文件及项目级 Compose 计划，等待生成应用镜像。"
                    .to_string(),
            ),
            not_runnable_reason: None,
            execution_service_id: Some("workspace".to_string()),
            detected_stack: empty_object(),
            required_services: empty_array(),
            env_vars: empty_object(),
            environment_variables: Vec::new(),
            generated_config_files: Vec::new(),
            last_agent_run_id: None,
            last_error: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        };
        let mut application = runtime_image("api", "application", Some("FROM node:24"), None);
        apply_program_managed_image_policy(&mut application);
        let mut workspace = runtime_image("workspace", "workspace", None, None);
        apply_program_managed_image_policy(&mut workspace);

        assert!(replace_legacy_internal_routing_summary(
            &mut environment,
            &[application, workspace],
        ));
        let summary = environment.analysis_summary.expect("workspace summary");
        assert!(summary.contains("1 个平等应用组件"));
        assert!(summary.contains("等待生成工作区执行镜像"));
        assert!(!summary.contains("等待生成应用镜像"));
    }

    #[test]
    fn cloud_boundary_resets_local_application_images_to_cloud_build_plans() {
        let mut environment = ProjectRuntimeEnvironmentRecord {
            project_id: "project-1".to_string(),
            status: ProjectRuntimeEnvironmentStatus::PendingConfiguration,
            sandbox_enabled: true,
            sandbox_provider: RuntimeEnvironmentProvider::CloudSandboxManager,
            file_provider: RuntimeEnvironmentProvider::Harness,
            analysis_summary: None,
            not_runnable_reason: None,
            execution_service_id: None,
            detected_stack: empty_object(),
            required_services: empty_array(),
            env_vars: empty_object(),
            environment_variables: Vec::new(),
            generated_config_files: Vec::new(),
            last_agent_run_id: None,
            last_error: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        };
        let mut images = vec![ProjectRuntimeEnvironmentImageRecord {
            id: "image-1".to_string(),
            project_id: "project-1".to_string(),
            environment_key: "application_runtime".to_string(),
            environment_type: "runtime".to_string(),
            display_name: "Application runtime".to_string(),
            service_id: String::new(),
            service_role: RuntimeServiceRole::Unknown,
            source_root: ".".to_string(),
            component_kind: "application".to_string(),
            startup_command: None,
            test_command: None,
            depends_on: Vec::new(),
            auto_start: false,
            mcp_policy: ProgramManagedMcpPolicy::default(),
            image_id: Some("local-image".to_string()),
            image_ref: Some("local/runtime:latest".to_string()),
            image_provider: RuntimeEnvironmentProvider::LocalConnector,
            features: serde_json::json!(["node-24"]),
            ports: empty_array(),
            env_vars: empty_object(),
            dockerfile: Some("FROM node:24".to_string()),
            custom_build_script: None,
            status: "ready".to_string(),
            error: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }];
        let project = project(ProjectSourceType::Cloud, None);

        assert!(enforce_project_runtime_boundary(
            &project,
            &mut environment,
            &mut images,
        ));
        let application = images
            .iter()
            .find(|image| image.service_role == RuntimeServiceRole::Application)
            .expect("application");
        assert_eq!(
            application.image_provider,
            RuntimeEnvironmentProvider::CloudSandboxManager
        );
        assert_eq!(application.status, "planned");
        assert!(application.image_id.is_none());
        assert!(application.image_ref.is_none());
        assert!(images
            .iter()
            .any(|image| image.service_role == RuntimeServiceRole::Workspace));
        assert_eq!(
            environment.status,
            ProjectRuntimeEnvironmentStatus::PendingImageBuild
        );
        assert!(environment
            .analysis_summary
            .as_deref()
            .is_some_and(|summary| summary.contains("工作区执行镜像")));
    }

    #[test]
    fn local_project_keeps_local_runtime_providers_after_cloud_orchestration_cutover() {
        let project = project(
            ProjectSourceType::LocalConnector,
            Some("local://connector/device-1/workspace-1/project"),
        );
        let mut environment = ProjectRuntimeEnvironmentRecord {
            project_id: project.id.clone(),
            status: ProjectRuntimeEnvironmentStatus::Ready,
            sandbox_enabled: true,
            sandbox_provider: RuntimeEnvironmentProvider::LocalConnector,
            file_provider: RuntimeEnvironmentProvider::LocalConnector,
            analysis_summary: None,
            not_runnable_reason: None,
            execution_service_id: None,
            detected_stack: empty_object(),
            required_services: empty_array(),
            env_vars: empty_object(),
            environment_variables: Vec::new(),
            generated_config_files: Vec::new(),
            last_agent_run_id: None,
            last_error: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        };
        let mut images = vec![runtime_image(
            "workspace",
            "workspace",
            Some("FROM node:24"),
            None,
        )];
        images[0].image_provider = RuntimeEnvironmentProvider::LocalConnector;

        enforce_project_runtime_boundary(&project, &mut environment, &mut images);

        assert_eq!(
            environment.sandbox_provider,
            RuntimeEnvironmentProvider::LocalConnector
        );
        assert_eq!(
            environment.file_provider,
            RuntimeEnvironmentProvider::LocalConnector
        );
        assert!(images.iter().all(|image| {
            image.image_provider == RuntimeEnvironmentProvider::LocalConnector
        }));
    }

    #[test]
    fn local_project_discards_stale_cloud_sandbox_state_without_fallback() {
        let project = project(
            ProjectSourceType::LocalConnector,
            Some("local://connector/device-1/workspace-1/project"),
        );
        let mut environment = ProjectRuntimeEnvironmentRecord {
            project_id: project.id.clone(),
            status: ProjectRuntimeEnvironmentStatus::Ready,
            sandbox_enabled: true,
            sandbox_provider: RuntimeEnvironmentProvider::CloudSandboxManager,
            file_provider: RuntimeEnvironmentProvider::Harness,
            analysis_summary: None,
            not_runnable_reason: None,
            execution_service_id: Some("workspace".to_string()),
            detected_stack: empty_object(),
            required_services: empty_array(),
            env_vars: empty_object(),
            environment_variables: Vec::new(),
            generated_config_files: Vec::new(),
            last_agent_run_id: None,
            last_error: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        };
        let mut images = vec![runtime_image(
            "workspace",
            "workspace",
            Some("FROM node:24"),
            None,
        )];

        assert!(enforce_project_runtime_boundary(
            &project,
            &mut environment,
            &mut images,
        ));
        assert_eq!(
            environment.sandbox_provider,
            RuntimeEnvironmentProvider::None
        );
        assert_eq!(environment.file_provider, RuntimeEnvironmentProvider::None);
        assert!(environment.execution_service_id.is_none());
        assert!(images.is_empty());
    }

    #[test]
    fn disabled_cloud_sandbox_preserves_harness_file_provider() {
        let project = project(ProjectSourceType::Cloud, None);
        let mut environment = ProjectRuntimeEnvironmentRecord {
            project_id: project.id.clone(),
            status: ProjectRuntimeEnvironmentStatus::Disabled,
            sandbox_enabled: false,
            sandbox_provider: RuntimeEnvironmentProvider::CloudSandboxManager,
            file_provider: RuntimeEnvironmentProvider::None,
            analysis_summary: None,
            not_runnable_reason: None,
            execution_service_id: Some("workspace".to_string()),
            detected_stack: empty_object(),
            required_services: empty_array(),
            env_vars: empty_object(),
            environment_variables: Vec::new(),
            generated_config_files: Vec::new(),
            last_agent_run_id: None,
            last_error: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        };
        let mut images = vec![runtime_image(
            "workspace",
            "workspace",
            Some("FROM node:24"),
            None,
        )];

        assert!(enforce_project_runtime_boundary(
            &project,
            &mut environment,
            &mut images,
        ));
        assert_eq!(environment.sandbox_provider, RuntimeEnvironmentProvider::None);
        assert_eq!(environment.file_provider, RuntimeEnvironmentProvider::Harness);
        assert!(environment.execution_service_id.is_none());
        assert!(images.is_empty());
    }
}
