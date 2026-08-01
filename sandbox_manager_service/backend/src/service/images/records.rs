// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) fn default_image_record(
    config: &AppConfig,
    backend: SandboxBackendKind,
) -> SandboxImageRecord {
    SandboxImageRecord {
        id: DEFAULT_IMAGE_ID.to_string(),
        name: "Default".to_string(),
        description: "Service default image from runtime configuration".to_string(),
        image_ref: default_image_ref(config, backend),
        features: image_specs::default_image_features(),
        backend: backend.as_str().to_string(),
        initialized: false,
        status: "unknown".to_string(),
        buildable: false,
        is_default: true,
    }
}

pub(super) fn dependency_catalog_image_record(
    backend: SandboxBackendKind,
    image_ref: &str,
) -> SandboxImageRecord {
    SandboxImageRecord {
        id: dependency_image_id(image_ref),
        name: dependency_image_name(image_ref),
        description: dependency_image_description(image_ref),
        image_ref: image_ref.to_string(),
        features: vec![format!("dependency@{image_ref}")],
        backend: backend.as_str().to_string(),
        initialized: false,
        status: "unknown".to_string(),
        buildable: false,
        is_default: false,
    }
}

pub(super) fn generated_image_record(
    config: &AppConfig,
    backend: SandboxBackendKind,
    selections: &[RuntimeSelectionSpec],
    custom_script_hash: Option<&str>,
) -> SandboxImageRecord {
    let mut feature_ids = selections
        .iter()
        .map(image_specs::selection_feature_token)
        .collect::<Vec<_>>();
    if let Some(hash) = custom_script_hash {
        feature_ids.push(format!("script@{hash}"));
    }
    let id = generated_image_id(&feature_ids, custom_script_hash);
    let name = if selections.is_empty() {
        if let Some(hash) = custom_script_hash {
            format!("Base + Custom script {hash}")
        } else {
            "Base".to_string()
        }
    } else {
        let mut names = selections
            .iter()
            .map(image_specs::selection_label)
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        if let Some(hash) = custom_script_hash {
            names.push(format!("Custom script {hash}"));
        }
        names.join(" + ")
    };
    let description = if selections.is_empty() {
        if custom_script_hash.is_some() {
            "Base image with custom build script".to_string()
        } else {
            "Base image with common shell, git, Python and workspace tools".to_string()
        }
    } else {
        format!("Development image with {name}")
    };

    SandboxImageRecord {
        id: id.clone(),
        name,
        description,
        image_ref: format!("{}:{id}", normalized_tag_prefix(config)),
        features: feature_ids,
        backend: backend.as_str().to_string(),
        initialized: false,
        status: "unknown".to_string(),
        buildable: true,
        is_default: false,
    }
}

pub(super) fn generated_image_record_for_id(
    config: &AppConfig,
    backend: SandboxBackendKind,
    image_id: &str,
) -> Option<SandboxImageRecord> {
    let parsed = image_specs::parse_generated_image_id(image_id)?;
    let mut record = generated_image_record(
        config,
        backend,
        &parsed.selections,
        parsed.custom_script_hash.as_deref(),
    );
    if record.id != image_id {
        record.id = image_id.to_string();
        record.image_ref = format!("{}:{image_id}", normalized_tag_prefix(config));
    }
    Some(record)
}

pub(super) fn local_image_record(
    config: &AppConfig,
    backend: SandboxBackendKind,
    image_ref: &str,
) -> Option<SandboxImageRecord> {
    let prefix = normalized_tag_prefix(config);
    let tag = image_ref.strip_prefix(format!("{prefix}:").as_str())?;
    let parsed = image_specs::parse_generated_image_id(tag)?;
    let mut record = generated_image_record(
        config,
        backend,
        &parsed.selections,
        parsed.custom_script_hash.as_deref(),
    );
    record.id = tag.to_string();
    record.image_ref = image_ref.to_string();
    record.initialized = true;
    record.status = "ready".to_string();
    Some(record)
}

pub(super) fn dependency_image_description(image_ref: &str) -> String {
    let label = match image_ref.trim() {
        "postgres:16-alpine" => "PostgreSQL database",
        "mysql:8.4" => "MySQL database",
        "mongo:7.0" => "MongoDB document database",
        "redis:7-alpine" => "Redis cache",
        "nacos/nacos-server:v2.4.3" => "Nacos service discovery and configuration",
        "rabbitmq:3.13-management-alpine" => "RabbitMQ message broker",
        "bitnami/kafka:3.7" => "Kafka streaming platform",
        "docker.elastic.co/elasticsearch/elasticsearch:8.14.3" => "Elasticsearch search engine",
        "minio/minio:latest" => "MinIO object storage",
        _ => "Platform dependency image",
    };
    format!("Platform-managed dependency image for {label}")
}

pub(super) fn normalize_custom_build_script(
    script: Option<&str>,
) -> Result<Option<String>, String> {
    let Some(script) = script else {
        return Ok(None);
    };
    let script = script.trim();
    if script.is_empty() {
        return Ok(None);
    }
    if script.len() > MAX_CUSTOM_BUILD_SCRIPT_LEN {
        return Err(format!(
            "custom build script is too large; maximum size is {} bytes",
            MAX_CUSTOM_BUILD_SCRIPT_LEN
        ));
    }
    if script.contains('\0') {
        return Err("custom build script cannot contain NUL bytes".to_string());
    }
    Ok(Some(script.to_string()))
}

pub(super) fn generated_image_id(
    feature_ids: &[String],
    custom_script_hash: Option<&str>,
) -> String {
    let mut segments = feature_ids
        .iter()
        .filter(|feature| !feature.starts_with("script@"))
        .map(|feature| feature.replace('@', ""))
        .collect::<Vec<_>>();
    if let Some(hash) = custom_script_hash {
        segments.push(format!("script{hash}"));
    }
    if segments.is_empty() {
        return "base".to_string();
    }
    format!("dev-{}", segments.join("-"))
}

pub(super) fn normalized_tag_prefix(config: &AppConfig) -> String {
    let prefix = config.image_tag_prefix.trim();
    if prefix.is_empty() {
        "chatos-sandbox-agent".to_string()
    } else {
        prefix.trim_end_matches(':').to_string()
    }
}

pub(super) fn default_image_ref(config: &AppConfig, backend: SandboxBackendKind) -> String {
    match backend {
        SandboxBackendKind::Kata => config.kata_image.clone(),
        SandboxBackendKind::Docker | SandboxBackendKind::Mock => config.docker_image.clone(),
    }
}

pub(super) fn container_cli(config: &AppConfig, backend: SandboxBackendKind) -> &str {
    match backend {
        SandboxBackendKind::Kata => config.kata_container_cli.as_str(),
        SandboxBackendKind::Docker | SandboxBackendKind::Mock => "docker",
    }
}
