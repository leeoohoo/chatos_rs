use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::process::Command;
use tokio::time::timeout;

pub(super) const DEFAULT_GIT_TIMEOUT: Duration = Duration::from_secs(20);
pub(super) const REMOTE_GIT_TIMEOUT: Duration = Duration::from_secs(120);

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
    let output = match timeout(duration, command.output()).await {
        Ok(result) => result.map_err(|err| git_launch_error(git_bin.path.as_path(), err))?,
        Err(_) => return Err("执行 git 命令超时".to_string()),
    };
    Ok(GitCommandStatusOutput {
        success: output.status.success(),
        status: output.status.to_string(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

pub(super) async fn git_version(git_bin: &GitBinaryResolution) -> Result<String, String> {
    let mut command = Command::new(git_bin.path.as_os_str());
    command.arg("--version").env("GIT_TERMINAL_PROMPT", "0");
    add_git_bin_dir_to_path(&mut command, git_bin.path.as_path());
    let output = match timeout(DEFAULT_GIT_TIMEOUT, command.output()).await {
        Ok(result) => result.map_err(|err| git_launch_error(git_bin.path.as_path(), err))?,
        Err(_) => return Err("执行 git --version 超时".to_string()),
    };
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.success() {
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
