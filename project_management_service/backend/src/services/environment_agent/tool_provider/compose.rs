// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) const PROJECT_COMPOSE_FILE_PATH: &str =
    ".chatos/runtime-environment/docker-compose.chatos.yml";

pub(super) fn upsert_project_compose_config_file(
    project_id: &str,
    files: &mut Vec<ProjectRuntimeEnvironmentConfigFileRecord>,
    variables: &[ProjectRuntimeEnvironmentVariableRecord],
    required_services: &Value,
    images: &[ProjectRuntimeEnvironmentImageRecord],
) -> Result<(), String> {
    let compose = build_project_compose_yaml(project_id, variables, required_services, images)?;
    files.retain(|file| file.path != PROJECT_COMPOSE_FILE_PATH);
    files.push(ProjectRuntimeEnvironmentConfigFileRecord {
        path: PROJECT_COMPOSE_FILE_PATH.to_string(),
        format: "yaml".to_string(),
        content: compose,
        description: Some(
            "项目级 Docker Compose 编排文件：应用和所有依赖服务会作为同一个 Compose 项目启动。"
                .to_string(),
        ),
        source_files: vec!["项目环境扫描结果".to_string()],
    });
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(())
}

pub(super) fn build_project_compose_yaml(
    project_id: &str,
    variables: &[ProjectRuntimeEnvironmentVariableRecord],
    required_services: &Value,
    images: &[ProjectRuntimeEnvironmentImageRecord],
) -> Result<String, String> {
    let application = images
        .iter()
        .find(|image| image_is_application_runtime(image))
        .ok_or_else(|| "application runtime plan is required for Docker Compose".to_string())?;
    if application
        .dockerfile
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        return Err("application Dockerfile is required for Docker Compose".to_string());
    }
    let service_kinds = provisionable_service_kinds(required_services);
    let mut output = String::new();
    output.push_str("name: ");
    output.push_str(yaml_string(compose_project_name(project_id).as_str()).as_str());
    output.push_str("\nservices:\n  application:\n    build:\n      context: ../..\n      dockerfile: .chatos/runtime-environment/Dockerfile.application\n    env_file:\n      - .env.chatos\n");
    if let Some(ports) = application.ports.as_array() {
        let ports = ports
            .iter()
            .filter_map(Value::as_u64)
            .filter(|port| *port > 0 && *port <= u16::MAX as u64)
            .collect::<Vec<_>>();
        if !ports.is_empty() {
            output.push_str("    ports:\n");
            for port in ports {
                output.push_str(format!("      - \"127.0.0.1:{port}:{port}\"\n").as_str());
            }
        }
    }
    if !service_kinds.is_empty() {
        output.push_str("    depends_on:\n");
        for service in &service_kinds {
            output.push_str(
                format!(
                    "      {}:\n        condition: service_healthy\n",
                    compose_service_name(service)
                )
                .as_str(),
            );
        }
    }
    output.push_str("    networks:\n      - chatos-runtime\n    restart: unless-stopped\n");
    for service in &service_kinds {
        append_compose_dependency_service(&mut output, service.as_str());
    }
    output.push_str("networks:\n  chatos-runtime:\n    driver: bridge\n");
    let volumes = service_kinds
        .iter()
        .filter_map(|service| compose_service_volume(service.as_str()))
        .collect::<Vec<_>>();
    if !volumes.is_empty() {
        output.push_str("volumes:\n");
        for volume in volumes {
            output.push_str(format!("  {volume}:\n").as_str());
        }
    }
    let _ = variables;
    Ok(output)
}

pub(super) fn compose_project_name(project_id: &str) -> String {
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

pub(super) fn compose_service_name(service: &str) -> &str {
    match service {
        "mongodb" => "mongodb",
        "postgres" => "postgres",
        other => other,
    }
}

pub(super) fn compose_service_volume(service: &str) -> Option<&'static str> {
    match service {
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

pub(super) fn append_compose_dependency_service(output: &mut String, service: &str) {
    match service {
        "mysql" => output.push_str("  mysql:\n    image: mysql:8.4\n    env_file: [.env.chatos]\n    environment:\n      MYSQL_DATABASE: ${MYSQL_DATABASE:-app}\n      MYSQL_USER: ${MYSQL_USER:-app}\n      MYSQL_PASSWORD: ${MYSQL_PASSWORD}\n      MYSQL_ROOT_PASSWORD: ${MYSQL_ROOT_PASSWORD}\n    ports: [\"127.0.0.1:3306:3306\"]\n    volumes: [mysql-data:/var/lib/mysql]\n    healthcheck:\n      test: [\"CMD-SHELL\", \"mysqladmin ping -h 127.0.0.1 -p$${MYSQL_ROOT_PASSWORD} --silent\"]\n      interval: 10s\n      timeout: 5s\n      retries: 20\n    networks: [chatos-runtime]\n    restart: unless-stopped\n"),
        "mongodb" => output.push_str("  mongodb:\n    image: mongo:7.0\n    env_file: [.env.chatos]\n    environment:\n      MONGO_INITDB_ROOT_USERNAME: ${MONGO_INITDB_ROOT_USERNAME:-app}\n      MONGO_INITDB_ROOT_PASSWORD: ${MONGO_INITDB_ROOT_PASSWORD}\n      MONGO_INITDB_DATABASE: ${MONGODB_DATABASE:-app}\n    ports: [\"127.0.0.1:27017:27017\"]\n    volumes: [mongodb-data:/data/db]\n    healthcheck:\n      test: [\"CMD-SHELL\", \"mongosh --quiet --eval 'db.runCommand({ ping: 1 }).ok' || exit 1\"]\n      interval: 10s\n      timeout: 5s\n      retries: 20\n    networks: [chatos-runtime]\n    restart: unless-stopped\n"),
        "postgres" => output.push_str("  postgres:\n    image: postgres:16-alpine\n    env_file: [.env.chatos]\n    environment:\n      POSTGRES_DB: ${POSTGRES_DB:-app}\n      POSTGRES_USER: ${POSTGRES_USER:-app}\n      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD}\n    ports: [\"127.0.0.1:5432:5432\"]\n    volumes: [postgres-data:/var/lib/postgresql/data]\n    healthcheck:\n      test: [\"CMD-SHELL\", \"pg_isready -U $${POSTGRES_USER} -d $${POSTGRES_DB}\"]\n      interval: 10s\n      timeout: 5s\n      retries: 20\n    networks: [chatos-runtime]\n    restart: unless-stopped\n"),
        "redis" => output.push_str("  redis:\n    image: redis:7-alpine\n    env_file: [.env.chatos]\n    command: [\"sh\", \"-c\", \"exec redis-server --appendonly yes --requirepass '$${REDIS_PASSWORD}'\"]\n    ports: [\"127.0.0.1:6379:6379\"]\n    volumes: [redis-data:/data]\n    healthcheck:\n      test: [\"CMD-SHELL\", \"redis-cli -a '$${REDIS_PASSWORD}' ping | grep PONG\"]\n      interval: 10s\n      timeout: 5s\n      retries: 20\n    networks: [chatos-runtime]\n    restart: unless-stopped\n"),
        "nacos" => output.push_str("  nacos:\n    image: nacos/nacos-server:v2.4.3\n    environment:\n      MODE: standalone\n      NACOS_AUTH_ENABLE: \"false\"\n    ports: [\"127.0.0.1:8848:8848\", \"127.0.0.1:9848:9848\", \"127.0.0.1:9849:9849\"]\n    volumes: [nacos-data:/home/nacos/data]\n    healthcheck:\n      test: [\"CMD-SHELL\", \"curl -fsS http://127.0.0.1:8848/nacos/ >/dev/null || exit 1\"]\n      interval: 15s\n      timeout: 5s\n      retries: 30\n    networks: [chatos-runtime]\n    restart: unless-stopped\n"),
        "rabbitmq" => output.push_str("  rabbitmq:\n    image: rabbitmq:3.13-management-alpine\n    env_file: [.env.chatos]\n    environment:\n      RABBITMQ_DEFAULT_USER: ${RABBITMQ_DEFAULT_USER:-app}\n      RABBITMQ_DEFAULT_PASS: ${RABBITMQ_DEFAULT_PASS}\n    ports: [\"127.0.0.1:5672:5672\", \"127.0.0.1:15672:15672\"]\n    volumes: [rabbitmq-data:/var/lib/rabbitmq]\n    healthcheck:\n      test: [\"CMD\", \"rabbitmq-diagnostics\", \"-q\", \"ping\"]\n      interval: 10s\n      timeout: 5s\n      retries: 20\n    networks: [chatos-runtime]\n    restart: unless-stopped\n"),
        "kafka" => output.push_str("  kafka:\n    image: bitnami/kafka:3.7\n    environment:\n      KAFKA_CFG_NODE_ID: 1\n      KAFKA_CFG_PROCESS_ROLES: broker,controller\n      KAFKA_CFG_CONTROLLER_QUORUM_VOTERS: 1@kafka:9093\n      KAFKA_CFG_LISTENERS: PLAINTEXT://:9092,CONTROLLER://:9093\n      KAFKA_CFG_ADVERTISED_LISTENERS: PLAINTEXT://kafka:9092\n      KAFKA_CFG_LISTENER_SECURITY_PROTOCOL_MAP: CONTROLLER:PLAINTEXT,PLAINTEXT:PLAINTEXT\n      KAFKA_CFG_CONTROLLER_LISTENER_NAMES: CONTROLLER\n    ports: [\"127.0.0.1:9092:9092\"]\n    volumes: [kafka-data:/bitnami/kafka]\n    healthcheck:\n      test: [\"CMD-SHELL\", \"kafka-topics.sh --bootstrap-server 127.0.0.1:9092 --list >/dev/null 2>&1\"]\n      interval: 15s\n      timeout: 10s\n      retries: 20\n    networks: [chatos-runtime]\n    restart: unless-stopped\n"),
        "elasticsearch" => output.push_str("  elasticsearch:\n    image: docker.elastic.co/elasticsearch/elasticsearch:8.14.3\n    environment:\n      discovery.type: single-node\n      xpack.security.enabled: \"false\"\n      ES_JAVA_OPTS: -Xms512m -Xmx512m\n    ports: [\"127.0.0.1:9200:9200\"]\n    volumes: [elasticsearch-data:/usr/share/elasticsearch/data]\n    healthcheck:\n      test: [\"CMD-SHELL\", \"curl -fsS http://127.0.0.1:9200/_cluster/health >/dev/null\"]\n      interval: 15s\n      timeout: 10s\n      retries: 30\n    networks: [chatos-runtime]\n    restart: unless-stopped\n"),
        "minio" => output.push_str("  minio:\n    image: minio/minio:latest\n    env_file: [.env.chatos]\n    environment:\n      MINIO_ROOT_USER: ${MINIO_ROOT_USER:-minioadmin}\n      MINIO_ROOT_PASSWORD: ${MINIO_ROOT_PASSWORD}\n    command: server /data --console-address :9001\n    ports: [\"127.0.0.1:9000:9000\", \"127.0.0.1:9001:9001\"]\n    volumes: [minio-data:/data]\n    healthcheck:\n      test: [\"CMD\", \"mc\", \"ready\", \"local\"]\n      interval: 10s\n      timeout: 5s\n      retries: 20\n    networks: [chatos-runtime]\n    restart: unless-stopped\n"),
        _ => {}
    }
}

pub(super) fn yaml_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"chatos-project\"".to_string())
}

pub(super) fn image_plan_is_complete(image: &ProjectRuntimeEnvironmentImageRecord) -> bool {
    image_is_real_and_ready(image)
        || image
            .dockerfile
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
}

pub(super) fn stack_requires_application_runtime(detected_stack: &Value) -> bool {
    [
        "language",
        "languages",
        "runtime",
        "framework",
        "frameworks",
        "build_tool",
        "package_manager",
        "project_type",
        "entrypoint",
        "startup_command",
    ]
    .iter()
    .any(|key| json_value_has_content(detected_stack.get(*key)))
        || detected_stack
            .get("manifests")
            .and_then(Value::as_array)
            .is_some_and(|manifests| {
                manifests.iter().any(|manifest| {
                    manifest.as_str().is_some_and(|value| {
                        let value = value.trim().to_ascii_lowercase();
                        [
                            "package.json",
                            "cargo.toml",
                            "pyproject.toml",
                            "requirements.txt",
                            "go.mod",
                            "pom.xml",
                            "build.gradle",
                            "build.gradle.kts",
                        ]
                        .iter()
                        .any(|candidate| value.ends_with(candidate))
                    })
                })
            })
}

pub(super) fn provisionable_service_kinds(
    required_services: &Value,
) -> std::collections::BTreeSet<String> {
    let mut kinds = std::collections::BTreeSet::new();
    for service in required_services.as_array().into_iter().flatten() {
        let raw = service
            .as_str()
            .or_else(|| {
                ["type", "service_type", "kind", "name", "service"]
                    .iter()
                    .find_map(|key| service.get(*key).and_then(Value::as_str))
            })
            .unwrap_or_default();
        infer_service_kinds_from_text(raw, &mut kinds);
    }
    kinds
}

pub(super) fn image_is_real_and_ready(image: &ProjectRuntimeEnvironmentImageRecord) -> bool {
    image.image_provider != RuntimeEnvironmentProvider::None
        && image
            .image_id
            .as_deref()
            .or(image.image_ref.as_deref())
            .is_some_and(|value| !value.trim().is_empty())
        && matches!(
            image.status.trim().to_ascii_lowercase().as_str(),
            "ready" | "available" | "local" | "succeeded" | "completed" | "running"
        )
}

pub(super) fn image_is_application_runtime(image: &ProjectRuntimeEnvironmentImageRecord) -> bool {
    let environment_type = image.environment_type.trim().to_ascii_lowercase();
    let environment_key = image.environment_key.trim().to_ascii_lowercase();
    environment_type.contains("runtime")
        || environment_type.contains("application")
        || matches!(
            environment_key.as_str(),
            "app" | "application" | "runtime" | "application_runtime"
        )
        || environment_key.ends_with("_runtime")
}

pub(super) fn image_matches_service(
    image: &ProjectRuntimeEnvironmentImageRecord,
    service: &str,
) -> bool {
    let identity = format!(
        "{} {} {}",
        image.environment_key, image.environment_type, image.display_name
    )
    .to_ascii_lowercase();
    match service {
        "mongodb" => ["mongodb", "mongo"]
            .iter()
            .any(|alias| identity.contains(alias)),
        "mysql" => ["mysql", "mariadb"]
            .iter()
            .any(|alias| identity.contains(alias)),
        "postgres" => ["postgres", "postgresql"]
            .iter()
            .any(|alias| identity.contains(alias)),
        "elasticsearch" => ["elasticsearch", "opensearch"]
            .iter()
            .any(|alias| identity.contains(alias)),
        other => identity.contains(other),
    }
}

pub(super) fn json_value_has_content(value: Option<&Value>) -> bool {
    match value {
        Some(Value::String(value)) => !value.trim().is_empty(),
        Some(Value::Array(value)) => !value.is_empty(),
        Some(Value::Object(value)) => !value.is_empty(),
        Some(Value::Bool(value)) => *value,
        Some(Value::Number(_)) => true,
        _ => false,
    }
}

pub(super) fn is_executable_project_manifest(value: &str) -> bool {
    let value = value.trim().to_ascii_lowercase();
    [
        "package.json",
        "cargo.toml",
        "pyproject.toml",
        "requirements.txt",
        "go.mod",
        "pom.xml",
        "build.gradle",
        "build.gradle.kts",
        "docker-compose.yml",
        "docker-compose.yaml",
        "compose.yml",
        "compose.yaml",
    ]
    .iter()
    .any(|manifest| value.ends_with(manifest))
}

pub(super) fn default_ports_for_environment(
    environment_key: &str,
    environment_type: &str,
) -> Value {
    let identity = format!("{environment_key} {environment_type}").to_ascii_lowercase();
    let ports: &[u16] = if identity.contains("nacos") {
        &[8848, 9848, 9849]
    } else if identity.contains("postgres") {
        &[5432]
    } else if identity.contains("mysql") || identity.contains("mariadb") {
        &[3306]
    } else if identity.contains("redis") {
        &[6379]
    } else if identity.contains("mongo") {
        &[27017]
    } else if identity.contains("rabbitmq") {
        &[5672, 15672]
    } else {
        &[]
    };
    Value::Array(ports.iter().copied().map(Value::from).collect())
}

pub(super) fn insert_text_default(
    env_vars: &mut serde_json::Map<String, Value>,
    key: &str,
    value: &str,
) {
    let should_insert = env_vars
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .is_none_or(|value| value.is_empty());
    if should_insert {
        env_vars.insert(key.to_string(), Value::String(value.to_string()));
    }
}

pub(super) fn copy_text_default(
    env_vars: &mut serde_json::Map<String, Value>,
    source: &str,
    target: &str,
) {
    let Some(value) = env_vars
        .get(source)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
    else {
        return;
    };
    insert_text_default(env_vars, target, value.as_str());
}

pub(super) fn insert_secret_default(env_vars: &mut serde_json::Map<String, Value>, key: &str) {
    let should_insert = env_vars
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .is_none_or(|value| value.is_empty());
    if should_insert {
        env_vars.insert(
            key.to_string(),
            Value::String(format!("pm-{}", Uuid::new_v4().simple())),
        );
    }
}

pub(super) fn normalize_owned(value: String) -> Option<String> {
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

pub(super) fn normalize_multiline_owned(value: String) -> Option<String> {
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}
