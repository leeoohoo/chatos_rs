// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::env;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};

use serde_json::{json, Value};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;
use tokio::time::{sleep, Duration};
use uuid::Uuid;

use crate::bundled_tools::agent_browser_binary_path;

const BROWSER_STDOUT_LIMIT_BYTES: usize = 4 * 1024 * 1024;
const BROWSER_STDERR_LIMIT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub(crate) struct BrowserRuntimeSession {
    pub(crate) session_name: String,
    pub(crate) cdp_url: Option<String>,
}

pub(crate) fn new_browser_session() -> BrowserRuntimeSession {
    let cdp_override = env::var("BROWSER_CDP_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    BrowserRuntimeSession {
        session_name: if cdp_override.is_some() {
            format!("cdp_{}", Uuid::new_v4().simple())
        } else {
            format!("h_{}", Uuid::new_v4().simple())
        },
        cdp_url: cdp_override,
    }
}

pub fn browser_backend_available() -> Result<(), String> {
    resolve_agent_browser_cmd()?;
    if let Some(path) = env::var_os("AGENT_BROWSER_EXECUTABLE_PATH") {
        let path = PathBuf::from(path);
        if !path.is_file() {
            return Err(format!(
                "Chrome for Testing is missing from the Local Connector packaged runtime: {}",
                path.display()
            ));
        }
    }
    Ok(())
}

pub(crate) async fn run_browser_command(
    workspace_dir: &Path,
    session: &BrowserRuntimeSession,
    command: &str,
    args: Vec<String>,
    timeout_seconds: u64,
) -> Result<Value, String> {
    browser_backend_available()?;
    let program = resolve_agent_browser_cmd()?;
    let mut cmd = Command::new(program);
    cmd.current_dir(workspace_dir);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(cdp_url) = session.cdp_url.as_deref() {
        cmd.arg("--cdp").arg(cdp_url);
    } else {
        cmd.arg("--session").arg(session.session_name.as_str());
    }
    cmd.arg("--json").arg(command);
    for value in args {
        cmd.arg(value);
    }

    let mut child = cmd
        .spawn()
        .map_err(|err| format!("spawn browser command failed: {}", err))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "missing browser stdout".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "missing browser stderr".to_string())?;
    let output = match collect_browser_output_limited(
        &mut child,
        stdout,
        stderr,
        Duration::from_secs(timeout_seconds.max(1)),
    )
    .await
    {
        Ok(output) => output,
        Err(err) => {
            return Ok(json!({
                "success": false,
                "error": err,
            }));
        }
    };

    let stdout_text = String::from_utf8_lossy(output.stdout.as_slice())
        .trim()
        .to_string();
    let stderr_text = String::from_utf8_lossy(output.stderr.as_slice())
        .trim()
        .to_string();
    if stdout_text.is_empty()
        && output.status.success()
        && command != "close"
        && command != "record"
    {
        return Ok(json!({
            "success": false,
            "error": format!("Browser command '{}' returned no output", command)
        }));
    }

    if !stdout_text.is_empty() {
        match serde_json::from_str::<Value>(&stdout_text) {
            Ok(parsed) => return Ok(parsed),
            Err(err) => {
                return Ok(json!({
                    "success": false,
                    "error": format!(
                        "Non-JSON output from agent-browser for '{}': {}",
                        command,
                        truncate_chars(&stdout_text, 2000)
                    ),
                    "detail": err.to_string(),
                }));
            }
        }
    }

    if !output.status.success() {
        return Ok(json!({
            "success": false,
            "error": if stderr_text.is_empty() {
                format!("Browser command failed with status {}", output.status)
            } else {
                stderr_text
            }
        }));
    }

    Ok(json!({
        "success": true,
        "data": {}
    }))
}

struct BrowserCommandOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

async fn collect_browser_output_limited(
    child: &mut Child,
    stdout: tokio::process::ChildStdout,
    stderr: tokio::process::ChildStderr,
    duration: Duration,
) -> Result<BrowserCommandOutput, String> {
    let mut stdout_task = tokio::spawn(read_browser_stream_limited(
        stdout,
        "stdout",
        BROWSER_STDOUT_LIMIT_BYTES,
    ));
    let mut stderr_task = tokio::spawn(read_browser_stream_limited(
        stderr,
        "stderr",
        BROWSER_STDERR_LIMIT_BYTES,
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
                match join_browser_stream_task("stdout", result) {
                    Ok(output) => stdout_result = Some(output),
                    Err(err) => {
                        abort_browser_child(child, &mut stdout_task, &mut stderr_task).await;
                        return Err(err);
                    }
                }
            }
            result = &mut stderr_task, if stderr_result.is_none() => {
                match join_browser_stream_task("stderr", result) {
                    Ok(output) => stderr_result = Some(output),
                    Err(err) => {
                        abort_browser_child(child, &mut stdout_task, &mut stderr_task).await;
                        return Err(err);
                    }
                }
            }
            wait_result = child.wait(), if status.is_none() => {
                match wait_result {
                    Ok(value) => status = Some(value),
                    Err(err) => {
                        abort_browser_child(child, &mut stdout_task, &mut stderr_task).await;
                        return Err(format!("wait browser command failed: {err}"));
                    }
                }
            }
            _ = &mut timeout_sleep => {
                abort_browser_child(child, &mut stdout_task, &mut stderr_task).await;
                return Err(format!("Command timed out after {} seconds", duration.as_secs()));
            }
        }
    }

    Ok(BrowserCommandOutput {
        status: status.ok_or_else(|| "missing browser exit status".to_string())?,
        stdout: stdout_result.unwrap_or_default(),
        stderr: stderr_result.unwrap_or_default(),
    })
}

async fn abort_browser_child(
    child: &mut Child,
    stdout_task: &mut JoinHandle<Result<Vec<u8>, String>>,
    stderr_task: &mut JoinHandle<Result<Vec<u8>, String>>,
) {
    let _ = child.kill().await;
    stdout_task.abort();
    stderr_task.abort();
}

async fn read_browser_stream_limited<R>(
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
            .map_err(|err| format!("read browser {stream_label} failed: {err}"))?;
        if read == 0 {
            return Ok(output);
        }
        let next_len = output.len().saturating_add(read);
        ensure_browser_stream_within_limit(stream_label, next_len, limit_bytes)?;
        output.extend_from_slice(&buffer[..read]);
    }
}

fn join_browser_stream_task(
    stream_label: &str,
    result: Result<Result<Vec<u8>, String>, tokio::task::JoinError>,
) -> Result<Vec<u8>, String> {
    result.map_err(|err| format!("read browser {stream_label} join failed: {err}"))?
}

fn ensure_browser_stream_within_limit(
    stream_label: &str,
    actual_bytes: usize,
    limit_bytes: usize,
) -> Result<(), String> {
    if actual_bytes > limit_bytes {
        return Err(format!(
            "browser {stream_label} exceeded output limit: {actual_bytes} bytes > {limit_bytes} bytes"
        ));
    }
    Ok(())
}

fn resolve_agent_browser_cmd() -> Result<PathBuf, String> {
    agent_browser_binary_path().ok_or_else(|| {
        "agent-browser CLI is missing from the Local Connector packaged runtime".to_string()
    })
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let total = text.chars().count();
    if total <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::ensure_browser_stream_within_limit;

    #[test]
    fn browser_stream_limit_accepts_boundary_size() {
        assert!(ensure_browser_stream_within_limit("stdout", 1024, 1024).is_ok());
    }

    #[test]
    fn browser_stream_limit_rejects_oversized_output() {
        let err = ensure_browser_stream_within_limit("stderr", 1025, 1024)
            .expect_err("oversized output should fail");

        assert!(err.contains("exceeded output limit"));
        assert!(err.contains("1025 bytes > 1024 bytes"));
    }
}
