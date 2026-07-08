// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fmt;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::task::JoinHandle;
use tokio::time::sleep;

#[derive(Debug)]
pub(crate) struct BoundedCommandOutput {
    pub(crate) status: ExitStatus,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
}

#[derive(Debug)]
pub(crate) enum BoundedCommandError {
    MissingPipe(&'static str),
    OutputLimit(String),
    Read(String),
    Spawn(std::io::Error),
    Timeout,
    Wait(String),
}

impl fmt::Display for BoundedCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPipe(stream) => write!(formatter, "missing process {stream}"),
            Self::OutputLimit(message) => formatter.write_str(message),
            Self::Read(message) => formatter.write_str(message),
            Self::Spawn(err) => write!(formatter, "process spawn failed: {err}"),
            Self::Timeout => formatter.write_str("process timed out"),
            Self::Wait(message) => formatter.write_str(message),
        }
    }
}

pub(crate) async fn run_command_limited(
    mut command: Command,
    duration: Duration,
    stdout_limit_bytes: usize,
    stderr_limit_bytes: usize,
    label: &'static str,
) -> Result<BoundedCommandOutput, BoundedCommandError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = command.spawn().map_err(BoundedCommandError::Spawn)?;
    let stdout = child
        .stdout
        .take()
        .ok_or(BoundedCommandError::MissingPipe("stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or(BoundedCommandError::MissingPipe("stderr"))?;
    let mut stdout_task = tokio::spawn(read_process_output_limited(
        stdout,
        label,
        "stdout",
        stdout_limit_bytes,
    ));
    let mut stderr_task = tokio::spawn(read_process_output_limited(
        stderr,
        label,
        "stderr",
        stderr_limit_bytes,
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
                match join_process_output_task("stdout", result) {
                    Ok(output) => stdout_result = Some(output),
                    Err(err) => {
                        abort_process(&mut child, &mut stdout_task, &mut stderr_task).await;
                        return Err(err);
                    }
                }
            }
            result = &mut stderr_task, if stderr_result.is_none() => {
                match join_process_output_task("stderr", result) {
                    Ok(output) => stderr_result = Some(output),
                    Err(err) => {
                        abort_process(&mut child, &mut stdout_task, &mut stderr_task).await;
                        return Err(err);
                    }
                }
            }
            wait_result = child.wait(), if status.is_none() => {
                match wait_result {
                    Ok(value) => status = Some(value),
                    Err(err) => {
                        abort_process(&mut child, &mut stdout_task, &mut stderr_task).await;
                        return Err(BoundedCommandError::Wait(err.to_string()));
                    }
                }
            }
            _ = &mut timeout_sleep => {
                abort_process(&mut child, &mut stdout_task, &mut stderr_task).await;
                return Err(BoundedCommandError::Timeout);
            }
        }
    }

    Ok(BoundedCommandOutput {
        status: status.ok_or(BoundedCommandError::Wait(
            "missing process exit status".to_string(),
        ))?,
        stdout: stdout_result.unwrap_or_default(),
        stderr: stderr_result.unwrap_or_default(),
    })
}

async fn abort_process(
    child: &mut tokio::process::Child,
    stdout_task: &mut JoinHandle<Result<Vec<u8>, BoundedCommandError>>,
    stderr_task: &mut JoinHandle<Result<Vec<u8>, BoundedCommandError>>,
) {
    let _ = child.kill().await;
    stdout_task.abort();
    stderr_task.abort();
}

async fn read_process_output_limited<R>(
    mut reader: R,
    label: &'static str,
    stream_label: &'static str,
    limit_bytes: usize,
) -> Result<Vec<u8>, BoundedCommandError>
where
    R: AsyncRead + Unpin,
{
    let mut output = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = reader
            .read(&mut buffer)
            .await
            .map_err(|err| BoundedCommandError::Read(err.to_string()))?;
        if read == 0 {
            return Ok(output);
        }
        let next_len = output.len().saturating_add(read);
        ensure_process_output_within_limit(label, stream_label, next_len, limit_bytes)
            .map_err(BoundedCommandError::OutputLimit)?;
        output.extend_from_slice(&buffer[..read]);
    }
}

fn join_process_output_task(
    stream_label: &str,
    result: Result<Result<Vec<u8>, BoundedCommandError>, tokio::task::JoinError>,
) -> Result<Vec<u8>, BoundedCommandError> {
    result.map_err(|err| {
        BoundedCommandError::Read(format!("{stream_label} reader task failed: {err}"))
    })?
}

pub(crate) fn ensure_process_output_within_limit(
    label: &str,
    stream_label: &str,
    actual_bytes: usize,
    limit_bytes: usize,
) -> Result<(), String> {
    if actual_bytes > limit_bytes {
        return Err(format!(
            "{label} {stream_label} exceeded output limit: {actual_bytes} bytes > {limit_bytes} bytes"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ensure_process_output_within_limit;

    #[test]
    fn process_output_limit_accepts_boundary_size() {
        assert!(ensure_process_output_within_limit("ssh", "stdout", 1024, 1024).is_ok());
    }

    #[test]
    fn process_output_limit_rejects_oversized_output() {
        let err = ensure_process_output_within_limit("scp", "stderr", 1025, 1024)
            .expect_err("oversized output should fail");

        assert!(err.contains("exceeded output limit"));
        assert!(err.contains("1025 bytes > 1024 bytes"));
    }
}
