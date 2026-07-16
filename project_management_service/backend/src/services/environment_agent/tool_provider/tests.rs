// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

fn planned_image(
    environment_key: &str,
    environment_type: &str,
) -> ProjectRuntimeEnvironmentImageRecord {
    ProjectRuntimeEnvironmentImageRecord {
        id: format!("record-{environment_key}"),
        project_id: "project-1".to_string(),
        environment_key: environment_key.to_string(),
        environment_type: environment_type.to_string(),
        display_name: environment_key.to_string(),
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
    }
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
fn application_image_plan_ignores_agent_ready_state_and_provider_override() {
    let record = image_input_to_record(
        "project-1",
        ProjectRuntimeEnvironmentImageInput {
            environment_key: Some("application_runtime".to_string()),
            environment_type: Some("runtime".to_string()),
            image_id: Some("agent-image".to_string()),
            image_ref: Some("agent/runtime:latest".to_string()),
            _image_provider: Some("local_connector".to_string()),
            dockerfile: Some("FROM node:24".to_string()),
            status: Some("ready".to_string()),
            ..ProjectRuntimeEnvironmentImageInput::default()
        },
        0,
        RuntimeEnvironmentProvider::CloudSandboxManager,
    );

    assert_eq!(
        record.image_provider,
        RuntimeEnvironmentProvider::CloudSandboxManager
    );
    assert_eq!(record.status, "planned");
    assert!(record.image_id.is_none());
    assert!(record.image_ref.is_none());
}

#[test]
fn dependency_image_plan_uses_platform_image_without_manual_build() {
    let record = image_input_to_record(
        "project-1",
        ProjectRuntimeEnvironmentImageInput {
            environment_key: Some("redis".to_string()),
            environment_type: Some("service".to_string()),
            _image_provider: Some("local_connector".to_string()),
            status: Some("planned".to_string()),
            ..ProjectRuntimeEnvironmentImageInput::default()
        },
        1,
        RuntimeEnvironmentProvider::CloudSandboxManager,
    );

    assert_eq!(
        record.image_provider,
        RuntimeEnvironmentProvider::CloudSandboxManager
    );
    assert_eq!(record.image_ref.as_deref(), Some("redis:7-alpine"));
    assert_eq!(record.status, "ready");
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
        planned_image("application_runtime", "runtime"),
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
    assert!(compose.contains("  application:"));
    assert!(compose.contains("  mysql:"));
    assert!(compose.contains("  redis:"));
    assert!(compose.contains("depends_on:"));
    assert!(compose.contains("127.0.0.1:3306:3306"));
    assert!(compose.contains("127.0.0.1:6379:6379"));
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
