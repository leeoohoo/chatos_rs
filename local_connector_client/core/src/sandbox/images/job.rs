// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use base64::engine::general_purpose;
use base64::Engine as _;
use chatos_sandbox_image_mcp::custom_build_script_feature;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

use crate::sandbox::docker::docker_command;
use crate::sandbox::types::{LocalSandboxImageJob, LocalSandboxImageRecord};
use crate::{local_now_rfc3339, tracing_stdout, LocalState};

use super::build::{local_sandbox_image_build_context, local_sandbox_image_dockerfile};

pub(super) async fn run_local_sandbox_image_job(
    jobs: Arc<RwLock<Vec<LocalSandboxImageJob>>>,
    state: Arc<RwLock<LocalState>>,
    state_path: PathBuf,
    job_id: String,
) {
    let job = {
        let jobs_guard = jobs.read().await;
        jobs_guard.iter().find(|job| job.id == job_id).cloned()
    };
    let Some(job) = job else {
        return;
    };
    let context = local_sandbox_image_build_context();
    let dockerfile = local_sandbox_image_dockerfile(context.as_path());
    let mut command = docker_command();
    command
        .arg("build")
        .arg("-t")
        .arg(job.image_ref.as_str())
        .arg("-f")
        .arg(dockerfile.as_path())
        .arg("--build-arg")
        .arg(format!("SANDBOX_FEATURES={}", job.features.join(",")));
    if let Some(script) = job.custom_build_script.as_deref() {
        command.arg("--build-arg").arg(format!(
            "SANDBOX_CUSTOM_SCRIPT_B64={}",
            general_purpose::STANDARD.encode(script.as_bytes())
        ));
    }
    command
        .arg(context.as_path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    append_local_sandbox_job_output(
        &jobs,
        job_id.as_str(),
        format!(
            "[local connector] docker build -t {} -f {} {}\n",
            job.image_ref,
            dockerfile.display(),
            context.display()
        )
        .as_str(),
    )
    .await;

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            let message = format!("start docker build failed: {err}");
            append_local_sandbox_job_output(
                &jobs,
                job_id.as_str(),
                format!("{message}\n").as_str(),
            )
            .await;
            finish_local_sandbox_image_job(&jobs, job_id.as_str(), "failed", Some(message)).await;
            return;
        }
    };

    let stdout_task = child.stdout.take().map(|stdout| {
        tokio::spawn(read_local_sandbox_job_stream(
            stdout,
            jobs.clone(),
            job_id.clone(),
        ))
    });
    let stderr_task = child.stderr.take().map(|stderr| {
        tokio::spawn(read_local_sandbox_job_stream(
            stderr,
            jobs.clone(),
            job_id.clone(),
        ))
    });

    let wait_result = child.wait().await;
    let stdout = join_sandbox_log_task(stdout_task).await;
    let stderr = join_sandbox_log_task(stderr_task).await;
    let (status, error) = match wait_result {
        Ok(exit_status) if exit_status.success() => ("succeeded", None),
        Ok(exit_status) => {
            let details = if stderr.trim().is_empty() {
                stdout.trim()
            } else {
                stderr.trim()
            };
            (
                "failed",
                Some(format!(
                    "docker build failed with status {exit_status}: {details}"
                )),
            )
        }
        Err(err) => (
            "failed",
            Some(format!("wait docker build process failed: {err}")),
        ),
    };
    finish_local_sandbox_image_job(&jobs, job_id.as_str(), status, error).await;
    if status == "succeeded" {
        let mut state_guard = state.write().await;
        upsert_local_sandbox_image_record(&mut state_guard, &job);
        state_guard.sandbox.selected_image_ref = Some(job.image_ref);
        if let Err(err) = state_guard.save(state_path.as_path()) {
            tracing_stdout(format!("save selected local sandbox image failed: {err}").as_str());
        }
    }
}

async fn read_local_sandbox_job_stream<R>(
    mut reader: R,
    jobs: Arc<RwLock<Vec<LocalSandboxImageJob>>>,
    job_id: String,
) -> String
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let mut collected = String::new();
    let mut buffer = [0u8; 8192];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) => break,
            Ok(n) => {
                let chunk = String::from_utf8_lossy(&buffer[..n]).to_string();
                append_local_sandbox_job_output(&jobs, job_id.as_str(), chunk.as_str()).await;
                collected.push_str(chunk.as_str());
                collected = truncate_local_sandbox_job_output(collected.as_str());
            }
            Err(err) => {
                let message = format!("[local connector] read docker build log failed: {err}\n");
                append_local_sandbox_job_output(&jobs, job_id.as_str(), message.as_str()).await;
                break;
            }
        }
    }
    collected
}

async fn join_sandbox_log_task(task: Option<JoinHandle<String>>) -> String {
    let Some(task) = task else {
        return String::new();
    };
    match task.await {
        Ok(output) => output,
        Err(err) => {
            tracing_stdout(format!("read docker build log task failed: {err}").as_str());
            String::new()
        }
    }
}

async fn append_local_sandbox_job_output(
    jobs: &Arc<RwLock<Vec<LocalSandboxImageJob>>>,
    job_id: &str,
    chunk: &str,
) {
    let mut jobs_guard = jobs.write().await;
    if let Some(stored) = jobs_guard.iter_mut().find(|job| job.id == job_id) {
        stored.output.push_str(chunk);
        stored.output = truncate_local_sandbox_job_output(stored.output.as_str());
        stored.updated_at = local_now_rfc3339();
    }
}

async fn finish_local_sandbox_image_job(
    jobs: &Arc<RwLock<Vec<LocalSandboxImageJob>>>,
    job_id: &str,
    status: &str,
    error: Option<String>,
) {
    let mut jobs_guard = jobs.write().await;
    if let Some(stored) = jobs_guard.iter_mut().find(|job| job.id == job_id) {
        stored.status = status.to_string();
        stored.updated_at = local_now_rfc3339();
        stored.finished_at = Some(local_now_rfc3339());
        if stored.output.trim().is_empty() {
            if let Some(error) = error.as_deref() {
                stored.output = truncate_local_sandbox_job_output(error);
            }
        }
        stored.error = error;
    }
}

fn truncate_local_sandbox_job_output(value: &str) -> String {
    const MAX_JOB_OUTPUT_LEN: usize = 80_000;
    if value.len() <= MAX_JOB_OUTPUT_LEN {
        return value.to_string();
    }
    let start = value.len().saturating_sub(MAX_JOB_OUTPUT_LEN);
    let mut boundary = start;
    while boundary < value.len() && !value.is_char_boundary(boundary) {
        boundary += 1;
    }
    format!("... output truncated ...\n{}", &value[boundary..])
}

fn upsert_local_sandbox_image_record(state: &mut LocalState, job: &LocalSandboxImageJob) {
    let now = local_now_rfc3339();
    let mut features = job.features.clone();
    if let Some(script) = job.custom_build_script.as_deref() {
        features.push(custom_build_script_feature(script));
    }
    let record = LocalSandboxImageRecord {
        id: job.image_id.clone(),
        image_name: job.image_name.clone(),
        image_ref: job.image_ref.clone(),
        features,
        custom_build_script: job.custom_build_script.clone(),
        backend: job.backend.clone(),
        created_at: job.created_at.clone(),
        updated_at: now,
    };
    if let Some(existing) = state
        .sandbox
        .images
        .iter_mut()
        .find(|image| image.id == record.id || image.image_ref == record.image_ref)
    {
        let created_at = existing.created_at.clone();
        *existing = record;
        existing.created_at = created_at;
    } else {
        state.sandbox.images.push(record);
    }
}
