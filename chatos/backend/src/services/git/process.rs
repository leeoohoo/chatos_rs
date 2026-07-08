// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::env;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::task::JoinHandle;
use tokio::time::sleep;

pub(super) const DEFAULT_GIT_TIMEOUT: Duration = Duration::from_secs(20);
pub(super) const REMOTE_GIT_TIMEOUT: Duration = Duration::from_secs(120);
const GIT_STDOUT_LIMIT_BYTES: usize = 16 * 1024 * 1024;
const GIT_STDERR_LIMIT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone)]
pub(super) struct GitCommandOutput {
    pub(super) stdout: String,
    pub(super) stderr: String,
}

#[derive(Debug, Clone)]
pub(super) struct GitCommandStatusOutput {
    pub(super) success: bool,
    pub(super) status: String,
    pub(super) stdout: String,
    pub(super) stderr: String,
}

#[derive(Debug, Clone)]
pub(super) struct GitBinaryResolution {
    pub(super) path: PathBuf,
    pub(super) source: GitBinarySource,
    pub(super) bundled_candidates: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum GitBinarySource {
    Env,
    Bundled,
    System,
}

impl GitBinarySource {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Env => "env",
            Self::Bundled => "bundled",
            Self::System => "system",
        }
    }
}

pub(super) async fn git_output<I, S>(
    root: &Path,
    args: I,
    duration: Duration,
) -> Result<GitCommandOutput, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = git_output_with_status(root, args, duration).await?;
    if !output.success {
        let message = output.stderr.trim();
        if message.is_empty() {
            let stdout = output.stdout.trim();
            if !stdout.is_empty() {
                return Err(stdout.chars().take(1200).collect());
            }
            return Err(format!("git 命令失败: {}", output.status));
        }
        return Err(message.chars().take(1200).collect());
    }
    Ok(GitCommandOutput {
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

pub(super) async fn git_output_with_status<I, S>(
    root: &Path,
    args: I,
    duration: Duration,
) -> Result<GitCommandStatusOutput, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let git_bin = resolve_git_binary();
    let mut command = Command::new(git_bin.path.as_os_str());
    command
        .arg("-C")
        .arg(root)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_MERGE_AUTOEDIT", "no")
        .args(args);
    add_git_bin_dir_to_path(&mut command, git_bin.path.as_path());
    let output = run_git_command_limited(command, duration, git_bin.path.as_path()).await?;
    Ok(GitCommandStatusOutput {
        success: output.success,
        status: output.status,
        stdout: String::from_utf8_lossy(output.stdout.as_slice()).to_string(),
        stderr: String::from_utf8_lossy(output.stderr.as_slice()).to_string(),
    })
}

pub(super) async fn git_version(git_bin: &GitBinaryResolution) -> Result<String, String> {
    let mut command = Command::new(git_bin.path.as_os_str());
    command.arg("--version").env("GIT_TERMINAL_PROMPT", "0");
    add_git_bin_dir_to_path(&mut command, git_bin.path.as_path());
    let output =
        match run_git_command_limited(command, DEFAULT_GIT_TIMEOUT, git_bin.path.as_path()).await {
            Ok(output) => output,
            Err(err) if err == "执行 git 命令超时" => {
                return Err("执行 git --version 超时".to_string())
            }
            Err(err) => return Err(err),
        };
    let stdout = String::from_utf8_lossy(output.stdout.as_slice())
        .trim()
        .to_string();
    let stderr = String::from_utf8_lossy(output.stderr.as_slice())
        .trim()
        .to_string();
    if output.success {
        return Ok(if stdout.is_empty() {
            "git version unknown".to_string()
        } else {
            stdout
        });
    }
    Err(if stderr.is_empty() {
        format!("git --version 失败: {}", output.status)
    } else {
        stderr.chars().take(1200).collect()
    })
}

struct RawGitCommandOutput {
    success: bool,
    status: String,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

async fn run_git_command_limited(
    mut command: Command,
    duration: Duration,
    git_bin: &Path,
) -> Result<RawGitCommandOutput, String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = command
        .spawn()
        .map_err(|err| git_launch_error(git_bin, err))?;
    let stdout = child.stdout.take().ok_or("missing git stdout")?;
    let stderr = child.stderr.take().ok_or("missing git stderr")?;
    let mut stdout_task = tokio::spawn(read_git_output_limited(
        stdout,
        "stdout",
        GIT_STDOUT_LIMIT_BYTES,
    ));
    let mut stderr_task = tokio::spawn(read_git_output_limited(
        stderr,
        "stderr",
        GIT_STDERR_LIMIT_BYTES,
    ));
    let timeout_sleep = sleep(duration);
    tokio::pin!(timeout_sleep);

    let mut status: Option<ExitStatus> = None;
    let mut stdout_result: Option<Vec<u8>> = None;
    let mut stderr_result: Option<Vec<u8>> = None;

    loop {
        if status.is_some() && stdout_result.is_some() && stderr_result.is_some() {
            break;
        }

        tokio::select! {
            result = &mut stdout_task, if stdout_result.is_none() => {
                match join_git_output_task("stdout", result) {
                    Ok(output) => stdout_result = Some(output),
                    Err(err) => {
                        abort_git_child(&mut child, &mut stdout_task, &mut stderr_task).await;
                        return Err(err);
                    }
                }
            }
            result = &mut stderr_task, if stderr_result.is_none() => {
                match join_git_output_task("stderr", result) {
                    Ok(output) => stderr_result = Some(output),
                    Err(err) => {
                        abort_git_child(&mut child, &mut stdout_task, &mut stderr_task).await;
                        return Err(err);
                    }
                }
            }
            wait_result = child.wait(), if status.is_none() => {
                match wait_result {
                    Ok(value) => status = Some(value),
                    Err(err) => {
                        abort_git_child(&mut child, &mut stdout_task, &mut stderr_task).await;
                        return Err(err.to_string());
                    }
                }
            }
            _ = &mut timeout_sleep => {
                abort_git_child(&mut child, &mut stdout_task, &mut stderr_task).await;
                return Err("执行 git 命令超时".to_string());
            }
        }
    }

    let status = status.ok_or("missing git exit status")?;
    Ok(RawGitCommandOutput {
        success: status.success(),
        status: status.to_string(),
        stdout: stdout_result.unwrap_or_default(),
        stderr: stderr_result.unwrap_or_default(),
    })
}

async fn abort_git_child(
    child: &mut tokio::process::Child,
    stdout_task: &mut JoinHandle<Result<Vec<u8>, String>>,
    stderr_task: &mut JoinHandle<Result<Vec<u8>, String>>,
) {
    let _ = child.kill().await;
    stdout_task.abort();
    stderr_task.abort();
}

async fn read_git_output_limited<R>(
    mut reader: R,
    stream_label: &'static str,
    limit_bytes: usize,
) -> Result<Vec<u8>, String>
where
    R: AsyncRead + Unpin,
{
    let mut output = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = reader
            .read(&mut buffer)
            .await
            .map_err(|err| err.to_string())?;
        if read == 0 {
            return Ok(output);
        }
        let next_len = output.len().saturating_add(read);
        ensure_git_output_within_limit(stream_label, next_len, limit_bytes)?;
        output.extend_from_slice(&buffer[..read]);
    }
}

fn join_git_output_task(
    stream_label: &str,
    result: Result<Result<Vec<u8>, String>, tokio::task::JoinError>,
) -> Result<Vec<u8>, String> {
    result.map_err(|err| format!("git {stream_label} reader failed: {err}"))?
}

fn ensure_git_output_within_limit(
    stream_label: &str,
    actual_bytes: usize,
    limit_bytes: usize,
) -> Result<(), String> {
    if actual_bytes > limit_bytes {
        return Err(format!(
            "git {stream_label} exceeded output limit: {actual_bytes} bytes > {limit_bytes} bytes"
        ));
    }
    Ok(())
}

pub(super) fn resolve_git_binary() -> GitBinaryResolution {
    let bundled_candidates = bundled_git_candidates();
    if let Some(path) = env::var_os("CHATOS_GIT_BIN")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        return GitBinaryResolution {
            path,
            source: GitBinarySource::Env,
            bundled_candidates,
        };
    }
    if let Some(path) = bundled_candidates
        .iter()
        .find(|path| path.is_file())
        .cloned()
    {
        return GitBinaryResolution {
            path,
            source: GitBinarySource::Bundled,
            bundled_candidates,
        };
    }
    GitBinaryResolution {
        path: PathBuf::from("git"),
        source: GitBinarySource::System,
        bundled_candidates,
    }
}

fn add_git_bin_dir_to_path(command: &mut Command, git_bin: &Path) {
    let Some(parent) = git_bin.parent().filter(|path| !path.as_os_str().is_empty()) else {
        return;
    };
    let git_root = parent.parent().unwrap_or(parent);
    let mut paths = vec![
        parent.to_path_buf(),
        git_root.join("libexec").join("git-core"),
        git_root.join("mingw64").join("libexec").join("git-core"),
        git_root.join("usr").join("bin"),
        git_root.join("cmd"),
    ];
    paths.retain(|path| path.is_dir());
    if let Some(existing) = env::var_os("PATH") {
        paths.extend(env::split_paths(&existing));
    }
    if let Ok(joined) = env::join_paths(paths) {
        command.env("PATH", joined);
    }
}

fn bundled_git_candidates() -> Vec<PathBuf> {
    let git_exe = git_executable_name();
    let mut candidates = Vec::new();
    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            candidates.push(exe_dir.join("git").join("bin").join(git_exe));
            candidates.push(exe_dir.join("git").join("cmd").join(git_exe));
            candidates.push(
                exe_dir
                    .join("resources")
                    .join("git")
                    .join("bin")
                    .join(git_exe),
            );
            candidates.push(
                exe_dir
                    .join("resources")
                    .join("git")
                    .join("cmd")
                    .join(git_exe),
            );
            candidates.push(
                exe_dir
                    .parent()
                    .unwrap_or(exe_dir)
                    .join("Resources")
                    .join("git")
                    .join("bin")
                    .join(git_exe),
            );
            candidates.push(
                exe_dir
                    .parent()
                    .unwrap_or(exe_dir)
                    .join("Resources")
                    .join("git")
                    .join("cmd")
                    .join(git_exe),
            );
        }
    }
    if let Ok(cwd) = env::current_dir() {
        candidates.push(cwd.join("resources").join("git").join("bin").join(git_exe));
        candidates.push(cwd.join("resources").join("git").join("cmd").join(git_exe));
        candidates.push(
            cwd.join("resources")
                .join("git")
                .join("mingw64")
                .join("bin")
                .join(git_exe),
        );
        candidates.push(
            cwd.join("resources")
                .join("git")
                .join("usr")
                .join("bin")
                .join(git_exe),
        );
        candidates.push(cwd.join("vendor").join("git").join("bin").join(git_exe));
        candidates.push(cwd.join("vendor").join("git").join("cmd").join(git_exe));
    }
    candidates
}

fn git_executable_name() -> &'static str {
    if cfg!(windows) {
        "git.exe"
    } else {
        "git"
    }
}

fn git_launch_error(git_bin: &Path, err: std::io::Error) -> String {
    format!(
        "执行 Git 客户端失败: {}。请安装 Git，或设置 CHATOS_GIT_BIN 指向内置 Git 可执行文件。当前尝试路径: {}",
        err,
        git_bin.to_string_lossy()
    )
}

#[cfg(test)]
mod tests {
    use super::ensure_git_output_within_limit;

    #[test]
    fn git_output_limit_accepts_boundary_size() {
        assert!(ensure_git_output_within_limit("stdout", 1024, 1024).is_ok());
    }

    #[test]
    fn git_output_limit_rejects_oversized_output() {
        let err = ensure_git_output_within_limit("stderr", 1025, 1024)
            .expect_err("oversized output should fail");

        assert!(err.contains("exceeded output limit"));
        assert!(err.contains("1025 bytes > 1024 bytes"));
    }
}
