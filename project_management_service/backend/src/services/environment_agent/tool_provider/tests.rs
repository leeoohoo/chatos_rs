// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::models::ProgramManagedMcpPolicy;

fn planned_image(
    environment_key: &str,
    environment_type: &str,
) -> ProjectRuntimeEnvironmentImageRecord {
    let mut image = ProjectRuntimeEnvironmentImageRecord {
        id: format!("record-{environment_key}"),
        project_id: "project-1".to_string(),
        environment_key: environment_key.to_string(),
        environment_type: environment_type.to_string(),
        display_name: environment_key.to_string(),
        service_id: String::new(),
        service_role: RuntimeServiceRole::Unknown,
        mcp_policy: ProgramManagedMcpPolicy::default(),
        image_id: None,
        image_ref: None,
        image_provider: RuntimeEnvironmentProvider::LocalConnector,
        features: json!(["base"]),
        ports: json!([]),
        env_vars: json!({}),
        dockerfile: Some(format!("FROM ubuntu:24.04\n# {environment_key}\n")),
        custom_build_script: Some("set -e\ntrue\n".to_string()),
        status: "planned".to_string(),
        error: None,
        created_at: "now".to_string(),
        updated_at: "now".to_string(),
    };
    crate::services::runtime_environment::apply_program_managed_image_policy(&mut image);
    image
}

#[test]
fn runtime_environment_cannot_be_saved_before_environment_variable_scan() {
    let missing = require_completed_environment_variable_scan(None).unwrap_err();
    assert!(missing.contains("scan must be completed"));

    let incomplete =
        require_completed_environment_variable_scan(Some(ProjectEnvironmentVariableScanInput {
            completed: false,
            files_scanned: vec![".env.example".to_string()],
            reference_count: 1,
            summary: Some("发现一个变量".to_string()),
        }))
        .unwrap_err();
    assert!(incomplete.contains("scan must be completed"));

    let missing_summary =
        require_completed_environment_variable_scan(Some(ProjectEnvironmentVariableScanInput {
            completed: true,
            files_scanned: Vec::new(),
            reference_count: 0,
            summary: Some(" ".to_string()),
        }))
        .unwrap_err();
    assert!(missing_summary.contains("scan summary is required"));

    assert!(require_completed_environment_variable_scan(Some(
        ProjectEnvironmentVariableScanInput {
            completed: true,
            files_scanned: vec!["src/main.rs".to_string()],
            reference_count: 0,
            summary: Some("已完成全项目扫描，未发现环境变量引用。".to_string()),
        },
    ))
    .is_ok());
}

#[test]
fn generated_config_files_are_normalized_and_cannot_escape_workspace() {
    let files = normalize_generated_config_files(vec![ProjectRuntimeEnvironmentConfigFileInput {
        path: " ./config/application-sandbox.yml ".to_string(),
        format: None,
        content: "server:\n  port: ${APP_PORT}\n".to_string(),
        description: Some("沙箱运行配置".to_string()),
        source_files: vec!["src/main/resources/application.yml".to_string()],
    }])
    .expect("normalize generated config file");
    assert_eq!(files[0].path, "config/application-sandbox.yml");
    assert_eq!(files[0].format, "yaml");
    assert!(
        normalize_generated_config_files(vec![ProjectRuntimeEnvironmentConfigFileInput {
            path: "../application.yml".to_string(),
            content: "server: {}".to_string(),
            ..ProjectRuntimeEnvironmentConfigFileInput::default()
        },])
        .is_err()
    );
}

#[test]
fn environment_variables_restore_omitted_dependency_plans() {
    let kinds = infer_service_kinds_from_environment_variables(
        &[],
        &[
            ProjectRuntimeEnvironmentVariableInput {
                name: "SPRING_DATASOURCE_URL".to_string(),
                recommended_value: Some("jdbc:mysql://mysql:3306/app".to_string()),
                ..ProjectRuntimeEnvironmentVariableInput::default()
            },
            ProjectRuntimeEnvironmentVariableInput {
                name: "SPRING_DATA_MONGODB_HOST".to_string(),
                recommended_value: Some("mongodb".to_string()),
                ..ProjectRuntimeEnvironmentVariableInput::default()
            },
            ProjectRuntimeEnvironmentVariableInput {
                name: "SPRING_CLOUD_NACOS_CONFIG_ENABLED".to_string(),
                recommended_value: Some("false".to_string()),
                ..ProjectRuntimeEnvironmentVariableInput::default()
            },
            ProjectRuntimeEnvironmentVariableInput {
                name: "REDIS_HOST".to_string(),
                recommended_value: Some("redis".to_string()),
                ..ProjectRuntimeEnvironmentVariableInput::default()
            },
        ],
        None,
    );
    assert_eq!(
        kinds,
        ["mongodb", "mysql", "nacos", "redis"]
            .into_iter()
            .map(ToOwned::to_owned)
            .collect()
    );
    let mut services = json!([]);
    ensure_required_service_records(&mut services, kinds);
    assert_eq!(provisionable_service_kinds(&services).len(), 4);
}

#[test]
fn application_image_plan_without_catalog_match_is_program_managed_and_planned() {
    let record = image_input_to_record(
        "project-1",
        ProjectRuntimeEnvironmentImageInput {
            environment_key: Some("application_runtime".to_string()),
            environment_type: Some("runtime".to_string()),
            dockerfile: Some("FROM node:24".to_string()),
            ..ProjectRuntimeEnvironmentImageInput::default()
        },
        0,
        RuntimeEnvironmentProvider::CloudSandboxManager,
        None,
    )
    .expect("planned application image");

    assert_eq!(
        record.image_provider,
        RuntimeEnvironmentProvider::CloudSandboxManager
    );
    assert_eq!(record.status, "planned");
    assert!(record.image_id.is_none());
    assert!(record.image_ref.is_none());
    assert_eq!(record.service_role, RuntimeServiceRole::Application);
    assert_eq!(
        record.mcp_policy.attachment,
        crate::models::RuntimeMcpAttachment::ProjectGatewayTarget
    );
}

#[test]
fn dependency_image_plan_uses_platform_image_without_manual_build() {
    let record = image_input_to_record(
        "project-1",
        ProjectRuntimeEnvironmentImageInput {
            environment_key: Some("redis".to_string()),
            environment_type: Some("service".to_string()),
            ..ProjectRuntimeEnvironmentImageInput::default()
        },
        1,
        RuntimeEnvironmentProvider::CloudSandboxManager,
        None,
    )
    .expect("platform dependency image");

    assert_eq!(
        record.image_provider,
        RuntimeEnvironmentProvider::CloudSandboxManager
    );
    assert_eq!(record.image_ref.as_deref(), Some("redis:7-alpine"));
    assert_eq!(record.status, "ready");
    assert_eq!(record.service_role, RuntimeServiceRole::Dependency);
    assert_eq!(record.mcp_policy, ProgramManagedMcpPolicy::default());
}

#[test]
fn application_image_can_reuse_an_initialized_catalog_image_id() {
    let catalog = json!({
        "images": [{
            "id": "dev-java8",
            "image_ref": "chatos-sandbox-agent:dev-java8",
            "features": ["java@8"],
            "initialized": true,
            "status": "ready"
        }]
    });
    let record = image_input_to_record(
        "project-1",
        ProjectRuntimeEnvironmentImageInput {
            environment_key: Some("app".to_string()),
            environment_type: Some("application".to_string()),
            image_id: Some("dev-java8".to_string()),
            features: Some(json!(["java@8"])),
            dockerfile: Some("FROM maven:3-eclipse-temurin-8".to_string()),
            ..ProjectRuntimeEnvironmentImageInput::default()
        },
        0,
        RuntimeEnvironmentProvider::CloudSandboxManager,
        Some(&catalog),
    )
    .expect("reuse catalog image");

    assert_eq!(record.image_id.as_deref(), Some("dev-java8"));
    assert_eq!(
        record.image_ref.as_deref(),
        Some("chatos-sandbox-agent:dev-java8")
    );
    assert_eq!(record.features, json!(["java@8"]));
    assert_eq!(record.status, "ready");
}

#[test]
fn agent_cannot_submit_program_managed_mcp_or_image_control_fields() {
    let image = json!({
        "environment_key": "api",
        "environment_type": "application",
        "display_name": "API",
        "dockerfile": "FROM node:24",
        "mcp_policy": { "attachment": "project_gateway_target" }
    });
    assert!(serde_json::from_value::<ProjectRuntimeEnvironmentImageInput>(image).is_err());

    let top_level = json!({
        "environment_variable_scan": {
            "completed": true,
            "files_scanned": [],
            "reference_count": 0,
            "summary": "scan complete"
        },
        "environment_variables": [],
        "generated_config_files": [],
        "mcp_enabled": true
    });
    assert!(serde_json::from_value::<UpdateProjectEnvironmentToolArgs>(top_level).is_err());

    for forbidden in [
        json!({"analysis_summary": "AI summary"}),
        json!({"status": "ready"}),
        json!({"last_error": "AI error"}),
        json!({"sandbox_provider": "cloud_sandbox_manager"}),
    ] {
        assert!(serde_json::from_value::<UpdateProjectEnvironmentToolArgs>(forbidden).is_err());
    }
}

#[test]
fn agent_visible_state_hides_routing_and_program_managed_fields() {
    let project = ProjectRecord {
        id: "project-1".to_string(),
        creator_user_id: None,
        creator_username: None,
        creator_display_name: None,
        owner_user_id: Some("user-1".to_string()),
        owner_username: None,
        owner_display_name: None,
        name: "Example".to_string(),
        root_path: Some("/private/workspace".to_string()),
        git_url: None,
        source_type: crate::models::ProjectSourceType::Cloud,
        execution_plane: crate::models::ProjectExecutionPlane::Cloud,
        cloud_import_source: crate::models::CloudImportSource::Zip,
        import_status: crate::models::ProjectImportStatus::Ready,
        source_git_url: None,
        harness_space_identifier: Some("space".to_string()),
        harness_repo_identifier: Some("repo".to_string()),
        harness_repo_path: Some("secret-path".to_string()),
        harness_git_url: None,
        harness_git_ssh_url: None,
        harness_default_branch: None,
        harness_provision_status: None,
        harness_provision_error: None,
        harness_provisioned_at: None,
        import_error: None,
        import_started_at: None,
        import_finished_at: None,
        description: None,
        status: crate::models::ProjectStatus::Active,
        created_at: "now".to_string(),
        updated_at: "now".to_string(),
        archived_at: None,
    };
    let mut environment =
        crate::services::runtime_environment::default_runtime_environment_for_project(
            &project,
            Some(true),
        );
    environment.file_provider = RuntimeEnvironmentProvider::Harness;
    environment.sandbox_provider = RuntimeEnvironmentProvider::CloudSandboxManager;
    environment.analysis_summary =
        Some("云端项目只通过 Harness MCP 读取文件，并只使用云端 Sandbox Manager。".to_string());
    let image = planned_image("services/api", "application");

    let visible = agent_visible_runtime_state(&project, &environment, &[image]);
    assert!(visible.pointer("/analysis/status").is_none());
    assert!(visible.pointer("/images/0/status").is_none());
    assert!(visible.pointer("/images/0/error").is_none());
    let serialized = serde_json::to_string(&visible).expect("serialize agent-visible state");
    for forbidden in [
        "file_provider",
        "sandbox_provider",
        "analysis_summary",
        "mcp_policy",
        "image_provider",
        "harness_repo_identifier",
        "secret-path",
        "/private/workspace",
    ] {
        assert!(!serialized.contains(forbidden));
    }
}

#[test]
fn technical_summary_is_generated_from_program_results() {
    let project = ProjectRecord {
        id: "project-1".to_string(),
        creator_user_id: None,
        creator_username: None,
        creator_display_name: None,
        owner_user_id: Some("user-1".to_string()),
        owner_username: None,
        owner_display_name: None,
        name: "Example".to_string(),
        root_path: None,
        git_url: None,
        source_type: crate::models::ProjectSourceType::Cloud,
        execution_plane: crate::models::ProjectExecutionPlane::Cloud,
        cloud_import_source: crate::models::CloudImportSource::Zip,
        import_status: crate::models::ProjectImportStatus::Ready,
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
        import_error: None,
        import_started_at: None,
        import_finished_at: None,
        description: None,
        status: crate::models::ProjectStatus::Active,
        created_at: "now".to_string(),
        updated_at: "now".to_string(),
        archived_at: None,
    };
    let mut environment =
        crate::services::runtime_environment::default_runtime_environment_for_project(
            &project,
            Some(true),
        );
    environment.status = ProjectRuntimeEnvironmentStatus::PendingImageBuild;
    environment.generated_config_files = vec![ProjectRuntimeEnvironmentConfigFileRecord {
        path: ".env.chatos".to_string(),
        format: "dotenv".to_string(),
        content: "PORT=3000".to_string(),
        description: None,
        source_files: vec![".env.example".to_string()],
    }];
    let images = vec![
        planned_image("services/api", "application"),
        planned_image("redis", "service"),
    ];

    let summary = program_generated_runtime_analysis_summary(&environment, images.as_slice());
    assert!(summary.contains("1 个应用组件"));
    assert!(summary.contains("1 个依赖服务"));
    assert!(summary.contains("等待生成应用镜像"));
    assert!(!summary.contains("Harness"));
    assert!(!summary.contains("Sandbox Manager"));
}

#[test]
fn application_dockerfile_cannot_install_program_managed_mcp_agent() {
    let mut controlled = planned_image("services/api", "application");
    controlled.dockerfile = Some(
        "FROM node:24\nCOPY chatos-sandbox-mcp-server /opt/chatos/bin/\nENV MCP_TOKEN=ai\n"
            .to_string(),
    );
    let error = validate_environment_image_plans(
        &json!({"language": "Node.js"}),
        &json!([]),
        &[controlled],
    )
    .expect_err("AI-authored MCP installation must be rejected");
    assert!(error.contains("program-managed Chat OS MCP Agent"));
}

#[test]
fn compose_planning_requires_application_dockerfile_and_each_dependency_record() {
    let stack = json!({"languages": ["java"], "manifests": ["pom.xml"]});
    let services = json!([
        {"type": "mysql"},
        {"type": "mongodb"},
        {"type": "redis"},
        {"type": "nacos"}
    ]);
    let mut images = vec![
        planned_image("application_runtime", "runtime"),
        planned_image("mysql", "service"),
        planned_image("mongodb", "service"),
        planned_image("redis", "service"),
        planned_image("nacos", "service"),
    ];
    validate_environment_image_plans(&stack, &services, &images)
        .expect("all Dockerfile plans exist");

    images.retain(|image| image.environment_key != "redis");
    let missing = validate_environment_image_plans(&stack, &services, &images)
        .expect_err("missing redis plan must be rejected");
    assert!(missing.contains("redis"));

    let mut standard_service = planned_image("redis", "service");
    standard_service.dockerfile = None;
    images.push(standard_service);
    validate_environment_image_plans(&stack, &services, &images)
        .expect("dependency services use platform-maintained images");

    images[0].dockerfile = None;
    let invalid = validate_environment_image_plans(&stack, &services, &images)
        .expect_err("application Dockerfile must be rejected");
    assert!(invalid.contains("application"));
}

#[test]
fn project_compose_groups_application_and_dependencies() {
    let images = vec![
        planned_image("services/api", "application"),
        planned_image("services/worker", "application"),
        planned_image("mysql", "service"),
        planned_image("redis", "service"),
    ];
    let compose = build_project_compose_yaml(
        "project-123",
        &[],
        &json!([{"type": "mysql"}, {"type": "redis"}]),
        images.as_slice(),
    )
    .expect("compose plan");
    assert!(compose.contains("name: \"chatos-project123\""));
    assert!(compose.contains("  services-api:"));
    assert!(compose.contains("  services-worker:"));
    assert!(compose
        .contains("dockerfile: .chatos/runtime-environment/services/services-api/Dockerfile"));
    assert!(compose
        .contains("dockerfile: .chatos/runtime-environment/services/services-worker/Dockerfile"));
    assert!(compose.contains("  mysql:"));
    assert!(compose.contains("  redis:"));
    assert!(compose.contains("depends_on:"));
    assert!(compose.contains("127.0.0.1:3306:3306"));
    assert!(compose.contains("127.0.0.1:6379:6379"));
}

#[test]
fn application_service_ids_are_program_normalized_and_bounded() {
    let image = planned_image(
        "123/services/API Worker with a deliberately very long component name that exceeds compose limits",
        "application",
    );
    let service_id = super::super::runtime_application_service_id(&image, 0);
    assert!(service_id.starts_with("app-123-services-api-worker"));
    assert!(service_id.len() <= 63);
    assert!(service_id.chars().all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
    }));
    assert!(!service_id.ends_with('-'));
}

#[test]
fn runnable_stack_cannot_be_downgraded_to_not_runnable() {
    assert!(environment_has_provisionable_evidence(
        &json!({
            "language": "Java",
            "build_tool": "Maven",
            "project_type": "Spring Boot backend"
        }),
        &json!([]),
        &[],
    ));
    assert!(environment_has_provisionable_evidence(
        &json!({}),
        &json!([{"type": "redis"}]),
        &[],
    ));
    assert!(!environment_has_provisionable_evidence(
        &json!({"source": "scan"}),
        &json!([]),
        &[],
    ));
}

#[test]
fn detected_services_receive_local_connection_defaults() {
    let env = generated_environment_variables(
        &json!([
            {"type": "nacos"},
            {"type": "redis"},
            {"type": "mongodb"},
            {"type": "mysql"}
        ]),
        None,
    );
    assert_eq!(env["NACOS_SERVER_ADDR"], "nacos:8848");
    assert_eq!(env["SPRING_DATA_REDIS_HOST"], "redis");
    assert_eq!(env["SPRING_DATA_MONGODB_HOST"], "mongodb");
    assert_eq!(
        env["SPRING_DATASOURCE_URL"],
        "jdbc:mysql://mysql:3306/app?useSSL=false&allowPublicKeyRetrieval=true"
    );
    assert_eq!(env["MYSQL_PASSWORD"], env["SPRING_DATASOURCE_PASSWORD"]);
}

#[test]
fn service_images_receive_default_ports() {
    assert_eq!(
        default_ports_for_environment("redis", "service"),
        json!([6379])
    );
    assert_eq!(
        default_ports_for_environment("nacos", "service"),
        json!([8848, 9848, 9849])
    );
    assert_eq!(default_ports_for_environment("app", "runtime"), json!([]));
}
