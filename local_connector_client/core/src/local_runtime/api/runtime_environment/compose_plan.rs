// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use crate::local_runtime::storage::LocalProjectRecord;
use crate::local_runtime::{LocalRuntimeEnvironmentImageRecord, LocalRuntimeEnvironmentRecord};

use super::response::program_managed_service_id;

pub(super) struct LocalComposeBuildPlan {
    pub(super) request: Value,
    pub(super) image_refs: Vec<(String, String)>,
}

pub(super) fn build_local_compose_plan(
    project: &LocalProjectRecord,
    environment: &LocalRuntimeEnvironmentRecord,
    images: &[LocalRuntimeEnvironmentImageRecord],
) -> Result<LocalComposeBuildPlan, String> {
    let project_name = compose_project_name(project.project_id.as_str());
    let mut application_dockerfiles = BTreeMap::new();
    let mut application_plans = Vec::new();
    for image in images.iter().filter(|image| image_is_application(image)) {
        let dockerfile = image
            .dockerfile
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                format!(
                    "Local application Dockerfile is missing: {}",
                    image.environment_key
                )
            })?;
        let service_id = program_managed_service_id(image.environment_key.as_str(), "application");
        if application_dockerfiles
            .insert(service_id.clone(), dockerfile.to_string())
            .is_some()
        {
            return Err(format!(
                "Duplicate local application service id: {service_id}"
            ));
        }
        application_plans.push((service_id, image));
    }
    if application_plans.is_empty() {
        return Err("Local runtime plan has no buildable application".to_string());
    }

    let required_services = parse_json(environment.required_services_json.as_str());
    let dependency_kinds = dependency_kinds(&required_services, images);
    let mut compose_yaml = String::new();
    compose_yaml.push_str(format!("name: {}\nservices:\n", yaml_string(&project_name)).as_str());
    for (service_id, image) in &application_plans {
        append_application_service(
            &mut compose_yaml,
            service_id.as_str(),
            image,
            &dependency_kinds,
        );
    }
    for dependency in &dependency_kinds {
        append_dependency_service(&mut compose_yaml, dependency.as_str());
    }
    compose_yaml.push_str("networks:\n  chatos-runtime:\n    driver: bridge\n");
    let volumes = dependency_kinds
        .iter()
        .filter_map(|kind| dependency_volume(kind.as_str()))
        .collect::<Vec<_>>();
    if !volumes.is_empty() {
        compose_yaml.push_str("volumes:\n");
        for volume in volumes {
            compose_yaml.push_str(format!("  {volume}:\n").as_str());
        }
    }

    let env_vars = parse_json(environment.env_vars_json.as_str());
    let env_file = environment_dotenv(&env_vars)?;
    let mut image_refs = application_plans
        .iter()
        .map(|(service_id, image)| (image.id.clone(), format!("{project_name}-{service_id}")))
        .collect::<Vec<_>>();
    for image in images.iter().filter(|image| !image_is_application(image)) {
        if let Some(kind) = dependency_kind_from_identity(image_identity(image).as_str()) {
            if dependency_kinds.contains(kind) {
                if let Some(image_ref) = dependency_image_ref(kind) {
                    image_refs.push((image.id.clone(), image_ref.to_string()));
                }
            }
        }
    }

    Ok(LocalComposeBuildPlan {
        request: json!({
            "project_name": project_name,
            "project_relative_path": project.root_relative_path,
            "compose_yaml": compose_yaml,
            "application_dockerfiles": application_dockerfiles,
            "env_file": env_file,
        }),
        image_refs,
    })
}

fn image_is_application(image: &LocalRuntimeEnvironmentImageRecord) -> bool {
    matches!(
        image.environment_type.trim().to_ascii_lowercase().as_str(),
        "application" | "runtime"
    ) && image
        .dockerfile
        .as_deref()
        .is_some_and(|dockerfile| !dockerfile.trim().is_empty())
}

fn append_application_service(
    output: &mut String,
    service_id: &str,
    image: &LocalRuntimeEnvironmentImageRecord,
    dependency_kinds: &BTreeSet<String>,
) {
    output.push_str(format!("  {service_id}:\n").as_str());
    output.push_str("    build:\n      context: ../..\n");
    output.push_str(
        format!("      dockerfile: .chatos/runtime-environment/services/{service_id}/Dockerfile\n")
            .as_str(),
    );
    output.push_str("    env_file:\n      - .env.chatos\n");
    let ports = application_ports(image);
    if !ports.is_empty() {
        output.push_str("    ports:\n");
        for port in ports {
            output.push_str(format!("      - \"127.0.0.1:{port}:{port}\"\n").as_str());
        }
    }
    if !dependency_kinds.is_empty() {
        output.push_str("    depends_on:\n");
        for dependency in dependency_kinds {
            output.push_str(
                format!(
                    "      {}:\n        condition: service_healthy\n",
                    dependency_service_name(dependency)
                )
                .as_str(),
            );
        }
    }
    output.push_str("    networks:\n      - chatos-runtime\n    restart: unless-stopped\n");
}

fn application_ports(image: &LocalRuntimeEnvironmentImageRecord) -> BTreeSet<u16> {
    let ports = parse_json(image.ports_json.as_str());
    ports
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| {
            value
                .as_u64()
                .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
                .or_else(|| {
                    ["target", "container", "port"]
                        .iter()
                        .find_map(|key| value.get(*key).and_then(Value::as_u64))
                })
        })
        .filter(|port| *port > 0 && *port <= u16::MAX as u64)
        .map(|port| port as u16)
        .collect()
}

fn environment_dotenv(value: &Value) -> Result<String, String> {
    let Some(entries) = value.as_object() else {
        return Ok(String::new());
    };
    let mut output = String::new();
    for (name, descriptor) in entries {
        if !valid_environment_variable_name(name) {
            return Err(format!("Invalid local environment variable name: {name}"));
        }
        let Some(value) = effective_environment_value(descriptor) else {
            continue;
        };
        if value.contains('\0') {
            return Err(format!(
                "Local environment variable contains a NUL byte: {name}"
            ));
        }
        let encoded = serde_json::to_string(value.as_str())
            .map_err(|error| format!("Encode local environment variable {name}: {error}"))?;
        output.push_str(name);
        output.push('=');
        output.push_str(encoded.as_str());
        output.push('\n');
    }
    Ok(output)
}

fn effective_environment_value(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Object(value) => [
            "effective_value",
            "user_value",
            "recommended_value",
            "project_value",
            "value",
        ]
        .iter()
        .find_map(|key| value.get(*key).and_then(scalar_text)),
        _ => None,
    }
}

fn scalar_text(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn valid_environment_variable_name(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn dependency_kinds(
    required_services: &Value,
    images: &[LocalRuntimeEnvironmentImageRecord],
) -> BTreeSet<String> {
    let mut kinds = BTreeSet::new();
    for service in required_services.as_array().into_iter().flatten() {
        let identity = service
            .as_str()
            .or_else(|| {
                ["type", "service_type", "kind", "name", "service"]
                    .iter()
                    .find_map(|key| service.get(*key).and_then(Value::as_str))
            })
            .unwrap_or_default();
        if let Some(kind) = dependency_kind_from_identity(identity) {
            kinds.insert(kind.to_string());
        }
    }
    for image in images.iter().filter(|image| !image_is_application(image)) {
        if let Some(kind) = dependency_kind_from_identity(image_identity(image).as_str()) {
            kinds.insert(kind.to_string());
        }
    }
    kinds
}

fn image_identity(image: &LocalRuntimeEnvironmentImageRecord) -> String {
    format!(
        "{} {} {} {}",
        image.environment_key,
        image.environment_type,
        image.display_name,
        image.image_ref.as_deref().unwrap_or_default()
    )
}

fn dependency_kind_from_identity(value: &str) -> Option<&'static str> {
    let value = value.to_ascii_lowercase();
    [
        ("mysql", &["mysql", "mariadb"][..]),
        ("mongodb", &["mongodb", "mongo"][..]),
        ("postgres", &["postgres", "postgresql"][..]),
        ("redis", &["redis"][..]),
        ("nacos", &["nacos"][..]),
        ("rabbitmq", &["rabbitmq"][..]),
        ("kafka", &["kafka"][..]),
        ("elasticsearch", &["elasticsearch", "opensearch"][..]),
        ("minio", &["minio"][..]),
    ]
    .into_iter()
    .find_map(|(kind, aliases)| {
        aliases
            .iter()
            .any(|alias| value.contains(alias))
            .then_some(kind)
    })
}

fn dependency_service_name(value: &str) -> &str {
    value
}

fn dependency_image_ref(value: &str) -> Option<&'static str> {
    match value {
        "mysql" => Some("mysql:8.4"),
        "mongodb" => Some("mongo:7.0"),
        "postgres" => Some("postgres:16-alpine"),
        "redis" => Some("redis:7-alpine"),
        "nacos" => Some("nacos/nacos-server:v2.4.3"),
        "rabbitmq" => Some("rabbitmq:3.13-management-alpine"),
        "kafka" => Some("bitnami/kafka:3.7"),
        "elasticsearch" => Some("docker.elastic.co/elasticsearch/elasticsearch:8.14.3"),
        "minio" => Some("minio/minio:latest"),
        _ => None,
    }
}

fn dependency_volume(value: &str) -> Option<&'static str> {
    match value {
        "mysql" => Some("mysql-data"),
        "mongodb" => Some("mongodb-data"),
        "postgres" => Some("postgres-data"),
        "redis" => Some("redis-data"),
        "nacos" => Some("nacos-data"),
        "rabbitmq" => Some("rabbitmq-data"),
        "kafka" => Some("kafka-data"),
        "elasticsearch" => Some("elasticsearch-data"),
        "minio" => Some("minio-data"),
        _ => None,
    }
}

fn append_dependency_service(output: &mut String, service: &str) {
    match service {
        "mysql" => output.push_str("  mysql:\n    image: mysql:8.4\n    env_file: [.env.chatos]\n    environment:\n      MYSQL_DATABASE: \"${MYSQL_DATABASE:-app}\"\n      MYSQL_USER: \"${MYSQL_USER:-app}\"\n      MYSQL_PASSWORD: \"${MYSQL_PASSWORD}\"\n      MYSQL_ROOT_PASSWORD: \"${MYSQL_ROOT_PASSWORD}\"\n    ports: [\"127.0.0.1:3306:3306\"]\n    volumes: [mysql-data:/var/lib/mysql]\n    healthcheck:\n      test: [\"CMD-SHELL\", \"mysqladmin ping -h 127.0.0.1 -p$${MYSQL_ROOT_PASSWORD} --silent\"]\n      interval: 10s\n      timeout: 5s\n      retries: 20\n    networks: [chatos-runtime]\n    restart: unless-stopped\n"),
        "mongodb" => output.push_str("  mongodb:\n    image: mongo:7.0\n    env_file: [.env.chatos]\n    environment:\n      MONGO_INITDB_ROOT_USERNAME: \"${MONGO_INITDB_ROOT_USERNAME:-app}\"\n      MONGO_INITDB_ROOT_PASSWORD: \"${MONGO_INITDB_ROOT_PASSWORD}\"\n      MONGO_INITDB_DATABASE: \"${MONGODB_DATABASE:-app}\"\n    ports: [\"127.0.0.1:27017:27017\"]\n    volumes: [mongodb-data:/data/db]\n    healthcheck:\n      test: [\"CMD-SHELL\", \"mongosh --quiet --eval 'db.runCommand({ ping: 1 }).ok' || exit 1\"]\n      interval: 10s\n      timeout: 5s\n      retries: 20\n    networks: [chatos-runtime]\n    restart: unless-stopped\n"),
        "postgres" => output.push_str("  postgres:\n    image: postgres:16-alpine\n    env_file: [.env.chatos]\n    environment:\n      POSTGRES_DB: \"${POSTGRES_DB:-app}\"\n      POSTGRES_USER: \"${POSTGRES_USER:-app}\"\n      POSTGRES_PASSWORD: \"${POSTGRES_PASSWORD}\"\n    ports: [\"127.0.0.1:5432:5432\"]\n    volumes: [postgres-data:/var/lib/postgresql/data]\n    healthcheck:\n      test: [\"CMD-SHELL\", \"pg_isready -U $${POSTGRES_USER} -d $${POSTGRES_DB}\"]\n      interval: 10s\n      timeout: 5s\n      retries: 20\n    networks: [chatos-runtime]\n    restart: unless-stopped\n"),
        "redis" => output.push_str("  redis:\n    image: redis:7-alpine\n    env_file: [.env.chatos]\n    command: [\"sh\", \"-c\", \"exec redis-server --appendonly yes --requirepass '$${REDIS_PASSWORD}'\"]\n    ports: [\"127.0.0.1:6379:6379\"]\n    volumes: [redis-data:/data]\n    healthcheck:\n      test: [\"CMD-SHELL\", \"redis-cli -a '$${REDIS_PASSWORD}' ping | grep PONG\"]\n      interval: 10s\n      timeout: 5s\n      retries: 20\n    networks: [chatos-runtime]\n    restart: unless-stopped\n"),
        "nacos" => output.push_str("  nacos:\n    image: nacos/nacos-server:v2.4.3\n    environment:\n      MODE: standalone\n      NACOS_AUTH_ENABLE: \"false\"\n    ports: [\"127.0.0.1:8848:8848\", \"127.0.0.1:9848:9848\", \"127.0.0.1:9849:9849\"]\n    volumes: [nacos-data:/home/nacos/data]\n    healthcheck:\n      test: [\"CMD-SHELL\", \"curl -fsS http://127.0.0.1:8848/nacos/ >/dev/null || exit 1\"]\n      interval: 15s\n      timeout: 5s\n      retries: 30\n    networks: [chatos-runtime]\n    restart: unless-stopped\n"),
        "rabbitmq" => output.push_str("  rabbitmq:\n    image: rabbitmq:3.13-management-alpine\n    env_file: [.env.chatos]\n    environment:\n      RABBITMQ_DEFAULT_USER: \"${RABBITMQ_DEFAULT_USER:-app}\"\n      RABBITMQ_DEFAULT_PASS: \"${RABBITMQ_DEFAULT_PASS}\"\n    ports: [\"127.0.0.1:5672:5672\", \"127.0.0.1:15672:15672\"]\n    volumes: [rabbitmq-data:/var/lib/rabbitmq]\n    healthcheck:\n      test: [\"CMD\", \"rabbitmq-diagnostics\", \"-q\", \"ping\"]\n      interval: 10s\n      timeout: 5s\n      retries: 20\n    networks: [chatos-runtime]\n    restart: unless-stopped\n"),
        "kafka" => output.push_str("  kafka:\n    image: bitnami/kafka:3.7\n    environment:\n      KAFKA_CFG_NODE_ID: 1\n      KAFKA_CFG_PROCESS_ROLES: broker,controller\n      KAFKA_CFG_CONTROLLER_QUORUM_VOTERS: 1@kafka:9093\n      KAFKA_CFG_LISTENERS: PLAINTEXT://:9092,CONTROLLER://:9093\n      KAFKA_CFG_ADVERTISED_LISTENERS: PLAINTEXT://kafka:9092\n      KAFKA_CFG_LISTENER_SECURITY_PROTOCOL_MAP: CONTROLLER:PLAINTEXT,PLAINTEXT:PLAINTEXT\n      KAFKA_CFG_CONTROLLER_LISTENER_NAMES: CONTROLLER\n    ports: [\"127.0.0.1:9092:9092\"]\n    volumes: [kafka-data:/bitnami/kafka]\n    healthcheck:\n      test: [\"CMD-SHELL\", \"kafka-topics.sh --bootstrap-server 127.0.0.1:9092 --list >/dev/null 2>&1\"]\n      interval: 15s\n      timeout: 10s\n      retries: 20\n    networks: [chatos-runtime]\n    restart: unless-stopped\n"),
        "elasticsearch" => output.push_str("  elasticsearch:\n    image: docker.elastic.co/elasticsearch/elasticsearch:8.14.3\n    environment:\n      discovery.type: single-node\n      xpack.security.enabled: \"false\"\n      ES_JAVA_OPTS: -Xms512m -Xmx512m\n    ports: [\"127.0.0.1:9200:9200\"]\n    volumes: [elasticsearch-data:/usr/share/elasticsearch/data]\n    healthcheck:\n      test: [\"CMD-SHELL\", \"curl -fsS http://127.0.0.1:9200/_cluster/health >/dev/null\"]\n      interval: 15s\n      timeout: 10s\n      retries: 30\n    networks: [chatos-runtime]\n    restart: unless-stopped\n"),
        "minio" => output.push_str("  minio:\n    image: minio/minio:latest\n    env_file: [.env.chatos]\n    environment:\n      MINIO_ROOT_USER: \"${MINIO_ROOT_USER:-minioadmin}\"\n      MINIO_ROOT_PASSWORD: \"${MINIO_ROOT_PASSWORD}\"\n    command: server /data --console-address :9001\n    ports: [\"127.0.0.1:9000:9000\", \"127.0.0.1:9001:9001\"]\n    volumes: [minio-data:/data]\n    healthcheck:\n      test: [\"CMD\", \"mc\", \"ready\", \"local\"]\n      interval: 10s\n      timeout: 5s\n      retries: 20\n    networks: [chatos-runtime]\n    restart: unless-stopped\n"),
        _ => {}
    }
}

fn compose_project_name(project_id: &str) -> String {
    let suffix = project_id
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .take(12)
        .collect::<String>()
        .to_ascii_lowercase();
    format!(
        "chatos-{}",
        if suffix.is_empty() {
            "project"
        } else {
            suffix.as_str()
        }
    )
}

fn yaml_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"chatos-project\"".to_string())
}

fn parse_json(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_compose_plan_uses_all_application_dockerfiles_and_recommended_env() {
        let project = LocalProjectRecord {
            project_id: "project-1234".to_string(),
            owner_user_id: "user".to_string(),
            device_id: "device".to_string(),
            workspace_id: "workspace".to_string(),
            project_name: "Project".to_string(),
            root_relative_path: Some("apps/project".to_string()),
            execution_plane: "local_connector".to_string(),
            runtime_schema_version: 1,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        };
        let environment = LocalRuntimeEnvironmentRecord {
            project_id: project.project_id.clone(),
            owner_user_id: project.owner_user_id.clone(),
            status: "ready".to_string(),
            sandbox_enabled: true,
            sandbox_provider: "local_connector".to_string(),
            file_provider: "local_connector".to_string(),
            analysis_summary: None,
            not_runnable_reason: None,
            detected_stack_json: "{}".to_string(),
            required_services_json: "[]".to_string(),
            env_vars_json: json!({
                "NODE_ENV": {
                    "project_value": "development",
                    "recommended_value": "production"
                }
            })
            .to_string(),
            generated_config_files_json: "[]".to_string(),
            last_agent_run_id: None,
            last_error: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        };
        let images = ["backend", "frontend"]
            .into_iter()
            .map(|key| LocalRuntimeEnvironmentImageRecord {
                id: format!("image-{key}"),
                project_id: project.project_id.clone(),
                environment_key: key.to_string(),
                environment_type: "application".to_string(),
                display_name: key.to_string(),
                image_id: None,
                image_ref: None,
                image_provider: "local_connector".to_string(),
                dockerfile: Some("FROM node:22\n".to_string()),
                features_json: "[]".to_string(),
                ports_json: if key == "backend" { "[3000]" } else { "[4173]" }.to_string(),
                env_vars_json: "{}".to_string(),
                status: "planned".to_string(),
                error: None,
                created_at: "now".to_string(),
                updated_at: "now".to_string(),
            })
            .collect::<Vec<_>>();

        let plan = build_local_compose_plan(&project, &environment, images.as_slice())
            .expect("build local Compose plan");
        let compose = plan.request["compose_yaml"].as_str().expect("compose YAML");
        assert!(compose.contains("services/backend/Dockerfile"));
        assert!(compose.contains("services/frontend/Dockerfile"));
        assert!(compose.contains("127.0.0.1:3000:3000"));
        assert_eq!(plan.request["env_file"], "NODE_ENV=\"production\"\n");
        assert_eq!(plan.image_refs.len(), 2);
    }
}
