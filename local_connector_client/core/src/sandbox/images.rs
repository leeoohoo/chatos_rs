// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};
use uuid::Uuid;

use crate::sandbox::catalog::local_sandbox_runtime_specs;
use crate::sandbox::types::LocalSandboxImageJob;
use crate::{
    local_now_rfc3339, LocalRuntime, DEFAULT_LOCAL_SANDBOX_IMAGE,
    DEFAULT_LOCAL_SANDBOX_IMAGE_TAG_PREFIX, LOCAL_SANDBOX_BACKEND,
};

mod build;
mod job;

use build::{local_docker_image_status, local_sandbox_image_id, normalize_local_sandbox_features};
use job::run_local_sandbox_image_job;

pub(crate) async fn local_sandbox_image_catalog(runtime: &LocalRuntime) -> Value {
    let jobs = runtime.sandbox_runtime.jobs.read().await.clone();
    let mut images = vec![json!({
        "id": "default",
        "name": DEFAULT_LOCAL_SANDBOX_IMAGE,
        "image_ref": DEFAULT_LOCAL_SANDBOX_IMAGE,
        "features": ["java@21", "node@24", "rust@stable", "go@1.26"],
        "backend": LOCAL_SANDBOX_BACKEND,
        "status": local_docker_image_status(DEFAULT_LOCAL_SANDBOX_IMAGE).await,
    })];
    for job in jobs.iter().filter(|job| job.status == "succeeded") {
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

pub(crate) async fn start_local_sandbox_image_job(
    runtime: &LocalRuntime,
    features: Vec<String>,
    custom_build_script: Option<String>,
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
    let image_id = local_sandbox_image_id(features.as_slice(), custom_build_script.as_deref());
    if let Some(existing) = runtime
        .sandbox_runtime
        .jobs
        .read()
        .await
        .iter()
        .find(|job| job.image_id == image_id && job.status == "running")
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
