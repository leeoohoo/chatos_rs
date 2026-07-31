// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeSet, HashMap, HashSet};
use std::process::Stdio;
use std::sync::Arc;

use base64::engine::general_purpose;
use base64::Engine as _;
use chatos_mcp::sandbox_images::custom_build_script_hash;
use chrono::Utc;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::config::{AppConfig, SandboxBackendKind};
use crate::models::{
    PrepareSandboxDependencyImagesResponse, PreparedSandboxDependencyImageRecord,
    SandboxImageCatalogResponse, SandboxImageJobRecord, SandboxImageRecord,
};

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

    async fn active_for_image(&self, image_id: &str) -> Option<SandboxImageJobRecord> {
        self.jobs
            .read()
            .await
            .values()
            .find(|job| job.image_id == image_id && job.status == JOB_STATUS_RUNNING)
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

    for image_ref in known_dependency_image_refs() {
        let mut dependency = dependency_catalog_image_record(backend, image_ref);
        apply_catalog_status(backend, &local_refs, &mut dependency);
        images.push(dependency);
    }

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

pub(crate) async fn prepare_dependency_images(
    jobs: ImageJobStore,
    config: &AppConfig,
    backend: SandboxBackendKind,
    image_refs: &[String],
    project_id: Option<&str>,
    run_id: Option<&str>,
) -> Result<PrepareSandboxDependencyImagesResponse, String> {
    if image_refs.len() > 64 {
        return Err("too many dependency images; maximum is 64".to_string());
    }
    let refs = image_refs
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>();
    for image_ref in &refs {
        if !known_dependency_image_ref(image_ref) {
            return Err(format!(
                "dependency image_ref is not platform-managed: {image_ref}"
            ));
        }
    }

    let project_id = normalize_job_context(project_id);
    let run_id = normalize_job_context(run_id);
    let mut tasks = tokio::task::JoinSet::new();
    for (position, image_ref) in refs.into_iter().enumerate() {
        let config = config.clone();
        let jobs = jobs.clone();
        let dependency_run_id = run_id
            .as_ref()
            .map(|run_id| format!("{run_id}_dependency_{position}"));
        let dependency_id = dependency_image_id(image_ref.as_str());
        if let Some(job) = jobs.active_for_image(dependency_id.as_str()).await {
            tasks.spawn(async move {
                dependency_prepare_record(image_ref, job.id, true, "running", None)
            });
            continue;
        }
        if matches!(backend, SandboxBackendKind::Mock)
            || image_exists(&config, backend, image_ref.as_str())
                .await
                .unwrap_or(false)
        {
            let mut job = dependency_image_job(
                backend,
                image_ref.as_str(),
                project_id.clone(),
                dependency_run_id,
            );
            job.status = JOB_STATUS_SUCCEEDED.to_string();
            job.finished_at = Some(now_rfc3339());
            let status = if matches!(backend, SandboxBackendKind::Mock) {
                append_job_output(
                    &mut job,
                    &format!(
                        "mock backend treats dependency image as already available; skip pull: {image_ref}\n"
                    ),
                );
                "mock"
            } else {
                append_job_output(
                    &mut job,
                    &format!("dependency image already exists locally; skip pull: {image_ref}\n"),
                );
                "ready"
            };
            let job_id = job.id.clone();
            jobs.insert(job).await;
            tasks.spawn(
                async move { dependency_prepare_record(image_ref, job_id, true, status, None) },
            );
            continue;
        }
        let job = dependency_image_job(
            backend,
            image_ref.as_str(),
            project_id.clone(),
            dependency_run_id,
        );
        let job_id = job.id.clone();
        jobs.insert(job).await;
        tasks.spawn(async move {
            run_dependency_image_job(jobs, config, backend, job_id, image_ref).await
        });
    }
    let mut images = Vec::new();
    while let Some(result) = tasks.join_next().await {
        images.push(result.map_err(|err| format!("dependency image task failed: {err}"))?);
    }
    images.sort_by(|left, right| left.image_ref.cmp(&right.image_ref));
    Ok(PrepareSandboxDependencyImagesResponse { images })
}

fn dependency_image_job(
    backend: SandboxBackendKind,
    image_ref: &str,
    project_id: Option<String>,
    run_id: Option<String>,
) -> SandboxImageJobRecord {
    let now = now_rfc3339();
    SandboxImageJobRecord {
        id: format!("dependency-image-job-{}", Uuid::new_v4()),
        image_id: dependency_image_id(image_ref),
        image_name: dependency_image_name(image_ref),
        image_ref: image_ref.to_string(),
        features: vec![format!("dependency@{image_ref}")],
        backend: backend.as_str().to_string(),
        status: JOB_STATUS_RUNNING.to_string(),
        created_at: now.clone(),
        updated_at: now.clone(),
        started_at: Some(now),
        finished_at: None,
        output: String::new(),
        error: None,
        project_id,
        run_id,
    }
}

async fn run_dependency_image_job(
    jobs: ImageJobStore,
    config: AppConfig,
    backend: SandboxBackendKind,
    job_id: String,
    image_ref: String,
) -> PreparedSandboxDependencyImageRecord {
    if matches!(backend, SandboxBackendKind::Mock) {
        jobs.update(job_id.as_str(), |job| {
            job.status = JOB_STATUS_SUCCEEDED.to_string();
            job.finished_at = Some(now_rfc3339());
            append_job_output(job, "mock backend does not pull dependency images\n");
        })
        .await;
        return dependency_prepare_record(image_ref, job_id, true, "mock", None);
    }

    let cli = container_cli(&config, backend).to_string();
    jobs.update(job_id.as_str(), |job| {
        append_job_output(
            job,
            &format!("starting dependency image pull: {image_ref}\n"),
        );
    })
    .await;

    match image_exists(&config, backend, image_ref.as_str()).await {
        Ok(true) => {
            jobs.update(job_id.as_str(), |job| {
                job.status = JOB_STATUS_SUCCEEDED.to_string();
                job.finished_at = Some(now_rfc3339());
                append_job_output(
                    job,
                    &format!("dependency image already exists locally: {image_ref}\n"),
                );
            })
            .await;
            return dependency_prepare_record(image_ref, job_id, true, "ready", None);
        }
        Ok(false) => {}
        Err(error) => {
            let error = format!("inspect dependency image {image_ref} failed: {error}");
            jobs.update(job_id.as_str(), |job| {
                job.status = JOB_STATUS_FAILED.to_string();
                job.finished_at = Some(now_rfc3339());
                job.error = Some(error.clone());
                append_job_output(job, &format!("{error}\n"));
            })
            .await;
            return dependency_prepare_record(image_ref, job_id, false, "failed", Some(error));
        }
    }

    let mut command = Command::new(&cli);
    command
        .arg("pull")
        .arg(image_ref.as_str())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            let error = format!("{cli} pull {image_ref} failed to start: {err}");
            jobs.update(job_id.as_str(), |job| {
                job.status = JOB_STATUS_FAILED.to_string();
                job.finished_at = Some(now_rfc3339());
                job.error = Some(error.clone());
                append_job_output(job, &format!("{error}\n"));
            })
            .await;
            return dependency_prepare_record(image_ref, job_id, false, "failed", Some(error));
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
                append_job_output(job, "dependency image pull completed\n");
            })
            .await;
            dependency_prepare_record(image_ref, job_id, false, "ready", None)
        }
        Ok(status) => {
            let fallback = format!("{cli} pull {image_ref} exited with {status}");
            let mut detail = fallback.clone();
            jobs.update(job_id.as_str(), |job| {
                let error = dependency_pull_failure_detail(job.output.as_str(), fallback.as_str());
                detail = error.clone();
                job.status = JOB_STATUS_FAILED.to_string();
                job.finished_at = Some(now_rfc3339());
                job.error = Some(error);
                append_job_output(job, &format!("{fallback}\n"));
            })
            .await;
            dependency_prepare_record(image_ref, job_id, false, "failed", Some(detail))
        }
        Err(err) => {
            let error = format!("wait dependency image pull failed for {image_ref}: {err}");
            jobs.update(job_id.as_str(), |job| {
                job.status = JOB_STATUS_FAILED.to_string();
                job.finished_at = Some(now_rfc3339());
                job.error = Some(error.clone());
                append_job_output(job, &format!("{error}\n"));
            })
            .await;
            dependency_prepare_record(image_ref, job_id, false, "failed", Some(error))
        }
    }
}

fn dependency_prepare_record(
    image_ref: String,
    job_id: String,
    reused: bool,
    status: &str,
    error: Option<String>,
) -> PreparedSandboxDependencyImageRecord {
    PreparedSandboxDependencyImageRecord {
        image_ref,
        reused,
        status: status.to_string(),
        job_id: (!job_id.is_empty()).then_some(job_id),
        error,
    }
}

fn dependency_image_id(image_ref: &str) -> String {
    format!("dependency:{}", image_ref.trim())
}

fn dependency_image_name(image_ref: &str) -> String {
    let name = match image_ref.trim() {
        "postgres:16-alpine" => "PostgreSQL",
        "mysql:8.4" => "MySQL",
        "mongo:7.0" => "MongoDB",
        "redis:7-alpine" => "Redis",
        "nacos/nacos-server:v2.4.3" => "Nacos",
        "rabbitmq:3.13-management-alpine" => "RabbitMQ",
        "bitnami/kafka:3.7" => "Kafka",
        "docker.elastic.co/elasticsearch/elasticsearch:8.14.3" => "Elasticsearch",
        "minio/minio:latest" => "MinIO",
        value => value
            .split('/')
            .next_back()
            .unwrap_or(value)
            .split(':')
            .next()
            .unwrap_or(value),
    };
    name.to_string()
}

fn dependency_pull_failure_detail(output: &str, fallback: &str) -> String {
    output
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| format!("{fallback}: {line}"))
        .unwrap_or_else(|| fallback.to_string())
}

pub(crate) fn known_dependency_image_ref(value: &str) -> bool {
    known_dependency_image_refs().contains(&value.trim())
}

fn known_dependency_image_refs() -> &'static [&'static str] {
    &[
        "postgres:16-alpine",
        "mysql:8.4",
        "mongo:7.0",
        "redis:7-alpine",
        "nacos/nacos-server:v2.4.3",
        "rabbitmq:3.13-management-alpine",
        "bitnami/kafka:3.7",
        "docker.elastic.co/elasticsearch/elasticsearch:8.14.3",
        "minio/minio:latest",
    ]
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
    if let Some(job) = jobs.active_for_image(image.id.as_str()).await {
        return Ok(job);
    }
    if matches!(backend, SandboxBackendKind::Mock)
        || image_exists(config, backend, image.image_ref.as_str())
            .await
            .unwrap_or(false)
    {
        let job = completed_initialize_job(
            backend,
            &image,
            project_id,
            run_id,
            if matches!(backend, SandboxBackendKind::Mock) {
                "mock backend image is already available; skip initialization"
            } else {
                "sandbox image already exists locally; skip initialization"
            },
        );
        jobs.insert(job.clone()).await;
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

fn completed_initialize_job(
    backend: SandboxBackendKind,
    image: &SandboxImageRecord,
    project_id: Option<String>,
    run_id: Option<String>,
    output: &str,
) -> SandboxImageJobRecord {
    let now = now_rfc3339();
    SandboxImageJobRecord {
        id: format!("image-job-reused-{}", Uuid::new_v4()),
        image_id: image.id.clone(),
        image_name: image.name.clone(),
        image_ref: image.image_ref.clone(),
        features: image.features.clone(),
        backend: backend.as_str().to_string(),
        status: JOB_STATUS_SUCCEEDED.to_string(),
        created_at: now.clone(),
        updated_at: now.clone(),
        started_at: Some(now.clone()),
        finished_at: Some(now),
        output: format!("{output}: {}\n", image.image_ref),
        error: None,
        project_id,
        run_id,
    }
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
    let previous_image_id = image_id_for_ref(&cli, build.record.image_ref.as_str()).await;
    let mut command = Command::new(&cli);
    command
        .arg("build")
        .arg("--force-rm")
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
            cleanup_replaced_image(
                jobs.clone(),
                job_id.as_str(),
                cli.as_str(),
                build.record.image_ref.as_str(),
                previous_image_id,
            )
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

async fn cleanup_replaced_image(
    jobs: ImageJobStore,
    job_id: &str,
    cli: &str,
    image_ref: &str,
    previous_image_id: Option<String>,
) {
    let Some(previous_image_id) = previous_image_id else {
        return;
    };
    let Some(current_image_id) = image_id_for_ref(cli, image_ref).await else {
        jobs.update(job_id, |job| {
            append_job_output(
                job,
                &format!("skip old image cleanup: cannot inspect rebuilt image {image_ref}\n"),
            );
        })
        .await;
        return;
    };
    if image_ids_equal(previous_image_id.as_str(), current_image_id.as_str()) {
        return;
    }
    let output = Command::new(cli)
        .args(["image", "rm", previous_image_id.as_str()])
        .output()
        .await;
    jobs.update(job_id, |job| match output {
        Ok(output) if output.status.success() => {
            append_job_output(
                job,
                &format!("removed replaced dangling image {previous_image_id}\n"),
            );
        }
        Ok(output) => {
            append_job_output(
                job,
                &format!(
                    "old image cleanup skipped for {previous_image_id}: {}\n",
                    String::from_utf8_lossy(output.stderr.as_slice()).trim()
                ),
            );
        }
        Err(err) => {
            append_job_output(
                job,
                &format!("old image cleanup failed for {previous_image_id}: {err}\n"),
            );
        }
    })
    .await;
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
    let mut record = match image_id {
        None | Some(DEFAULT_IMAGE_ID) => default_image_record(config, backend),
        Some(image_id) => generated_image_record_for_id(config, backend, image_id)
            .ok_or_else(|| format!("unknown sandbox image id: {image_id}"))?,
    };
    apply_status(config, backend, &mut record).await;
    ensure_image_ready_for_create(&record)?;
    Ok(record)
}

fn ensure_image_ready_for_create(record: &SandboxImageRecord) -> Result<(), String> {
    if record.initialized {
        return Ok(());
    }
    Err(format!(
        "sandbox image {} is not initialized; initialize it before creating a sandbox",
        record.name
    ))
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

async fn image_id_for_ref(cli: &str, image_ref: &str) -> Option<String> {
    let output = Command::new(cli)
        .args(["image", "inspect", "--format", "{{.Id}}", image_ref])
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(output.stdout.as_slice())
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

fn image_ids_equal(left: &str, right: &str) -> bool {
    left.trim().trim_start_matches("sha256:") == right.trim().trim_start_matches("sha256:")
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

fn dependency_catalog_image_record(
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

fn dependency_image_description(image_ref: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_rejects_an_uninitialized_default_image() {
        let config = AppConfig::from_env().expect("sandbox config");
        let record = default_image_record(&config, SandboxBackendKind::Docker);
        let error = ensure_image_ready_for_create(&record)
            .expect_err("missing default image must not be used for a lease");
        assert!(error.contains("Default"));
        assert!(error.contains("not initialized"));
    }

    #[tokio::test]
    async fn mock_dependency_preparation_reuses_all_platform_images() {
        let config = AppConfig::from_env().expect("sandbox config");
        let jobs = ImageJobStore::default();
        let result = prepare_dependency_images(
            jobs.clone(),
            &config,
            SandboxBackendKind::Mock,
            &[
                "redis:7-alpine".to_string(),
                "mysql:8.4".to_string(),
                "redis:7-alpine".to_string(),
            ],
            Some("project-1"),
            Some("run-1"),
        )
        .await
        .expect("prepare mock dependency images");
        assert_eq!(result.images.len(), 2);
        assert!(result.images.iter().all(|image| image.reused));
        assert!(result.images.iter().all(|image| image.status == "mock"));
        assert!(result.images.iter().all(|image| image.job_id.is_some()));

        let jobs = jobs.list().await;
        assert_eq!(jobs.len(), 2);
        assert!(jobs
            .iter()
            .all(|job| job.project_id.as_deref() == Some("project-1")));
        assert!(jobs.iter().all(|job| job
            .run_id
            .as_deref()
            .is_some_and(|run_id| run_id.starts_with("run-1_dependency_"))));
        assert!(jobs.iter().all(|job| job.status == JOB_STATUS_SUCCEEDED));
        assert!(jobs.iter().all(|job| job.output.contains("skip pull")));
    }

    #[tokio::test]
    async fn mock_initialize_records_reused_request_without_starting_build() {
        let config = AppConfig::from_env().expect("sandbox config");
        let jobs = ImageJobStore::default();

        let first = start_initialize_job(
            jobs.clone(),
            &config,
            SandboxBackendKind::Mock,
            &["node@24".to_string(), "python@3.11".to_string()],
            None,
            Some("project-1"),
            Some("run-1"),
        )
        .await
        .expect("mock initialize image");
        let second = start_initialize_job(
            jobs.clone(),
            &config,
            SandboxBackendKind::Mock,
            &["node@24".to_string(), "python@3.11".to_string()],
            None,
            Some("project-1"),
            Some("run-2"),
        )
        .await
        .expect("mock initialize image again");

        assert_eq!(first.image_id, second.image_id);
        assert_eq!(first.status, JOB_STATUS_SUCCEEDED);
        assert_eq!(second.status, JOB_STATUS_SUCCEEDED);
        assert!(first.output.contains("skip initialization"));
        assert!(second.output.contains("skip initialization"));
        let jobs = jobs.list().await;
        assert_eq!(jobs.len(), 2);
        assert!(jobs.iter().all(|job| job.status == JOB_STATUS_SUCCEEDED));
        assert!(jobs
            .iter()
            .all(|job| job.output.contains("skip initialization")));
    }

    #[tokio::test]
    async fn catalog_lists_platform_dependency_images_for_reuse_visibility() {
        let config = AppConfig::from_env().expect("sandbox config");
        let catalog = catalog(&config, SandboxBackendKind::Mock).await;

        let postgres = catalog
            .images
            .iter()
            .find(|image| image.image_ref == "postgres:16-alpine")
            .expect("PostgreSQL dependency image is visible in catalog");
        assert_eq!(postgres.id, "dependency:postgres:16-alpine");
        assert_eq!(postgres.status, "mock");
        assert!(!postgres.buildable);
        assert!(postgres
            .features
            .contains(&"dependency@postgres:16-alpine".to_string()));
    }

    #[tokio::test]
    async fn dependency_preparation_rejects_unmanaged_image_refs() {
        let config = AppConfig::from_env().expect("sandbox config");
        let error = prepare_dependency_images(
            ImageJobStore::default(),
            &config,
            SandboxBackendKind::Mock,
            &["untrusted.example/database:latest".to_string()],
            None,
            None,
        )
        .await
        .expect_err("unmanaged dependency image must be rejected");
        assert!(error.contains("not platform-managed"));
    }
}

fn append_job_output(job: &mut SandboxImageJobRecord, text: &str) {
    job.output.push_str(text);
    if job.output.len() > MAX_JOB_OUTPUT_LEN {
        let tail_bytes = MAX_JOB_OUTPUT_LEN.saturating_sub(3);
        let desired_start = job.output.len().saturating_sub(tail_bytes);
        let keep_from = job
            .output
            .char_indices()
            .find_map(|(index, _)| (index >= desired_start).then_some(index))
            .unwrap_or(job.output.len());
        job.output = format!("...{}", &job.output[keep_from..]);
    }
}

#[cfg(test)]
mod output_tests {
    use super::*;

    #[test]
    fn job_output_truncation_preserves_utf8_boundaries() {
        let mut job = SandboxImageJobRecord {
            id: "job-1".to_string(),
            image_id: "image-1".to_string(),
            image_name: "image-1".to_string(),
            image_ref: "image-1:latest".to_string(),
            features: Vec::new(),
            backend: "docker".to_string(),
            status: "running".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
            started_at: Some("now".to_string()),
            finished_at: None,
            output: "→".repeat(MAX_JOB_OUTPUT_LEN / 3),
            error: None,
            project_id: Some("project-1".to_string()),
            run_id: Some("run-1".to_string()),
        };

        append_job_output(&mut job, "→ done");

        assert!(job.output.starts_with("..."));
        assert!(job.output.ends_with("→ done"));
        assert!(job.output.len() <= MAX_JOB_OUTPUT_LEN);
    }

    #[test]
    fn image_id_comparison_ignores_sha256_prefix() {
        assert!(image_ids_equal("sha256:abc123", "abc123"));
        assert!(!image_ids_equal("sha256:abc123", "sha256:def456"));
    }
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}
