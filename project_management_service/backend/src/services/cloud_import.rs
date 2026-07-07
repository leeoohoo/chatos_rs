// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, Cursor};
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;

use reqwest::Method;
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::time::timeout;
use walkdir::WalkDir;
use zip::ZipArchive;

use crate::config::AppConfig;
use crate::http_body::{read_response_text_limited_or_message, ERROR_BODY_PREVIEW_LIMIT_BYTES};
use crate::models::ProjectRecord;

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
    let client = reqwest::Client::builder()
        .timeout(config.user_service_request_timeout)
        .build()
        .map_err(|err| format!("build user_service client failed: {err}"))?;
    let response = client
        .request(Method::POST, endpoint)
        .bearer_auth(access_token.trim())
        .header("x-user-service-internal-secret", secret)
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
    response
        .json::<HarnessProjectRepoResponse>()
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
        if !has_importable_files(work_dir.as_path()) {
            return Ok(());
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

fn unpack_zip_safely(
    zip_bytes: Vec<u8>,
    target_dir: &Path,
    max_files: usize,
    max_unpacked_bytes: u64,
) -> Result<(), String> {
    let cursor = Cursor::new(zip_bytes);
    let mut archive = ZipArchive::new(cursor).map_err(|err| format!("open zip failed: {err}"))?;
    let mut file_count = 0usize;
    let mut unpacked_bytes = 0u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|err| format!("read zip entry failed: {err}"))?;
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| format!("zip entry has unsafe path: {}", entry.name()))?
            .to_path_buf();
        reject_git_metadata_path(enclosed.as_path())?;
        if entry.is_dir() {
            fs::create_dir_all(target_dir.join(enclosed)).map_err(|err| err.to_string())?;
            continue;
        }
        file_count += 1;
        if file_count > max_files {
            return Err(format!(
                "zip contains too many files: {file_count} > {max_files}"
            ));
        }
        unpacked_bytes = unpacked_bytes.saturating_add(entry.size());
        if unpacked_bytes > max_unpacked_bytes {
            return Err(format!(
                "zip unpacked content is too large: {unpacked_bytes} bytes > {max_unpacked_bytes} bytes"
            ));
        }
        let out_path = target_dir.join(enclosed);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let mut out_file = File::create(out_path).map_err(|err| err.to_string())?;
        io::copy(&mut entry, &mut out_file).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn reject_git_metadata_path(path: &Path) -> Result<(), String> {
    if path.components().any(|component| match component {
        Component::Normal(value) => value == OsStr::new(".git"),
        _ => false,
    }) {
        Err("zip archives containing .git directories are not accepted".to_string())
    } else {
        Ok(())
    }
}

fn has_importable_files(path: &Path) -> bool {
    WalkDir::new(path)
        .into_iter()
        .filter_map(Result::ok)
        .any(|entry| entry.file_type().is_file())
}

fn authenticated_git_url(raw_url: &str, username: &str, token: &str) -> Result<String, String> {
    let mut url =
        reqwest::Url::parse(raw_url).map_err(|err| format!("invalid harness git url: {err}"))?;
    url.set_username(username.trim())
        .map_err(|_| "failed to set harness git username".to_string())?;
    url.set_password(Some(token.trim()))
        .map_err(|_| "failed to set harness git token".to_string())?;
    Ok(url.to_string())
}

async fn run_git(
    args: Vec<String>,
    cwd: Option<&Path>,
    config: &AppConfig,
    scrub_values: &[&str],
) -> Result<(), String> {
    let output = run_git_raw(args, cwd, config, scrub_values).await?;
    if output.status.success() {
        return Ok(());
    }
    Err(git_output_error(
        "git command failed",
        &output,
        scrub_values,
    ))
}

async fn run_git_output(
    args: Vec<String>,
    cwd: Option<&Path>,
    config: &AppConfig,
    scrub_values: &[&str],
) -> Result<String, String> {
    let output = run_git_raw(args, cwd, config, scrub_values).await?;
    if !output.status.success() {
        return Err(git_output_error(
            "git command failed",
            &output,
            scrub_values,
        ));
    }
    Ok(scrub_sensitive(
        String::from_utf8_lossy(output.stdout.as_slice()).as_ref(),
        scrub_values,
    ))
}

async fn run_git_raw(
    args: Vec<String>,
    cwd: Option<&Path>,
    config: &AppConfig,
    scrub_values: &[&str],
) -> Result<std::process::Output, String> {
    let mut command = Command::new("git");
    command.args(args.iter().map(String::as_str));
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    command.kill_on_drop(true);
    let child = command.spawn().map_err(|err| {
        scrub_sensitive(
            format!("failed to start git command: {err}").as_str(),
            scrub_values,
        )
    })?;
    timeout(config.cloud_project_git_timeout, child.wait_with_output())
        .await
        .map_err(|_| "git command timed out".to_string())?
        .map_err(|err| {
            scrub_sensitive(
                format!("failed to wait for git command: {err}").as_str(),
                scrub_values,
            )
        })
}

fn git_output_error(prefix: &str, output: &std::process::Output, scrub_values: &[&str]) -> String {
    let stderr = scrub_sensitive(
        String::from_utf8_lossy(output.stderr.as_slice()).as_ref(),
        scrub_values,
    );
    let stdout = scrub_sensitive(
        String::from_utf8_lossy(output.stdout.as_slice()).as_ref(),
        scrub_values,
    );
    format!(
        "{}: status={} stderr={} stdout={}",
        prefix,
        output.status,
        stderr.trim(),
        stdout.trim()
    )
}

fn scrub_sensitive(value: &str, scrub_values: &[&str]) -> String {
    let mut out = value.to_string();
    for secret in scrub_values {
        let secret = secret.trim();
        if !secret.is_empty() {
            out = out.replace(secret, "***");
        }
    }
    out
}
