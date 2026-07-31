// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use super::scan::LocalProjectScanEvidence;

pub(super) fn environment_analysis_prompt(
    project_id: &str,
    project_name: &str,
    evidence: &LocalProjectScanEvidence,
    capability_prompt: Option<&str>,
    analysis_requirement: Option<&str>,
    selected_dependencies: &[String],
) -> Result<String, String> {
    let context = serde_json::json!({
        "mode": "local_json_analysis",
        "project": {
            "id": project_id,
            "name": project_name,
        },
        "analysis_request": {
            "user_requirement": analysis_requirement
                .map(str::trim)
                .filter(|value| !value.is_empty()),
            "selected_dependencies": selected_dependencies,
        },
        "local_scan_evidence": evidence,
        "plugin_capability_constraints": capability_prompt
            .map(str::trim)
            .filter(|value| !value.is_empty()),
    });
    serde_json::to_string_pretty(&context).map_err(|error| error.to_string())
}

pub(super) fn normalize_analysis(
    mut value: super::LocalEnvironmentAnalysisResult,
) -> Result<super::LocalEnvironmentAnalysisResult, String> {
    value.status = value.status.trim().to_ascii_lowercase();
    if !matches!(
        value.status.as_str(),
        "ready" | "not_runnable" | "pending_configuration"
    ) {
        return Err(format!("unsupported environment status: {}", value.status));
    }
    value.detected_stack = object_or_default(value.detected_stack);
    value.required_services = array_or_default(value.required_services);
    value.env_vars = object_or_default(value.env_vars);
    value.generated_config_files = array_or_default(value.generated_config_files);
    value
        .images
        .retain(|image| !image.environment_key.trim().is_empty());
    for image in &mut value.images {
        image.environment_key = image.environment_key.trim().to_string();
        image.environment_type = image.environment_type.trim().to_ascii_lowercase();
        image.display_name = image.display_name.trim().to_string();
        image.source_root = normalize_component_root(image.source_root.as_str())?;
        image.component_kind = if image.component_kind.trim().is_empty() {
            image.environment_type.clone()
        } else {
            image.component_kind.trim().to_ascii_lowercase()
        };
        image.startup_command = normalized_optional_text(image.startup_command.take());
        image.test_command = normalized_optional_text(image.test_command.take());
        image.depends_on = normalized_depends_on(std::mem::take(&mut image.depends_on));
        image.auto_start = image.component_kind != "artifact";
    }
    ensure_application_dockerfile(&mut value);
    if let Some(image) = value.images.iter().find(|image| {
        image
            .dockerfile
            .as_deref()
            .is_some_and(dockerfile_contains_program_managed_mcp_control)
    }) {
        return Err(format!(
            "application Dockerfile attempts to install or configure the program-managed Chat OS MCP Agent: {}",
            image.environment_key
        ));
    }
    Ok(value)
}

fn normalize_component_root(value: &str) -> Result<String, String> {
    let value = value.trim().replace('\\', "/");
    if value.is_empty() || value == "." {
        return Ok(".".to_string());
    }
    if value.starts_with('/')
        || value
            .as_bytes()
            .get(1)
            .is_some_and(|separator| *separator == b':')
    {
        return Err(format!("invalid local component source_root: {value}"));
    }
    let segments = value
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .collect::<Vec<_>>();
    if segments.is_empty() || segments.contains(&"..") {
        return Err(format!("invalid local component source_root: {value}"));
    }
    Ok(segments.join("/"))
}

fn normalized_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_string();
        (!value.is_empty()).then_some(value)
    })
}

fn normalized_depends_on(values: Vec<String>) -> Vec<String> {
    let mut values = values
        .into_iter()
        .filter_map(|value| normalized_optional_text(Some(value)))
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

pub(super) fn enforce_selected_dependencies(
    value: &mut super::LocalEnvironmentAnalysisResult,
    selected_dependencies: &[String],
) {
    let selected_kinds = selected_dependencies
        .iter()
        .filter_map(|dependency| selected_dependency_kind(dependency))
        .collect::<std::collections::BTreeSet<_>>();
    if selected_kinds.is_empty() {
        return;
    }

    let services = value
        .required_services
        .as_array_mut()
        .expect("required services are normalized to an array");
    let mut existing_service_kinds = services
        .iter()
        .filter_map(service_identity)
        .filter_map(selected_dependency_kind)
        .collect::<std::collections::BTreeSet<_>>();
    for kind in &selected_kinds {
        if existing_service_kinds.insert(kind) {
            services.push(serde_json::json!({
                "type": kind,
                "source": "user_selection",
            }));
        }
    }

    for kind in selected_kinds {
        if value.images.iter().any(|image| {
            selected_dependency_kind(
                format!(
                    "{} {} {}",
                    image.environment_key, image.environment_type, image.display_name
                )
                .as_str(),
            ) == Some(kind)
        }) {
            continue;
        }
        value.images.push(super::models::LocalEnvironmentImagePlan {
            environment_key: kind.to_string(),
            environment_type: "service".to_string(),
            display_name: dependency_display_name(kind).to_string(),
            ports: dependency_ports(kind),
            ..Default::default()
        });
    }
}

fn service_identity(service: &Value) -> Option<&str> {
    service.as_str().or_else(|| {
        ["type", "service_type", "kind", "name", "service"]
            .iter()
            .find_map(|key| service.get(*key).and_then(Value::as_str))
    })
}

fn selected_dependency_kind(value: &str) -> Option<&'static str> {
    let value = value.trim().to_ascii_lowercase();
    [
        ("mysql", &["mysql", "mariadb"][..]),
        ("mongodb", &["mongodb", "mongo"][..]),
        ("postgres", &["postgres", "postgresql"][..]),
        ("redis", &["redis", "valkey", "dragonfly"][..]),
        ("nacos", &["nacos"][..]),
        ("rabbitmq", &["rabbitmq"][..]),
        ("kafka", &["kafka", "redpanda"][..]),
        ("elasticsearch", &["elasticsearch", "opensearch"][..]),
        ("minio", &["minio", "s3-compatible", "s3 compatible"][..]),
    ]
    .into_iter()
    .find_map(|(kind, aliases)| {
        aliases
            .iter()
            .any(|alias| value.contains(alias))
            .then_some(kind)
    })
}

fn dependency_display_name(kind: &str) -> &str {
    match kind {
        "postgres" => "PostgreSQL",
        "mysql" => "MySQL / MariaDB",
        "mongodb" => "MongoDB",
        "redis" => "Redis-compatible cache",
        "nacos" => "Nacos",
        "rabbitmq" => "RabbitMQ",
        "kafka" => "Apache Kafka-compatible broker",
        "elasticsearch" => "Elasticsearch-compatible search",
        "minio" => "MinIO / S3-compatible storage",
        other => other,
    }
}

fn dependency_ports(kind: &str) -> Value {
    let ports: &[u16] = match kind {
        "postgres" => &[5432],
        "mysql" => &[3306],
        "mongodb" => &[27017],
        "redis" => &[6379],
        "nacos" => &[8848, 9848, 9849],
        "rabbitmq" => &[5672, 15672],
        "kafka" => &[9092],
        "elasticsearch" => &[9200],
        "minio" => &[9000, 9001],
        _ => &[],
    };
    Value::Array(ports.iter().copied().map(Value::from).collect())
}

fn dockerfile_contains_program_managed_mcp_control(dockerfile: &str) -> bool {
    let dockerfile = dockerfile.to_ascii_lowercase();
    [
        "chatos-sandbox-mcp",
        "chatos_sandbox_mcp",
        "chat os mcp agent",
        "chatos mcp agent",
        "mcp_token",
        "mcp_port",
        "mcp_image",
        "mcp_command",
        "agent_install_script",
        "agent_injection_mode",
        "/opt/chatos/",
    ]
    .iter()
    .any(|marker| dockerfile.contains(marker))
}

fn ensure_application_dockerfile(value: &mut super::LocalEnvironmentAnalysisResult) {
    if value.status == "not_runnable" {
        return;
    }
    let fallback = fallback_dockerfile(&value.detected_stack);
    let mut application_found = false;
    for image in &mut value.images {
        if image
            .environment_type
            .trim()
            .eq_ignore_ascii_case("application")
        {
            application_found = true;
            if image
                .dockerfile
                .as_deref()
                .map(str::trim)
                .is_none_or(str::is_empty)
            {
                image.dockerfile = Some(fallback.clone());
            }
        } else {
            image.dockerfile = None;
        }
    }
    if !application_found {
        value.images.insert(
            0,
            super::models::LocalEnvironmentImagePlan {
                environment_key: "app".to_string(),
                environment_type: "application".to_string(),
                display_name: "Application".to_string(),
                dockerfile: Some(fallback),
                ..Default::default()
            },
        );
    }
}

pub(crate) fn fallback_dockerfile(stack: &Value) -> String {
    let has = |name: &str| stack.get(name).and_then(Value::as_bool).unwrap_or(false);
    if has("nodejs") {
        return "FROM node:22-bookworm-slim\nWORKDIR /app\nCOPY package*.json ./\nRUN if [ -f package-lock.json ]; then npm ci; else npm install; fi\nCOPY . .\nEXPOSE 3000\nCMD [\"npm\", \"start\"]\n".to_string();
    }
    if has("python") {
        return "FROM python:3.12-slim\nWORKDIR /app\nCOPY . .\nRUN if [ -f requirements.txt ]; then pip install --no-cache-dir -r requirements.txt; elif [ -f pyproject.toml ]; then pip install --no-cache-dir .; fi\nCMD [\"python\", \"main.py\"]\n".to_string();
    }
    if has("rust") {
        return "FROM rust:1-bookworm AS build\nWORKDIR /app\nCOPY . .\nRUN cargo build --release\n\nFROM debian:bookworm-slim\nWORKDIR /app\nCOPY --from=build /app/target/release/ /app/bin/\nCMD [\"sh\", \"-lc\", \"exec /app/bin/$(find /app/bin -maxdepth 1 -type f -perm -111 | head -n 1 | xargs basename)\"]\n".to_string();
    }
    if has("go") {
        return "FROM golang:1.24-bookworm AS build\nWORKDIR /app\nCOPY . .\nRUN go build -o /out/app .\n\nFROM debian:bookworm-slim\nCOPY --from=build /out/app /app\nCMD [\"/app\"]\n".to_string();
    }
    if has("java") {
        return "FROM maven:3-eclipse-temurin-21 AS build\nWORKDIR /app\nCOPY . .\nRUN mvn -DskipTests package\n\nFROM eclipse-temurin:21-jre\nCOPY --from=build /app/target/*.jar /app/app.jar\nCMD [\"java\", \"-jar\", \"/app/app.jar\"]\n".to_string();
    }
    "FROM ubuntu:24.04\nWORKDIR /app\nCOPY . .\nCMD [\"sh\"]\n".to_string()
}

fn object_or_default(value: Value) -> Value {
    if value.is_object() {
        value
    } else {
        serde_json::json!({})
    }
}

fn array_or_default(value: Value) -> Value {
    if value.is_array() {
        value
    } else {
        serde_json::json!([])
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn selected_postgres_is_restored_when_the_model_omits_it() {
        let mut analysis = super::super::models::LocalEnvironmentAnalysisResult {
            status: "ready".to_string(),
            required_services: serde_json::json!([]),
            images: vec![super::super::models::LocalEnvironmentImagePlan {
                environment_key: "app".to_string(),
                environment_type: "application".to_string(),
                display_name: "Application".to_string(),
                dockerfile: Some("FROM node:22".to_string()),
                ..Default::default()
            }],
            ..Default::default()
        };

        super::enforce_selected_dependencies(&mut analysis, &["PostgreSQL".to_string()]);

        assert_eq!(analysis.required_services[0]["type"], "postgres");
        assert!(analysis.images.iter().any(|image| {
            image.environment_key == "postgres"
                && image.environment_type == "service"
                && image.ports == serde_json::json!([5432])
        }));
    }

    #[test]
    fn fills_missing_application_dockerfile_locally() {
        let analysis =
            super::normalize_analysis(super::super::models::LocalEnvironmentAnalysisResult {
                status: "ready".to_string(),
                detected_stack: serde_json::json!({ "nodejs": true }),
                images: vec![super::super::models::LocalEnvironmentImagePlan {
                    environment_key: "app".to_string(),
                    environment_type: "application".to_string(),
                    display_name: "Application".to_string(),
                    ..Default::default()
                }],
                ..Default::default()
            })
            .expect("normalize analysis");

        assert!(analysis.images[0]
            .dockerfile
            .as_deref()
            .is_some_and(|dockerfile| dockerfile.contains("FROM node:22")));
    }

    #[test]
    fn rejects_ai_authored_mcp_installation_in_local_dockerfile() {
        let error =
            super::normalize_analysis(super::super::models::LocalEnvironmentAnalysisResult {
                status: "ready".to_string(),
                detected_stack: serde_json::json!({ "nodejs": true }),
                images: vec![super::super::models::LocalEnvironmentImagePlan {
                    environment_key: "services/api".to_string(),
                    environment_type: "application".to_string(),
                    display_name: "API".to_string(),
                    dockerfile: Some(
                        "FROM node:24\nCOPY chatos-sandbox-mcp-server /opt/chatos/bin/\n"
                            .to_string(),
                    ),
                    ..Default::default()
                }],
                ..Default::default()
            })
            .expect_err("AI-authored MCP installation must be rejected");
        assert!(error.contains("program-managed Chat OS MCP Agent"));
    }
}
