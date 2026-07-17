// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;
use std::process::Stdio;

use tokio::process::Command;
use tokio::time::timeout;

use crate::config::AppConfig;

pub(super) fn authenticated_git_url(
    raw_url: &str,
    username: &str,
    token: &str,
) -> Result<String, String> {
    let mut url =
        reqwest::Url::parse(raw_url).map_err(|err| format!("invalid harness git url: {err}"))?;
    url.set_username(username.trim())
        .map_err(|_| "failed to set harness git username".to_string())?;
    url.set_password(Some(token.trim()))
        .map_err(|_| "failed to set harness git token".to_string())?;
    Ok(url.to_string())
}

pub(super) async fn run_git(
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

pub(super) async fn run_git_output(
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
