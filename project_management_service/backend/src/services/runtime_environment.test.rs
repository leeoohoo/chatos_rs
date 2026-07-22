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

    #[test]
    fn program_policy_allows_only_verified_application_targets() {
        let mut application = runtime_image("api", "application", Some("FROM node:24\n"), None);
        assert!(apply_program_managed_image_policy(&mut application));
        assert_eq!(application.service_role, RuntimeServiceRole::Application);
        assert_eq!(
            application.mcp_policy.attachment,
            RuntimeMcpAttachment::ProjectGatewayTarget
        );
        assert!(application.mcp_policy.filesystem);
        assert!(application.mcp_policy.terminal);
        assert_eq!(application.service_id, "api");

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
        assert!(summary.contains("1 个应用组件"));
        assert!(summary.contains("1 个依赖服务"));
        assert!(!summary.contains("Harness"));
        assert!(!summary.contains("Sandbox Manager"));
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

        assert!(enforce_project_runtime_boundary(
            ProjectExecutionPlane::Cloud,
            &mut environment,
            images.as_mut_slice(),
        ));
        assert_eq!(
            images[0].image_provider,
            RuntimeEnvironmentProvider::CloudSandboxManager
        );
        assert_eq!(images[0].status, "planned");
        assert!(images[0].image_id.is_none());
        assert!(images[0].image_ref.is_none());
        assert_eq!(
            environment.status,
            ProjectRuntimeEnvironmentStatus::PendingImageBuild
        );
        assert!(environment
            .analysis_summary
            .as_deref()
            .is_some_and(|summary| summary.contains("Local Connector 镜像记录已作废")));
    }
}
