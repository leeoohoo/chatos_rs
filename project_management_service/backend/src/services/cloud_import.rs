// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::PathBuf;

use reqwest::Method;
use serde::{Deserialize, Serialize};

mod archive;
mod archive_policy;
mod git;

use archive::{flatten_single_project_directory, has_importable_files, unpack_zip_safely};
use git::{authenticated_git_url, run_git, run_git_output};

use crate::config::AppConfig;
use crate::http_body::{read_response_text_limited_or_message, ERROR_BODY_PREVIEW_LIMIT_BYTES};
use crate::models::ProjectRecord;
use chatos_service_runtime::http_body::{read_response_json_limited, JSON_BODY_LIMIT_BYTES};
use chatos_service_runtime::{build_http_client, HttpClientTimeouts};

#[derive(Debug, Clone, Deserialize)]
pub struct HarnessProjectRepoResponse {
    pub space_identifier: String,
    pub repo_identifier: String,
    pub repo_path: String,
    pub git_url: String,
    pub git_ssh_url: Option<String>,
    pub default_branch: String,
    pub push_username: String,
    pub push_token: String,
}

#[derive(Debug, Serialize)]
struct HarnessProjectRepoCreateRequest<'a> {
    project_id: &'a str,
    project_name: &'a str,
    description: Option<&'a str>,
}

pub async fn create_harness_repo_for_project(
    config: &AppConfig,
    access_token: &str,
    project: &ProjectRecord,
) -> Result<HarnessProjectRepoResponse, String> {
    let secret = config
        .user_service_internal_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "PROJECT_SERVICE_USER_SERVICE_INTERNAL_SECRET is not configured".to_string()
        })?;
    let endpoint = format!(
        "{}/api/internal/harness/repos",
        config.user_service_base_url.trim().trim_end_matches('/')
    );
    let body = HarnessProjectRepoCreateRequest {
        project_id: project.id.as_str(),
        project_name: project.name.as_str(),
        description: project.description.as_deref(),
    };
    let client = build_http_client(HttpClientTimeouts::new(config.user_service_request_timeout))
        .map_err(|err| format!("build user_service client failed: {err}"))?;
    let response = crate::user_model_runtime_client::signed_user_service_request(
        client.request(Method::POST, endpoint),
        secret,
        crate::user_model_runtime_client::HARNESS_REPO_WRITE_SCOPE,
    )?
    .bearer_auth(access_token.trim())
    .json(&body)
    .send()
    .await
    .map_err(|err| format!("user_service harness repo request failed: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let text =
            read_response_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES).await;
        return Err(format!(
            "user_service harness repo request failed: {status} {text}"
        ));
    }
    read_response_json_limited::<HarnessProjectRepoResponse>(response, JSON_BODY_LIMIT_BYTES)
        .await
        .map_err(|err| format!("parse user_service harness repo response failed: {err}"))
}

pub async fn import_git_url_to_harness(
    config: &AppConfig,
    source_git_url: &str,
    repo: &HarnessProjectRepoResponse,
    project_id: &str,
) -> Result<(), String> {
    let temp_root = create_temp_import_dir(project_id, "git")?;
    let mirror_dir = temp_root.join("source.git");
    let push_url = authenticated_git_url(
        repo.git_url.as_str(),
        repo.push_username.as_str(),
        repo.push_token.as_str(),
    )?;
    let result = async {
        run_git(
            vec![
                "clone".to_string(),
                "--mirror".to_string(),
                source_git_url.trim().to_string(),
                mirror_dir.to_string_lossy().to_string(),
            ],
            None,
            config,
            &[repo.push_token.as_str()],
        )
        .await?;
        run_git(
            vec!["push".to_string(), "--mirror".to_string(), push_url.clone()],
            Some(mirror_dir.as_path()),
            config,
            &[repo.push_token.as_str()],
        )
        .await
    }
    .await;
    let _ = fs::remove_dir_all(&temp_root);
    result
}

pub async fn import_zip_to_harness(
    config: &AppConfig,
    zip_bytes: Vec<u8>,
    repo: &HarnessProjectRepoResponse,
    project_id: &str,
) -> Result<(), String> {
    if zip_bytes.len() > config.cloud_project_max_zip_bytes {
        return Err(format!(
            "zip file is too large: {} bytes > {} bytes",
            zip_bytes.len(),
            config.cloud_project_max_zip_bytes
        ));
    }
    let temp_root = create_temp_import_dir(project_id, "zip")?;
    let work_dir = temp_root.join("worktree");
    fs::create_dir_all(&work_dir).map_err(|err| err.to_string())?;
    let result = async {
        unpack_zip_safely(
            zip_bytes,
            work_dir.as_path(),
            config.cloud_project_max_files,
            config.cloud_project_max_unpacked_bytes,
        )?;
        flatten_single_project_directory(work_dir.as_path())?;
        if !has_importable_files(work_dir.as_path()) {
            return Err(
                "ZIP 中没有可导入的源文件；.git、依赖目录、编译产物和缓存会被自动忽略".to_string(),
            );
        }
        let push_url = authenticated_git_url(
            repo.git_url.as_str(),
            repo.push_username.as_str(),
            repo.push_token.as_str(),
        )?;
        run_git(
            vec!["init".to_string()],
            Some(work_dir.as_path()),
            config,
            &[],
        )
        .await?;
        run_git(
            vec!["checkout".to_string(), "-B".to_string(), "main".to_string()],
            Some(work_dir.as_path()),
            config,
            &[],
        )
        .await?;
        run_git(
            vec!["add".to_string(), "-A".to_string()],
            Some(work_dir.as_path()),
            config,
            &[],
        )
        .await?;
        let status = run_git_output(
            vec!["status".to_string(), "--porcelain".to_string()],
            Some(work_dir.as_path()),
            config,
            &[],
        )
        .await?;
        if status.trim().is_empty() {
            return Ok(());
        }
        run_git(
            vec![
                "-c".to_string(),
                "user.name=Chatos".to_string(),
                "-c".to_string(),
                "user.email=chatos@local".to_string(),
                "commit".to_string(),
                "-m".to_string(),
                "Initial cloud project import".to_string(),
            ],
            Some(work_dir.as_path()),
            config,
            &[],
        )
        .await?;
        run_git(
            vec![
                "remote".to_string(),
                "add".to_string(),
                "origin".to_string(),
                push_url.clone(),
            ],
            Some(work_dir.as_path()),
            config,
            &[repo.push_token.as_str()],
        )
        .await?;
        run_git(
            vec![
                "push".to_string(),
                "-u".to_string(),
                "origin".to_string(),
                "main".to_string(),
            ],
            Some(work_dir.as_path()),
            config,
            &[repo.push_token.as_str()],
        )
        .await
    }
    .await;
    let _ = fs::remove_dir_all(&temp_root);
    result
}

fn create_temp_import_dir(project_id: &str, kind: &str) -> Result<PathBuf, String> {
    let safe_project_id: String = project_id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-')
        .take(64)
        .collect();
    let dir = std::env::temp_dir().join(format!(
        "chatos-cloud-import-{kind}-{}-{}",
        safe_project_id,
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(dir)
}
