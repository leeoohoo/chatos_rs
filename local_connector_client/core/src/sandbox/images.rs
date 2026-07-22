// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::sandbox::catalog::local_sandbox_runtime_specs;
use crate::sandbox::docker::docker_command;
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
    let selected_image_ref = runtime
        .state
        .read()
        .await
        .sandbox
        .selected_image_ref
        .clone()
        .unwrap_or_else(|| DEFAULT_LOCAL_SANDBOX_IMAGE.to_string());
    let selected_record = stored_images
        .iter()
        .find(|image| image.image_ref == selected_image_ref);
    let default_features = selected_record
        .map(|image| image.features.clone())
        .unwrap_or_else(default_local_sandbox_features);
    let default_rebuildable = selected_record.is_none_or(sandbox_image_record_rebuildable);
    let mut seen_refs = std::collections::BTreeSet::new();
    seen_refs.insert(selected_image_ref.clone());
    let mut images = vec![json!({
        "id": "default",
        "name": selected_image_ref,
        "image_ref": selected_image_ref,
        "features": default_features,
        "backend": LOCAL_SANDBOX_BACKEND,
        "status": local_docker_image_status(selected_image_ref.as_str()).await,
        "rebuildable": default_rebuildable,
    })];
    for image in stored_images {
        if !seen_refs.insert(image.image_ref.clone()) {
            continue;
        }
        let rebuildable = sandbox_image_record_rebuildable(&image);
        images.push(json!({
            "id": image.id,
            "name": image.image_name,
            "image_ref": image.image_ref,
            "features": image.features,
            "backend": image.backend,
            "status": local_docker_image_status(image.image_ref.as_str()).await,
            "created_at": image.created_at,
            "rebuildable": rebuildable,
        }));
    }
    for job in jobs.iter().filter(|job| job.status == "succeeded") {
        if !seen_refs.insert(job.image_ref.clone()) {
            continue;
        }
        let status = local_docker_image_status(job.image_ref.as_str()).await;
        images.push(json!({
            "id": job.image_id,
            "name": job.image_name,
            "image_ref": job.image_ref,
            "features": job.features,
            "backend": job.backend,
            "status": status,
            "created_at": job.created_at,
            "rebuildable": true,
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

pub(crate) async fn delete_local_sandbox_image(
    runtime: &LocalRuntime,
    image_id: &str,
) -> Result<Value, String> {
    let image_id = image_id.trim();
    if image_id.is_empty() {
        return Err("sandbox image id is required".to_string());
    }
    let image_ref = {
        let state = runtime.state.read().await;
        if image_id == "default" {
            state
                .sandbox
                .selected_image_ref
                .clone()
                .unwrap_or_else(|| DEFAULT_LOCAL_SANDBOX_IMAGE.to_string())
        } else {
            let record = state
                .sandbox
                .images
                .iter()
                .find(|image| image.id == image_id)
                .ok_or_else(|| format!("sandbox image not found: {image_id}"))?;
            record.image_ref.clone()
        }
    };
    let in_use = runtime
        .sandbox_runtime
        .leases
        .read()
        .await
        .values()
        .any(|lease| {
            lease.status != crate::LOCAL_SANDBOX_STATUS_DESTROYED
                && (lease.image_id.as_deref() == Some(image_id)
                    || lease.image_ref.as_deref() == Some(image_ref.as_str()))
        });
    if in_use {
        return Err(format!(
            "sandbox image is in use by an active lease: {image_ref}"
        ));
    }
    let build_running = runtime
        .sandbox_runtime
        .jobs
        .read()
        .await
        .iter()
        .any(|job| job.status == "running" && job.image_ref == image_ref);
    if build_running {
        return Err(format!(
            "sandbox image is being initialized by an active build: {image_ref}"
        ));
    }
    let output = docker_command()
        .args(["image", "rm", image_ref.as_str()])
        .output()
        .await
        .map_err(|err| format!("remove docker image failed: {err}"))?;
    let stderr = String::from_utf8_lossy(output.stderr.as_slice());
    if !output.status.success() && !stderr.contains("No such image") {
        return Err(format!("docker image rm failed: {}", stderr.trim()));
    }
    runtime
        .sandbox_runtime
        .jobs
        .write()
        .await
        .retain(|job| job.image_ref != image_ref);
    let mut state = runtime.state.write().await;
    remove_local_sandbox_image_state(&mut state, image_id, image_ref.as_str());
    state
        .save(runtime.state_path.as_path())
        .map_err(|err| format!("save local sandbox state failed: {err}"))?;
    Ok(json!({ "ok": true, "image_id": image_id, "image_ref": image_ref }))
}

pub(crate) async fn reinitialize_local_sandbox_image(
    runtime: &LocalRuntime,
    image_id: &str,
) -> Result<LocalSandboxImageJob, String> {
    let image_id = image_id.trim();
    let (features, custom_build_script) = if image_id == "default" {
        let state = runtime.state.read().await;
        let selected = state.sandbox.selected_image_ref.as_deref();
        if let Some(record) = selected.and_then(|selected| {
            state
                .sandbox
                .images
                .iter()
                .find(|image| image.image_ref == selected)
        }) {
            rebuild_spec_from_record(record)?
        } else {
            (default_local_sandbox_features(), None)
        }
    } else {
        let state = runtime.state.read().await;
        let record = state
            .sandbox
            .images
            .iter()
            .find(|image| image.id == image_id)
            .ok_or_else(|| format!("sandbox image not found: {image_id}"))?;
        rebuild_spec_from_record(record)?
    };
    start_local_sandbox_image_job(runtime, features, custom_build_script, None, None).await
}

fn rebuild_spec_from_record(
    record: &LocalSandboxImageRecord,
) -> Result<(Vec<String>, Option<String>), String> {
    if record.features.iter().any(|feature| feature == "imported") {
        return recover_imported_sandbox_features(record.image_ref.as_str())
            .map(|features| (features, None))
            .ok_or_else(|| "imported sandbox image has no rebuild specification".to_string());
    }
    if record
        .features
        .iter()
        .any(|feature| feature.starts_with("script@"))
        && record.custom_build_script.is_none()
    {
        return Err("sandbox image was created before rebuild scripts were persisted; create a new image with the original script".to_string());
    }
    Ok((
        record
            .features
            .iter()
            .filter(|feature| !feature.starts_with("script@"))
            .cloned()
            .collect(),
        record.custom_build_script.clone(),
    ))
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
    let mut changed = false;
    if let Some(record) = imported_selected_sandbox_image(&state) {
        state.sandbox.images.push(record);
        changed = true;
    }
    for image in &mut state.sandbox.images {
        if image.features.iter().any(|feature| feature == "imported") {
            if let Some(features) = recover_imported_sandbox_features(image.image_ref.as_str()) {
                image.features = features;
                image.updated_at = local_now_rfc3339();
                changed = true;
            }
        }
    }
    if changed {
        if let Err(err) = state.save(runtime.state_path.as_path()) {
            tracing_stdout(format!("save reconciled local sandbox images failed: {err}").as_str());
        }
    }
    state.sandbox.images.clone()
}

fn default_local_sandbox_features() -> Vec<String> {
    vec![
        "java@21".to_string(),
        "node@24".to_string(),
        "rust@stable".to_string(),
        "go@1.26".to_string(),
    ]
}

fn sandbox_image_record_rebuildable(record: &LocalSandboxImageRecord) -> bool {
    rebuild_spec_from_record(record).is_ok()
}

fn remove_local_sandbox_image_state(state: &mut LocalState, image_id: &str, image_ref: &str) {
    state
        .sandbox
        .images
        .retain(|image| image.id != image_id && image.image_ref != image_ref);
    if state.sandbox.selected_image_ref.as_deref() == Some(image_ref) {
        state.sandbox.selected_image_ref = None;
    }
}

fn recover_imported_sandbox_features(image_ref: &str) -> Option<Vec<String>> {
    let (_, image_tag) = image_ref.rsplit_once(':')?;
    let (feature_slug, digest) = image_tag.rsplit_once('-')?;
    if digest.len() != 12 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }

    let mut candidates = Vec::new();
    for runtime in local_sandbox_runtime_specs() {
        let runtime_id = runtime.get("id").and_then(Value::as_str)?;
        for version in runtime.get("versions").and_then(Value::as_array)? {
            let version_id = version.get("id").and_then(Value::as_str)?;
            let feature = format!("{runtime_id}@{version_id}");
            let slug = feature.replace('@', "-").replace('.', "_");
            candidates.push((slug, feature));
        }
    }
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.0.len()));

    let mut features = Vec::new();
    if !parse_imported_feature_slug(feature_slug, candidates.as_slice(), &mut features) {
        return None;
    }
    features.sort();
    features.dedup();
    let recovered_name = local_sandbox_image_id(features.as_slice(), None);
    (recovered_name.strip_prefix("local-") == Some(image_tag)).then_some(features)
}

fn parse_imported_feature_slug(
    remaining: &str,
    candidates: &[(String, String)],
    features: &mut Vec<String>,
) -> bool {
    for (slug, feature) in candidates {
        let Some(rest) = remaining.strip_prefix(slug.as_str()) else {
            continue;
        };
        if rest.is_empty() {
            features.push(feature.clone());
            return true;
        }
        let Some(rest) = rest.strip_prefix('_') else {
            continue;
        };
        features.push(feature.clone());
        if parse_imported_feature_slug(rest, candidates, features) {
            return true;
        }
        features.pop();
    }
    false
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
        custom_build_script: None,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn record(features: Vec<&str>, script: Option<&str>) -> LocalSandboxImageRecord {
        LocalSandboxImageRecord {
            id: "image-1".to_string(),
            image_name: "image-1".to_string(),
            image_ref: "chatos-sandbox-agent:image-1".to_string(),
            features: features.into_iter().map(ToOwned::to_owned).collect(),
            custom_build_script: script.map(ToOwned::to_owned),
            backend: LOCAL_SANDBOX_BACKEND.to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }

    #[test]
    fn rebuild_spec_restores_runtime_features_and_script() {
        let record = record(vec!["node@24", "script@abcdef"], Some("apt-get update"));
        let (features, script) = rebuild_spec_from_record(&record).expect("rebuild spec");
        assert_eq!(features, vec!["node@24"]);
        assert_eq!(script.as_deref(), Some("apt-get update"));
    }

    #[test]
    fn legacy_script_image_without_persisted_script_is_not_rebuildable() {
        let record = record(vec!["node@24", "script@abcdef"], None);
        assert!(rebuild_spec_from_record(&record).is_err());
    }

    #[test]
    fn imported_generated_image_recovers_rebuild_features_from_legacy_tag() {
        let mut record = record(vec!["imported"], None);
        record.image_ref = "chatos-sandbox-agent:java-21_node-22-9c4b8e8477ca".to_string();

        let (features, script) = rebuild_spec_from_record(&record).expect("recovered spec");

        assert_eq!(features, vec!["java@21", "node@22"]);
        assert_eq!(script, None);
    }

    #[test]
    fn arbitrary_imported_image_stays_non_rebuildable() {
        let mut record = record(vec!["imported"], None);
        record.image_ref = "example.invalid/custom-sandbox:latest".to_string();

        assert!(rebuild_spec_from_record(&record).is_err());
    }

    #[test]
    fn deleting_default_image_removes_selected_record_and_selection() {
        let mut state = LocalState::default();
        let record = record(vec!["node@24"], None);
        state.sandbox.selected_image_ref = Some(record.image_ref.clone());
        state.sandbox.images.push(record.clone());

        remove_local_sandbox_image_state(&mut state, "default", record.image_ref.as_str());

        assert!(state.sandbox.images.is_empty());
        assert_eq!(state.sandbox.selected_image_ref, None);
    }
}
