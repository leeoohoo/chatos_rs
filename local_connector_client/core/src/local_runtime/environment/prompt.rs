// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use super::scan::LocalProjectScanEvidence;

pub(super) fn environment_analysis_prompt(
    project_id: &str,
    project_name: &str,
    evidence: &LocalProjectScanEvidence,
    capability_prompt: Option<&str>,
) -> Result<String, String> {
    let context = serde_json::json!({
        "mode": "local_json_analysis",
        "project": {
            "id": project_id,
            "name": project_name,
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
    if value.analysis_summary.trim().is_empty() {
        return Err("environment analysis_summary is required".to_string());
    }
    value.detected_stack = object_or_default(value.detected_stack);
    value.required_services = array_or_default(value.required_services);
    value.env_vars = object_or_default(value.env_vars);
    value.generated_config_files = array_or_default(value.generated_config_files);
    value
        .images
        .retain(|image| !image.environment_key.trim().is_empty());
    ensure_application_dockerfile(&mut value);
    Ok(value)
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
    value
        .is_object()
        .then_some(value)
        .unwrap_or_else(|| serde_json::json!({}))
}

fn array_or_default(value: Value) -> Value {
    value
        .is_array()
        .then_some(value)
        .unwrap_or_else(|| serde_json::json!([]))
}

#[cfg(test)]
mod tests {
    #[test]
    fn fills_missing_application_dockerfile_locally() {
        let analysis =
            super::normalize_analysis(super::super::models::LocalEnvironmentAnalysisResult {
                status: "ready".to_string(),
                analysis_summary: "Node service".to_string(),
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
}
