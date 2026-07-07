// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::config::optional_env;
use crate::sandbox::catalog::local_sandbox_runtime_specs;

pub(super) async fn local_docker_image_status(image_ref: &str) -> &'static str {
    match tokio::process::Command::new("docker")
        .args(["image", "inspect", image_ref])
        .output()
        .await
    {
        Ok(output) if output.status.success() => "local",
        _ => "missing",
    }
}

pub(super) fn local_sandbox_image_build_context() -> PathBuf {
    optional_env("LOCAL_CONNECTOR_SANDBOX_IMAGE_BUILD_CONTEXT")
        .map(PathBuf::from)
        .or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .and_then(Path::parent)
                .map(Path::to_path_buf)
        })
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub(super) fn local_sandbox_image_dockerfile(context: &Path) -> PathBuf {
    optional_env("LOCAL_CONNECTOR_SANDBOX_IMAGE_DOCKERFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            context
                .join("sandbox_manager_service")
                .join("sandbox_agent")
                .join("Dockerfile")
        })
}

pub(super) fn normalize_local_sandbox_features(
    features: Vec<String>,
) -> Result<Vec<String>, String> {
    let catalog = local_sandbox_runtime_specs();
    let mut allowed = std::collections::BTreeSet::new();
    for feature in catalog {
        let Some(runtime) = feature.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(versions) = feature.get("versions").and_then(Value::as_array) else {
            continue;
        };
        for version in versions {
            if let Some(version) = version.get("id").and_then(Value::as_str) {
                allowed.insert(format!("{runtime}@{version}"));
            }
        }
    }
    let mut normalized = std::collections::BTreeSet::new();
    for feature in features {
        let value = feature.trim().to_ascii_lowercase();
        if value.is_empty() {
            continue;
        }
        if !allowed.contains(value.as_str()) {
            return Err(format!("unsupported sandbox runtime version: {value}"));
        }
        normalized.insert(value);
    }
    Ok(normalized.into_iter().collect())
}

pub(super) fn local_sandbox_image_id(
    features: &[String],
    custom_build_script: Option<&str>,
) -> String {
    let mut hasher = Sha256::new();
    for feature in features {
        hasher.update(feature.as_bytes());
        hasher.update(b"\n");
    }
    if let Some(script) = custom_build_script {
        hasher.update(b"custom\n");
        hasher.update(script.as_bytes());
    }
    let digest = hex::encode(hasher.finalize());
    let feature_slug = if features.is_empty() {
        "base".to_string()
    } else {
        features
            .iter()
            .map(|feature| feature.replace('@', "-").replace('.', "_"))
            .collect::<Vec<_>>()
            .join("_")
    };
    format!("local-{feature_slug}-{}", &digest[..12])
}
