// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::sandbox::catalog::local_sandbox_runtime_specs;
use crate::sandbox::types::{LocalSandboxImageJob, LocalSandboxImageRecord};
use crate::{
    local_now_rfc3339, tracing_stdout, LocalRuntime, LocalState, DEFAULT_LOCAL_SANDBOX_IMAGE,
    DEFAULT_LOCAL_SANDBOX_IMAGE_TAG_PREFIX, LOCAL_SANDBOX_BACKEND,
};

mod build;
mod job;

use build::{local_docker_image_status, local_sandbox_image_id, normalize_local_sandbox_features};
use job::run_local_sandbox_image_job;

pub(crate) async fn local_sandbox_image_catalog(runtime: &LocalRuntime) -> Value {
    let jobs = runtime.sandbox_runtime.jobs.read().await.clone();
    let stored_images = local_sandbox_stored_images(runtime).await;
    let mut seen_refs = std::collections::BTreeSet::new();
    seen_refs.insert(DEFAULT_LOCAL_SANDBOX_IMAGE.to_string());
    let mut images = vec![json!({
        "id": "default",
        "name": DEFAULT_LOCAL_SANDBOX_IMAGE,
        "image_ref": DEFAULT_LOCAL_SANDBOX_IMAGE,
        "features": ["java@21", "node@24", "rust@stable", "go@1.26"],
        "backend": LOCAL_SANDBOX_BACKEND,
        "status": local_docker_image_status(DEFAULT_LOCAL_SANDBOX_IMAGE).await,
    })];
    for image in stored_images {
        if !seen_refs.insert(image.image_ref.clone()) {
            continue;
        }
        images.push(json!({
            "id": image.id,
            "name": image.image_name,
            "image_ref": image.image_ref,
            "features": image.features,
            "backend": image.backend,
            "status": local_docker_image_status(image.image_ref.as_str()).await,
            "created_at": image.created_at,
        }));
    }
    for job in jobs.iter().filter(|job| job.status == "succeeded") {
        if !seen_refs.insert(job.image_ref.clone()) {
            continue;
        }
        images.push(json!({
            "id": job.image_id,
            "name": job.image_name,
            "image_ref": job.image_ref,
            "features": job.features,
            "backend": job.backend,
            "status": "local",
            "created_at": job.created_at,
        }));
    }
    json!({
        "backend": LOCAL_SANDBOX_BACKEND,
        "default_image_id": "default",
        "image_tag_prefix": DEFAULT_LOCAL_SANDBOX_IMAGE_TAG_PREFIX,
        "features": local_sandbox_runtime_specs(),
        "images": images,
    })
}

pub(crate) fn local_sandbox_image_ref_for_id(state: &LocalState, image_id: &str) -> Option<String> {
    state
        .sandbox
        .images
        .iter()
        .find(|image| image.id == image_id)
        .map(|image| image.image_ref.clone())
}

pub(crate) async fn start_local_sandbox_image_job(
    runtime: &LocalRuntime,
    features: Vec<String>,
    custom_build_script: Option<String>,
    project_id: Option<String>,
    run_id: Option<String>,
) -> Result<LocalSandboxImageJob, String> {
    if custom_build_script
        .as_deref()
        .map(str::len)
        .unwrap_or_default()
        > 128 * 1024
    {
        return Err("custom build script is too large".to_string());
    }
    let features = normalize_local_sandbox_features(features)?;
    let project_id = normalize_job_context(project_id);
    let run_id = normalize_job_context(run_id);
    let image_id = local_sandbox_image_id(features.as_slice(), custom_build_script.as_deref());
    if let Some(existing) = runtime
        .sandbox_runtime
        .jobs
        .read()
        .await
        .iter()
        .find(|job| {
            job.image_id == image_id
                && job.status == "running"
                && job.project_id == project_id
                && job.run_id == run_id
        })
        .cloned()
    {
        return Ok(existing);
    }
    let image_name = image_id.trim_start_matches("local-").to_string();
    let image_ref = format!("{DEFAULT_LOCAL_SANDBOX_IMAGE_TAG_PREFIX}:{image_name}");
    let now = local_now_rfc3339();
    let job = LocalSandboxImageJob {
        id: format!("image-job-{}", Uuid::new_v4()),
        image_id,
        image_name,
        image_ref,
        features,
        backend: LOCAL_SANDBOX_BACKEND.to_string(),
        status: "running".to_string(),
        created_at: now.clone(),
        updated_at: now.clone(),
        started_at: Some(now),
        finished_at: None,
        output: String::new(),
        error: None,
        project_id,
        run_id,
        custom_build_script,
    };
    runtime.sandbox_runtime.jobs.write().await.push(job.clone());

    let jobs = runtime.sandbox_runtime.jobs.clone();
    let state = runtime.state.clone();
    let state_path = runtime.state_path.clone();
    let job_id = job.id.clone();
    tokio::spawn(async move {
        run_local_sandbox_image_job(jobs, state, state_path, job_id).await;
    });
    Ok(job)
}

fn normalize_job_context(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

async fn local_sandbox_stored_images(runtime: &LocalRuntime) -> Vec<LocalSandboxImageRecord> {
    let mut state = runtime.state.write().await;
    if let Some(record) = imported_selected_sandbox_image(&state) {
        state.sandbox.images.push(record);
        if let Err(err) = state.save(runtime.state_path.as_path()) {
            tracing_stdout(format!("save imported local sandbox image failed: {err}").as_str());
        }
    }
    state.sandbox.images.clone()
}

fn imported_selected_sandbox_image(state: &LocalState) -> Option<LocalSandboxImageRecord> {
    let image_ref = state.sandbox.selected_image_ref.as_deref()?;
    if image_ref == DEFAULT_LOCAL_SANDBOX_IMAGE {
        return None;
    }
    if state
        .sandbox
        .images
        .iter()
        .any(|image| image.image_ref == image_ref)
    {
        return None;
    }
    let now = local_now_rfc3339();
    Some(LocalSandboxImageRecord {
        id: imported_local_sandbox_image_id(image_ref),
        image_name: image_ref
            .rsplit_once(':')
            .map(|(_, tag)| tag)
            .unwrap_or(image_ref)
            .to_string(),
        image_ref: image_ref.to_string(),
        features: vec!["imported".to_string()],
        backend: LOCAL_SANDBOX_BACKEND.to_string(),
        created_at: now.clone(),
        updated_at: now,
    })
}

fn imported_local_sandbox_image_id(image_ref: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(image_ref.as_bytes());
    let digest = hex::encode(hasher.finalize());
    format!("local-imported-{}", &digest[..12])
}
