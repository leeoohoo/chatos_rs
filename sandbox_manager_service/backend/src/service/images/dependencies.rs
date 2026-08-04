// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

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

pub(super) fn dependency_image_job(
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

pub(super) async fn run_dependency_image_job(
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

pub(super) fn dependency_prepare_record(
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

pub(super) fn dependency_image_id(image_ref: &str) -> String {
    format!("dependency:{}", image_ref.trim())
}

pub(super) fn dependency_image_name(image_ref: &str) -> String {
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

pub(super) fn dependency_pull_failure_detail(output: &str, fallback: &str) -> String {
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

pub(super) fn known_dependency_image_refs() -> &'static [&'static str] {
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
