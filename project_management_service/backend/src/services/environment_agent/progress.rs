// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;
use std::{collections::BTreeMap, str::FromStr};

use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use serde::Deserialize;

use crate::http_body::{
    read_response_json_limited, read_response_text_limited_or_message,
    ERROR_BODY_PREVIEW_LIMIT_BYTES, JSON_BODY_LIMIT_BYTES,
};
use crate::models::{
    now_rfc3339, ProjectRecord, ProjectRuntimeEnvironmentProgressResponse,
    ProjectRuntimeEnvironmentRecord, ProjectRuntimeEnvironmentStatus, RuntimeEnvironmentProvider,
};
use crate::state::AppState;

use super::routing::{find_enabled_local_sandbox_pairing, parse_local_connector_project_root};

const MAX_PROGRESS_LOG_BYTES: usize = 40_000;

#[derive(Debug, Clone, Default, Deserialize)]
struct SandboxImageJobProgress {
    #[serde(default)]
    id: String,
    #[serde(default)]
    image_id: String,
    #[serde(default)]
    image_ref: String,
    #[serde(default)]
    features: Vec<String>,
    #[serde(default)]
    status: String,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    updated_at: String,
    #[serde(default)]
    started_at: Option<String>,
    #[serde(default)]
    finished_at: Option<String>,
    #[serde(default)]
    output: String,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    run_id: Option<String>,
}

pub async fn get_project_runtime_environment_progress(
    state: &AppState,
    project: &ProjectRecord,
    user_access_token: Option<&str>,
) -> Result<ProjectRuntimeEnvironmentProgressResponse, String> {
    let mut environment = state
        .store
        .get_project_runtime_environment(project.id.as_str())
        .await?
        .unwrap_or_else(|| {
            super::super::runtime_environment::default_runtime_environment_for_project(
                project, None,
            )
        });
    let provider = provider_for_environment(&environment);
    let active_image_build = environment.status
        == ProjectRuntimeEnvironmentStatus::PendingImageBuild
        && environment
            .last_agent_run_id
            .as_deref()
            .is_some_and(|run_id| run_id.starts_with("project_image_build_"));
    let jobs = if environment.sandbox_enabled
        && (active_image_build
            || matches!(
                environment.status,
                ProjectRuntimeEnvironmentStatus::Analyzing
                    | ProjectRuntimeEnvironmentStatus::Ready
                    | ProjectRuntimeEnvironmentStatus::Failed
            )) {
        fetch_image_jobs(state, project, provider, user_access_token).await?
    } else {
        Vec::new()
    };
    let job = select_project_job(jobs, project.id.as_str(), &environment);

    if environment.status == ProjectRuntimeEnvironmentStatus::Analyzing
        && job
            .as_ref()
            .is_some_and(|job| job.status.eq_ignore_ascii_case("failed"))
    {
        let failed_job = job.as_ref().expect("failed job checked above");
        environment.status = ProjectRuntimeEnvironmentStatus::Failed;
        environment.analysis_summary = Some("沙箱镜像初始化失败。".to_string());
        environment.last_error = Some(job_failure_detail(failed_job));
        environment.updated_at = now_rfc3339();
        environment = state
            .store
            .upsert_project_runtime_environment(&environment)
            .await?;
    } else if environment.status == ProjectRuntimeEnvironmentStatus::Analyzing && job.is_none() {
        let analysis_active = state
            .runtime_environment_analysis_jobs
            .lock()
            .await
            .contains(project.id.as_str());
        if !analysis_active {
            environment.status = ProjectRuntimeEnvironmentStatus::Failed;
            environment.analysis_summary = Some("项目运行环境分析任务已中断。".to_string());
            environment.last_error = Some(
                "analysis worker is no longer active; please initialize the runtime environment again"
                    .to_string(),
            );
            environment.updated_at = now_rfc3339();
            environment = state
                .store
                .upsert_project_runtime_environment(&environment)
                .await?;
        }
    } else if active_image_build && job.is_none() {
        let image_build_active = state
            .runtime_environment_image_jobs
            .lock()
            .await
            .contains(project.id.as_str());
        if !image_build_active {
            environment.status = ProjectRuntimeEnvironmentStatus::Failed;
            environment.analysis_summary = Some("项目沙箱镜像准备任务已中断。".to_string());
            environment.last_error = Some(
                "image build worker is no longer active; please prepare the runtime images again"
                    .to_string(),
            );
            environment.updated_at = now_rfc3339();
            environment = state
                .store
                .upsert_project_runtime_environment(&environment)
                .await?;
        }
    }

    Ok(progress_response(
        project,
        &environment,
        provider,
        job.as_ref(),
    ))
}

fn provider_for_environment(
    environment: &ProjectRuntimeEnvironmentRecord,
) -> RuntimeEnvironmentProvider {
    if !environment.sandbox_enabled {
        return RuntimeEnvironmentProvider::None;
    }
    match environment.sandbox_provider {
        RuntimeEnvironmentProvider::LocalConnector => RuntimeEnvironmentProvider::LocalConnector,
        RuntimeEnvironmentProvider::CloudSandboxManager => {
            RuntimeEnvironmentProvider::CloudSandboxManager
        }
        RuntimeEnvironmentProvider::None | RuntimeEnvironmentProvider::Harness => {
            RuntimeEnvironmentProvider::None
        }
    }
}

async fn fetch_image_jobs(
    state: &AppState,
    project: &ProjectRecord,
    provider: RuntimeEnvironmentProvider,
    user_access_token: Option<&str>,
) -> Result<Vec<SandboxImageJobProgress>, String> {
    match provider {
        RuntimeEnvironmentProvider::CloudSandboxManager => {
            fetch_cloud_image_jobs(state, project.id.as_str()).await
        }
        RuntimeEnvironmentProvider::LocalConnector => {
            fetch_local_image_jobs(state, project, user_access_token).await
        }
        RuntimeEnvironmentProvider::None | RuntimeEnvironmentProvider::Harness => Ok(Vec::new()),
    }
}

async fn fetch_cloud_image_jobs(
    state: &AppState,
    project_id: &str,
) -> Result<Vec<SandboxImageJobProgress>, String> {
    let client_id = "project-service";
    let client_key = required_config_value(
        state.config.sandbox_manager_client_key.as_deref(),
        "PROJECT_SERVICE_SANDBOX_MANAGER_CLIENT_KEY",
    )?;
    let url = format!(
        "{}/api/internal/sandbox-images/jobs",
        state
            .config
            .sandbox_manager_base_url
            .trim()
            .trim_end_matches('/')
    );
    let token = chatos_service_runtime::issue_internal_service_token(
        client_key,
        client_id,
        "sandbox-manager",
        "sandbox.service",
        60,
    )?;
    read_jobs_response(
        state
            .config
            .sandbox_manager_http_client
            .get(url)
            .header("x-sandbox-caller", client_id)
            .header("x-sandbox-internal-token", token)
            .header(
                chatos_mcp::sandbox_images::SANDBOX_IMAGE_PROJECT_ID_HEADER,
                project_id,
            )
            .send()
            .await
            .map_err(|err| format!("query cloud sandbox image jobs failed: {err}"))?,
        "cloud sandbox image jobs",
    )
    .await
}

async fn fetch_local_image_jobs(
    state: &AppState,
    project: &ProjectRecord,
    user_access_token: Option<&str>,
) -> Result<Vec<SandboxImageJobProgress>, String> {
    let token = required_config_value(user_access_token, "user access token")?;
    let project_ref = project
        .root_path
        .as_deref()
        .and_then(parse_local_connector_project_root);
    let pairing =
        find_enabled_local_sandbox_pairing(&state.config, Some(token), project_ref.as_ref())
            .await?
            .ok_or_else(|| "没有找到已启用的 Local Connector 沙箱配对".to_string())?;
    let facade_base = pairing
        .id
        .as_deref()
        .map(|id| {
            format!(
                "{}/api/local-connectors/sandbox-facade/{}",
                state
                    .config
                    .local_connector_service_base_url
                    .trim()
                    .trim_end_matches('/'),
                urlencoding::encode(id)
            )
        })
        .or_else(|| {
            pairing
                .facade_base_url
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
        .ok_or_else(|| "Local Connector 沙箱配对缺少 facade_base_url".to_string())?;
    read_jobs_response(
        state
            .config
            .local_connector_http_client
            .get(format!(
                "{}/api/local/sandbox/images/jobs",
                facade_base.trim_end_matches('/')
            ))
            .timeout(Duration::from_secs(20))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|err| format!("query local sandbox image jobs failed: {err}"))?,
        "local sandbox image jobs",
    )
    .await
}

async fn read_jobs_response(
    response: reqwest::Response,
    label: &str,
) -> Result<Vec<SandboxImageJobProgress>, String> {
    let status = response.status();
    if status == StatusCode::NOT_FOUND {
        return Ok(Vec::new());
    }
    if !status.is_success() {
        let detail =
            read_response_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES).await;
        return Err(format!(
            "query {label} returned status={status} detail={}",
            truncate_chars(detail.as_str(), 2_000)
        ));
    }
    read_response_json_limited::<Vec<SandboxImageJobProgress>>(response, JSON_BODY_LIMIT_BYTES)
        .await
        .map_err(|err| format!("parse {label} failed: {err}"))
}

fn select_project_job(
    jobs: Vec<SandboxImageJobProgress>,
    project_id: &str,
    environment: &ProjectRuntimeEnvironmentRecord,
) -> Option<SandboxImageJobProgress> {
    let run_id = environment.last_agent_run_id.as_deref();
    let mut exact = jobs
        .iter()
        .filter(|job| {
            job.project_id.as_deref() == Some(project_id)
                && run_id.is_some_and(|run_id| {
                    job.run_id.as_deref().is_some_and(|job_run_id| {
                        job_run_id == run_id
                            || job_run_id.strip_prefix(run_id).is_some_and(|suffix| {
                                suffix.starts_with("_workspace_")
                                    || suffix.starts_with("_dependency_")
                            })
                    })
                })
        })
        .cloned()
        .collect::<Vec<_>>();
    exact.sort_by_key(job_timestamp);
    if let Some(job) = select_representative_project_job(&exact) {
        return Some(job);
    }

    // Jobs created before project/run metadata was introduced can still close a stale analysis.
    let environment_started = parse_timestamp(environment.updated_at.as_str())?;
    let legacy_floor = environment_started - chrono::Duration::seconds(30);
    let legacy_ceiling = environment_started + chrono::Duration::minutes(30);
    let mut legacy = jobs
        .into_iter()
        .filter(|job| job.project_id.is_none() && job.run_id.is_none())
        .filter(|job| {
            job_timestamp(job)
                .is_some_and(|created| created >= legacy_floor && created <= legacy_ceiling)
        })
        .collect::<Vec<_>>();
    legacy.sort_by_key(job_timestamp);
    select_representative_project_job(&legacy)
}

fn select_representative_project_job(
    jobs: &[SandboxImageJobProgress],
) -> Option<SandboxImageJobProgress> {
    jobs.iter()
        .rev()
        .find(|job| job.status.eq_ignore_ascii_case("failed"))
        .or_else(|| {
            jobs.iter()
                .rev()
                .find(|job| job.status.eq_ignore_ascii_case("running"))
        })
        .or_else(|| jobs.last())
        .cloned()
}

fn progress_response(
    project: &ProjectRecord,
    environment: &ProjectRuntimeEnvironmentRecord,
    provider: RuntimeEnvironmentProvider,
    job: Option<&SandboxImageJobProgress>,
) -> ProjectRuntimeEnvironmentProgressResponse {
    let job_status = job.map(|job| job.status.trim().to_ascii_lowercase());
    let build_progress = job.and_then(|job| estimate_build_progress(job.output.as_str()));
    let (mut phase, status, progress_percent) =
        progress_state(environment.status, job_status.as_deref(), build_progress);
    if job.is_some_and(is_dependency_job) && phase == "building_image" {
        phase = "preparing_dependency_image";
    }
    let logs = job
        .map(|job| tail_utf8(job.output.as_str(), MAX_PROGRESS_LOG_BYTES))
        .unwrap_or_default();
    let error = if status == "failed" {
        job.map(job_failure_detail)
            .or_else(|| environment.last_error.clone())
    } else {
        None
    };

    ProjectRuntimeEnvironmentProgressResponse {
        project_id: project.id.clone(),
        run_id: environment.last_agent_run_id.clone(),
        phase: phase.to_string(),
        status: status.to_string(),
        progress_percent,
        provider,
        job_id: job
            .map(|job| job.id.clone())
            .filter(|value| !value.is_empty()),
        image_id: job
            .map(|job| job.image_id.clone())
            .filter(|value| !value.is_empty()),
        image_ref: job
            .map(|job| job.image_ref.clone())
            .filter(|value| !value.is_empty()),
        started_at: job.and_then(|job| job.started_at.clone()),
        updated_at: job
            .map(|job| job.updated_at.clone())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| environment.updated_at.clone()),
        finished_at: job.and_then(|job| job.finished_at.clone()),
        logs,
        error,
    }
}

fn progress_state(
    environment_status: ProjectRuntimeEnvironmentStatus,
    job_status: Option<&str>,
    build_progress: Option<u8>,
) -> (&'static str, &'static str, Option<u8>) {
    match environment_status {
        ProjectRuntimeEnvironmentStatus::Disabled => ("disabled", "idle", Some(0)),
        ProjectRuntimeEnvironmentStatus::PendingConfiguration => {
            ("pending_configuration", "idle", Some(0))
        }
        ProjectRuntimeEnvironmentStatus::PendingImageBuild => match job_status {
            Some("running") => ("building_image", "running", build_progress),
            Some("succeeded") => ("finalizing", "running", Some(90)),
            Some("failed") => ("failed", "failed", Some(100)),
            _ => ("pending_image_build", "idle", Some(0)),
        },
        ProjectRuntimeEnvironmentStatus::Pending => ("pending", "idle", Some(0)),
        ProjectRuntimeEnvironmentStatus::Ready => ("completed", "succeeded", Some(100)),
        ProjectRuntimeEnvironmentStatus::NotRunnable => ("not_runnable", "succeeded", Some(100)),
        ProjectRuntimeEnvironmentStatus::Failed => ("failed", "failed", Some(100)),
        ProjectRuntimeEnvironmentStatus::Analyzing => match job_status {
            Some("running") => ("building_image", "running", build_progress),
            Some("succeeded") => ("finalizing", "running", Some(90)),
            Some("failed") => ("failed", "failed", Some(100)),
            _ => ("analyzing_project", "running", Some(15)),
        },
    }
}

fn estimate_build_progress(output: &str) -> Option<u8> {
    let mut stages = BTreeMap::<String, (u32, u32)>::new();
    for line in output.lines() {
        let Some(open) = line.find('[') else {
            continue;
        };
        let Some(close_offset) = line[open + 1..].find(']') else {
            continue;
        };
        let marker = &line[open + 1..open + 1 + close_offset];
        let mut parts = marker.split_whitespace().collect::<Vec<_>>();
        let Some(step) = parts.pop() else {
            continue;
        };
        let Some((current, total)) = step.split_once('/') else {
            continue;
        };
        let (Ok(current), Ok(total)) = (u32::from_str(current), u32::from_str(total)) else {
            continue;
        };
        if total == 0 || parts.is_empty() {
            continue;
        }
        let stage = parts.join(" ");
        let entry = stages.entry(stage).or_insert((0, total));
        if current >= entry.0 {
            *entry = (current.min(total), total);
        }
    }
    if stages.is_empty() {
        return None;
    }
    let average = stages
        .values()
        .map(|(current, total)| *current as f64 / *total as f64)
        .sum::<f64>()
        / stages.len() as f64;
    Some((20.0 + average * 70.0).round().clamp(20.0, 90.0) as u8)
}

fn job_failure_detail(job: &SandboxImageJobProgress) -> String {
    let error = job
        .error
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("sandbox image creation failed");
    let useful_line = job.output.lines().rev().find(|line| {
        let normalized = line.trim().to_ascii_lowercase();
        normalized.starts_with("error:")
            || normalized.contains("externally-managed-environment")
            || normalized.contains("failed to solve")
    });
    useful_line
        .map(str::trim)
        .filter(|line| !line.is_empty() && !error.contains(line))
        .map(|line| format!("{error}: {}", truncate_chars(line, 800)))
        .unwrap_or_else(|| error.to_string())
}

fn is_dependency_job(job: &SandboxImageJobProgress) -> bool {
    job.id.starts_with("dependency-image-job-")
        || job.image_id.starts_with("dependency:")
        || job
            .features
            .iter()
            .any(|feature| feature.starts_with("dependency@"))
}

fn job_timestamp(job: &SandboxImageJobProgress) -> Option<DateTime<Utc>> {
    parse_timestamp(
        job.started_at
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(job.created_at.as_str()),
    )
}

fn parse_timestamp(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value.trim())
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

fn required_config_value<'a>(value: Option<&'a str>, name: &str) -> Result<&'a str, String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{name} is required"))
}

fn tail_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut start = value.len().saturating_sub(max_bytes);
    while start < value.len() && !value.is_char_boundary(start) {
        start += 1;
    }
    format!("... earlier output omitted ...\n{}", &value[start..])
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut output = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        output.push_str("...<truncated>");
    }
    output
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn analyzing_environment() -> ProjectRuntimeEnvironmentRecord {
        ProjectRuntimeEnvironmentRecord {
            project_id: "project-1".to_string(),
            status: ProjectRuntimeEnvironmentStatus::Analyzing,
            sandbox_enabled: true,
            sandbox_provider: RuntimeEnvironmentProvider::CloudSandboxManager,
            file_provider: RuntimeEnvironmentProvider::LocalConnector,
            analysis_summary: None,
            not_runnable_reason: None,
            execution_service_id: None,
            detected_stack: json!({}),
            required_services: json!([]),
            env_vars: json!({}),
            environment_variables: Vec::new(),
            generated_config_files: Vec::new(),
            last_agent_run_id: Some("run-1".to_string()),
            last_error: None,
            created_at: "2026-07-10T10:00:00Z".to_string(),
            updated_at: "2026-07-10T10:00:00Z".to_string(),
        }
    }

    #[test]
    fn uses_persisted_sandbox_provider_independently_from_file_provider() {
        let environment = analyzing_environment();
        assert_eq!(
            provider_for_environment(&environment),
            RuntimeEnvironmentProvider::CloudSandboxManager
        );
    }

    #[test]
    fn selects_job_bound_to_current_project_and_run() {
        let environment = analyzing_environment();
        let selected = select_project_job(
            vec![
                SandboxImageJobProgress {
                    id: "other".to_string(),
                    project_id: Some("project-2".to_string()),
                    run_id: Some("run-2".to_string()),
                    created_at: "2026-07-10T10:00:02Z".to_string(),
                    ..SandboxImageJobProgress::default()
                },
                SandboxImageJobProgress {
                    id: "current".to_string(),
                    project_id: Some("project-1".to_string()),
                    run_id: Some("run-1".to_string()),
                    created_at: "2026-07-10T10:00:01Z".to_string(),
                    ..SandboxImageJobProgress::default()
                },
            ],
            "project-1",
            &environment,
        )
        .expect("current project job");

        assert_eq!(selected.id, "current");
    }

    #[test]
    fn selects_workspace_job_for_the_current_image_build_batch() {
        let mut environment = analyzing_environment();
        environment.status = ProjectRuntimeEnvironmentStatus::PendingImageBuild;
        environment.last_agent_run_id = Some("project_image_build_batch-1".to_string());
        let selected = select_project_job(
            vec![SandboxImageJobProgress {
                id: "workspace-job".to_string(),
                project_id: Some("project-1".to_string()),
                run_id: Some("project_image_build_batch-1_workspace_0".to_string()),
                status: "running".to_string(),
                created_at: "2026-07-10T10:00:01Z".to_string(),
                ..SandboxImageJobProgress::default()
            }],
            "project-1",
            &environment,
        )
        .expect("current workspace build job");

        assert_eq!(selected.id, "workspace-job");
        assert_eq!(
            progress_state(environment.status, Some("running"), Some(42)),
            ("building_image", "running", Some(42))
        );
    }

    #[test]
    fn selects_dependency_job_for_the_current_image_build_batch() {
        let mut environment = analyzing_environment();
        environment.status = ProjectRuntimeEnvironmentStatus::PendingImageBuild;
        environment.last_agent_run_id = Some("project_image_build_batch-1".to_string());
        let selected = select_project_job(
            vec![SandboxImageJobProgress {
                id: "dependency-image-job-1".to_string(),
                image_id: "dependency:postgres:16-alpine".to_string(),
                image_ref: "postgres:16-alpine".to_string(),
                project_id: Some("project-1".to_string()),
                run_id: Some("project_image_build_batch-1_dependency_0".to_string()),
                status: "running".to_string(),
                created_at: "2026-07-10T10:00:01Z".to_string(),
                ..SandboxImageJobProgress::default()
            }],
            "project-1",
            &environment,
        )
        .expect("current dependency pull job");

        assert_eq!(selected.id, "dependency-image-job-1");
        let response = progress_response(
            &project_record("project-1"),
            &environment,
            RuntimeEnvironmentProvider::CloudSandboxManager,
            Some(&selected),
        );
        assert_eq!(response.phase, "preparing_dependency_image");
        assert_eq!(response.image_ref.as_deref(), Some("postgres:16-alpine"));
    }

    #[test]
    fn representative_job_prefers_failed_then_running_before_latest_completed() {
        let selected = select_representative_project_job(&[
            SandboxImageJobProgress {
                id: "running-workspace".to_string(),
                status: "running".to_string(),
                created_at: "2026-07-10T10:00:01Z".to_string(),
                ..SandboxImageJobProgress::default()
            },
            SandboxImageJobProgress {
                id: "succeeded-dependency".to_string(),
                status: "succeeded".to_string(),
                created_at: "2026-07-10T10:00:02Z".to_string(),
                ..SandboxImageJobProgress::default()
            },
        ])
        .expect("representative running job");
        assert_eq!(selected.id, "running-workspace");

        let selected = select_representative_project_job(&[
            selected,
            SandboxImageJobProgress {
                id: "failed-dependency".to_string(),
                status: "failed".to_string(),
                created_at: "2026-07-10T10:00:00Z".to_string(),
                ..SandboxImageJobProgress::default()
            },
        ])
        .expect("representative failed job");
        assert_eq!(selected.id, "failed-dependency");
    }

    #[test]
    fn does_not_attach_unrelated_late_legacy_job() {
        let environment = analyzing_environment();
        let selected = select_project_job(
            vec![SandboxImageJobProgress {
                id: "unrelated".to_string(),
                created_at: "2026-07-10T11:00:00Z".to_string(),
                ..SandboxImageJobProgress::default()
            }],
            "project-1",
            &environment,
        );

        assert!(selected.is_none());
    }

    #[test]
    fn extracts_specific_build_error_from_logs() {
        let detail = job_failure_detail(&SandboxImageJobProgress {
            error: Some("docker build failed".to_string()),
            output: "step 1\nerror: externally-managed-environment\nfailed".to_string(),
            ..SandboxImageJobProgress::default()
        });

        assert!(detail.contains("externally-managed-environment"));
    }

    #[test]
    fn estimates_parallel_buildkit_stage_progress() {
        let progress = estimate_build_progress(
            "#14 [stage-1 2/6] RUN install\n#16 [agent-builder 12/12] RUN cargo build\n",
        );

        assert_eq!(progress, Some(67));
    }

    #[test]
    fn pending_image_build_starts_at_zero_percent() {
        assert_eq!(
            progress_state(
                ProjectRuntimeEnvironmentStatus::PendingImageBuild,
                None,
                None,
            ),
            ("pending_image_build", "idle", Some(0))
        );
    }

    fn project_record(id: &str) -> ProjectRecord {
        ProjectRecord {
            id: id.to_string(),
            creator_user_id: None,
            creator_username: None,
            creator_display_name: None,
            owner_user_id: None,
            owner_username: None,
            owner_display_name: None,
            name: "Project".to_string(),
            root_path: None,
            git_url: None,
            source_type: crate::models::ProjectSourceType::Cloud,
            execution_plane: crate::models::ProjectExecutionPlane::Cloud,
            cloud_import_source: crate::models::CloudImportSource::None,
            import_status: crate::models::ProjectImportStatus::None,
            source_git_url: None,
            harness_space_identifier: None,
            harness_repo_identifier: None,
            harness_repo_path: None,
            harness_git_url: None,
            harness_git_ssh_url: None,
            harness_default_branch: None,
            harness_provision_status: None,
            harness_provision_error: None,
            harness_provisioned_at: None,
            import_error: None,
            import_started_at: None,
            import_finished_at: None,
            description: None,
            status: crate::models::ProjectStatus::Active,
            created_at: "2026-07-10T10:00:00Z".to_string(),
            updated_at: "2026-07-10T10:00:00Z".to_string(),
            archived_at: None,
        }
    }
}
