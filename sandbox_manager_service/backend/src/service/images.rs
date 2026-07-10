// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};
use std::process::Stdio;
use std::sync::Arc;

use base64::engine::general_purpose;
use base64::Engine as _;
use chatos_sandbox_image_mcp::custom_build_script_hash;
use chrono::Utc;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::config::{AppConfig, SandboxBackendKind};
use crate::models::{SandboxImageCatalogResponse, SandboxImageJobRecord, SandboxImageRecord};

use super::image_specs::{self, RuntimeSelectionSpec};

const DEFAULT_IMAGE_ID: &str = "default";
const JOB_STATUS_RUNNING: &str = "running";
const JOB_STATUS_SUCCEEDED: &str = "succeeded";
const JOB_STATUS_FAILED: &str = "failed";
const MAX_JOB_OUTPUT_LEN: usize = 80_000;
const MAX_CUSTOM_BUILD_SCRIPT_LEN: usize = 128 * 1024;

#[derive(Debug, Clone, Default)]
pub(crate) struct ImageJobStore {
    jobs: Arc<RwLock<HashMap<String, SandboxImageJobRecord>>>,
}

impl ImageJobStore {
    pub(crate) async fn insert(&self, job: SandboxImageJobRecord) {
        self.jobs.write().await.insert(job.id.clone(), job);
    }

    pub(crate) async fn list(&self) -> Vec<SandboxImageJobRecord> {
        let mut jobs = self.jobs.read().await.values().cloned().collect::<Vec<_>>();
        jobs.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        jobs
    }

    async fn active_for_image(
        &self,
        image_id: &str,
        project_id: Option<&str>,
        run_id: Option<&str>,
    ) -> Option<SandboxImageJobRecord> {
        self.jobs
            .read()
            .await
            .values()
            .find(|job| {
                job.image_id == image_id
                    && job.status == JOB_STATUS_RUNNING
                    && job.project_id.as_deref() == project_id
                    && job.run_id.as_deref() == run_id
            })
            .cloned()
    }

    async fn update<F>(&self, job_id: &str, update: F)
    where
        F: FnOnce(&mut SandboxImageJobRecord),
    {
        if let Some(job) = self.jobs.write().await.get_mut(job_id) {
            update(job);
            job.updated_at = now_rfc3339();
        }
    }
}

#[derive(Debug, Clone)]
struct ImageBuildSpec {
    record: SandboxImageRecord,
    install_features: Vec<String>,
    custom_build_script: Option<String>,
}

pub(crate) async fn catalog(
    config: &AppConfig,
    backend: SandboxBackendKind,
) -> SandboxImageCatalogResponse {
    let local_refs = local_image_refs(config, backend).await;
    let mut images = Vec::new();
    let mut default_record = default_image_record(config, backend);
    apply_catalog_status(backend, &local_refs, &mut default_record);
    images.push(default_record);

    if let Ok(refs) = &local_refs {
        let mut local_images = refs
            .iter()
            .filter_map(|image_ref| local_image_record(config, backend, image_ref))
            .collect::<Vec<_>>();
        local_images.sort_by(|left, right| left.name.cmp(&right.name));
        images.extend(local_images);
    }

    SandboxImageCatalogResponse {
        backend: backend.as_str().to_string(),
        default_image_id: DEFAULT_IMAGE_ID.to_string(),
        image_tag_prefix: normalized_tag_prefix(config),
        features: image_specs::catalog_features(),
        images,
    }
}

pub(crate) async fn start_initialize_job(
    jobs: ImageJobStore,
    config: &AppConfig,
    backend: SandboxBackendKind,
    features: &[String],
    custom_build_script: Option<&str>,
    project_id: Option<&str>,
    run_id: Option<&str>,
) -> Result<SandboxImageJobRecord, String> {
    let feature_specs = image_specs::canonical_features(features)?;
    let custom_build_script = normalize_custom_build_script(custom_build_script)?;
    let custom_script_hash = custom_build_script.as_deref().map(custom_build_script_hash);
    let image = generated_image_record(
        config,
        backend,
        &feature_specs,
        custom_script_hash.as_deref(),
    );
    let install_features = feature_specs
        .iter()
        .map(image_specs::selection_feature_token)
        .collect::<Vec<_>>();

    let project_id = normalize_job_context(project_id);
    let run_id = normalize_job_context(run_id);
    if let Some(job) = jobs
        .active_for_image(image.id.as_str(), project_id.as_deref(), run_id.as_deref())
        .await
    {
        return Ok(job);
    }

    let now = now_rfc3339();
    let job = SandboxImageJobRecord {
        id: format!("image-job-{}", Uuid::new_v4()),
        image_id: image.id.clone(),
        image_name: image.name.clone(),
        image_ref: image.image_ref.clone(),
        features: image.features.clone(),
        backend: backend.as_str().to_string(),
        status: JOB_STATUS_RUNNING.to_string(),
        created_at: now.clone(),
        updated_at: now,
        started_at: Some(now_rfc3339()),
        finished_at: None,
        output: String::new(),
        error: None,
        project_id,
        run_id,
    };
    jobs.insert(job.clone()).await;

    let job_id = job.id.clone();
    let config = config.clone();
    let build = ImageBuildSpec {
        record: image,
        install_features,
        custom_build_script,
    };
    tokio::spawn(async move {
        run_initialize_job(jobs, config, backend, job_id, build).await;
    });

    Ok(job)
}

fn normalize_job_context(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

async fn run_initialize_job(
    jobs: ImageJobStore,
    config: AppConfig,
    backend: SandboxBackendKind,
    job_id: String,
    build: ImageBuildSpec,
) {
    if matches!(backend, SandboxBackendKind::Mock) {
        jobs.update(job_id.as_str(), |job| {
            job.status = JOB_STATUS_SUCCEEDED.to_string();
            job.finished_at = Some(now_rfc3339());
            append_job_output(job, "mock backend does not build container images\n");
        })
        .await;
        return;
    }

    let cli = container_cli(&config, backend).to_string();
    jobs.update(job_id.as_str(), |job| {
        append_job_output(
            job,
            &format!("starting image build: {}\n", build.record.image_ref),
        );
        if build.custom_build_script.is_some() {
            append_job_output(job, "custom build script is enabled\n");
        }
    })
    .await;

    let custom_script_b64 = build
        .custom_build_script
        .as_deref()
        .map(|script| general_purpose::STANDARD.encode(script.as_bytes()));
    let mut command = Command::new(&cli);
    command
        .arg("build")
        .arg("-t")
        .arg(&build.record.image_ref)
        .arg("-f")
        .arg(&config.image_dockerfile)
        .arg("--build-arg")
        .arg(format!(
            "SANDBOX_FEATURES={}",
            build.install_features.join(",")
        ));
    if let Some(custom_script_b64) = &custom_script_b64 {
        command
            .arg("--build-arg")
            .arg(format!("SANDBOX_CUSTOM_SCRIPT_B64={custom_script_b64}"));
    }
    command
        .arg(&config.image_build_context)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            jobs.update(job_id.as_str(), |job| {
                job.status = JOB_STATUS_FAILED.to_string();
                job.finished_at = Some(now_rfc3339());
                job.error = Some(format!("start image build failed with {cli}: {err}"));
                append_job_output(
                    job,
                    &format!("start image build failed with {cli}: {err}\n"),
                );
            })
            .await;
            return;
        }
    };

    let stdout_reader = child
        .stdout
        .take()
        .map(|stdout| tokio::spawn(read_job_output(jobs.clone(), job_id.clone(), stdout)));
    let stderr_reader = child
        .stderr
        .take()
        .map(|stderr| tokio::spawn(read_job_output(jobs.clone(), job_id.clone(), stderr)));

    let status = child.wait().await;
    if let Some(reader) = stdout_reader {
        let _ = reader.await;
    }
    if let Some(reader) = stderr_reader {
        let _ = reader.await;
    }

    match status {
        Ok(status) if status.success() => {
            jobs.update(job_id.as_str(), |job| {
                job.status = JOB_STATUS_SUCCEEDED.to_string();
                job.finished_at = Some(now_rfc3339());
                append_job_output(job, "image build completed\n");
            })
            .await;
        }
        Ok(status) => {
            jobs.update(job_id.as_str(), |job| {
                job.status = JOB_STATUS_FAILED.to_string();
                job.finished_at = Some(now_rfc3339());
                job.error = Some(format!("image build exited with {status}"));
                append_job_output(job, &format!("image build exited with {status}\n"));
            })
            .await;
        }
        Err(err) => {
            jobs.update(job_id.as_str(), |job| {
                job.status = JOB_STATUS_FAILED.to_string();
                job.finished_at = Some(now_rfc3339());
                job.error = Some(format!("wait image build failed: {err}"));
                append_job_output(job, &format!("wait image build failed: {err}\n"));
            })
            .await;
        }
    }
}

async fn read_job_output<R>(jobs: ImageJobStore, job_id: String, stream: R)
where
    R: AsyncRead + Unpin,
{
    let mut lines = BufReader::new(stream).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                jobs.update(job_id.as_str(), |job| {
                    append_job_output(job, &line);
                    append_job_output(job, "\n");
                })
                .await;
            }
            Ok(None) => break,
            Err(err) => {
                jobs.update(job_id.as_str(), |job| {
                    append_job_output(job, &format!("read image build output failed: {err}\n"));
                })
                .await;
                break;
            }
        }
    }
}

pub(crate) async fn resolve_for_create(
    config: &AppConfig,
    backend: SandboxBackendKind,
    image_id: Option<&str>,
) -> Result<SandboxImageRecord, String> {
    let image_id = image_id.map(str::trim).filter(|value| !value.is_empty());
    let Some(image_id) = image_id else {
        return Ok(default_image_record(config, backend));
    };
    if image_id == DEFAULT_IMAGE_ID {
        return Ok(default_image_record(config, backend));
    }

    let mut record = generated_image_record_for_id(config, backend, image_id)
        .ok_or_else(|| format!("unknown sandbox image id: {image_id}"))?;
    apply_status(config, backend, &mut record).await;
    if !record.initialized {
        return Err(format!(
            "sandbox image {} is not initialized; initialize it before creating a sandbox",
            record.name
        ));
    }
    Ok(record)
}

async fn apply_status(
    config: &AppConfig,
    backend: SandboxBackendKind,
    image: &mut SandboxImageRecord,
) {
    if matches!(backend, SandboxBackendKind::Mock) {
        image.initialized = true;
        image.status = "mock".to_string();
        return;
    }

    match image_exists(config, backend, image.image_ref.as_str()).await {
        Ok(true) => {
            image.initialized = true;
            image.status = "ready".to_string();
        }
        Ok(false) => {
            image.initialized = false;
            image.status = "missing".to_string();
        }
        Err(err) => {
            image.initialized = false;
            image.status = format!("inspect_error: {err}");
        }
    }
}

fn apply_catalog_status(
    backend: SandboxBackendKind,
    local_refs: &Result<HashSet<String>, String>,
    image: &mut SandboxImageRecord,
) {
    if matches!(backend, SandboxBackendKind::Mock) {
        image.initialized = true;
        image.status = "mock".to_string();
        return;
    }

    match local_refs {
        Ok(refs) if refs.contains(image.image_ref.as_str()) => {
            image.initialized = true;
            image.status = "ready".to_string();
        }
        Ok(_) => {
            image.initialized = false;
            image.status = "missing".to_string();
        }
        Err(err) => {
            image.initialized = false;
            image.status = format!("inspect_error: {err}");
        }
    }
}

async fn local_image_refs(
    config: &AppConfig,
    backend: SandboxBackendKind,
) -> Result<HashSet<String>, String> {
    if matches!(backend, SandboxBackendKind::Mock) {
        return Ok(HashSet::new());
    }
    let cli = container_cli(config, backend);
    let output = Command::new(cli)
        .arg("image")
        .arg("ls")
        .arg("--format")
        .arg("{{.Repository}}:{{.Tag}}")
        .output()
        .await
        .map_err(|err| format!("{cli} image ls failed: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "{cli} image ls failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.contains("<none>"))
        .map(ToOwned::to_owned)
        .collect())
}

async fn image_exists(
    config: &AppConfig,
    backend: SandboxBackendKind,
    image_ref: &str,
) -> Result<bool, String> {
    let cli = container_cli(config, backend);
    let output = Command::new(cli)
        .arg("image")
        .arg("inspect")
        .arg(image_ref)
        .output()
        .await
        .map_err(|err| format!("{cli} image inspect failed: {err}"))?;
    Ok(output.status.success())
}

fn default_image_record(config: &AppConfig, backend: SandboxBackendKind) -> SandboxImageRecord {
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

fn generated_image_record(
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

fn generated_image_record_for_id(
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

fn local_image_record(
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

fn normalize_custom_build_script(script: Option<&str>) -> Result<Option<String>, String> {
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

fn generated_image_id(feature_ids: &[String], custom_script_hash: Option<&str>) -> String {
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

fn normalized_tag_prefix(config: &AppConfig) -> String {
    let prefix = config.image_tag_prefix.trim();
    if prefix.is_empty() {
        "chatos-sandbox-agent".to_string()
    } else {
        prefix.trim_end_matches(':').to_string()
    }
}

fn default_image_ref(config: &AppConfig, backend: SandboxBackendKind) -> String {
    match backend {
        SandboxBackendKind::Kata => config.kata_image.clone(),
        SandboxBackendKind::Docker | SandboxBackendKind::Mock => config.docker_image.clone(),
    }
}

fn container_cli(config: &AppConfig, backend: SandboxBackendKind) -> &str {
    match backend {
        SandboxBackendKind::Kata => config.kata_container_cli.as_str(),
        SandboxBackendKind::Docker | SandboxBackendKind::Mock => "docker",
    }
}

fn append_job_output(job: &mut SandboxImageJobRecord, text: &str) {
    job.output.push_str(text);
    if job.output.len() > MAX_JOB_OUTPUT_LEN {
        let keep_from = job.output.len().saturating_sub(MAX_JOB_OUTPUT_LEN);
        job.output = format!("...{}", &job.output[keep_from..]);
    }
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}
