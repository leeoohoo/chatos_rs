// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::io::Read;
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use serde_json::Value;

use super::{
    COMPUTER_USE_COMMAND_TIMEOUT, COMPUTER_USE_OUTPUT_MAX_BYTES, COMPUTER_USE_STDERR_MAX_BYTES,
    MACOS_OSASCRIPT_PATH,
};

pub(super) fn execute_jxa(script: &str, arguments: &[String]) -> Result<Value> {
    execute_jxa_with_policy(script, arguments, true)
}

pub(super) fn execute_jxa_action(script: &str, arguments: &[String]) -> Result<Value> {
    execute_jxa_with_policy(script, arguments, false)
}

fn execute_jxa_with_policy(
    script: &str,
    arguments: &[String],
    mark_read_only: bool,
) -> Result<Value> {
    let mut command = Command::new(MACOS_OSASCRIPT_PATH);
    command
        .args(["-l", "JavaScript", "-e", script, "--"])
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .context("start macOS Computer Use observer")?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("Computer Use observer stdout is unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("Computer Use observer stderr is unavailable"))?;
    let stdout_reader =
        thread::spawn(move || read_limited(stdout, "stdout", COMPUTER_USE_OUTPUT_MAX_BYTES));
    let stderr_reader =
        thread::spawn(move || read_limited(stderr, "stderr", COMPUTER_USE_STDERR_MAX_BYTES));
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().context("poll Computer Use observer")? {
            break status;
        }
        if started.elapsed() >= COMPUTER_USE_COMMAND_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(anyhow!(
                "Computer Use observation timed out after {} seconds",
                COMPUTER_USE_COMMAND_TIMEOUT.as_secs()
            ));
        }
        thread::sleep(Duration::from_millis(25));
    };
    let stdout = join_reader(stdout_reader, "stdout")?;
    let stderr = join_reader(stderr_reader, "stderr")?;
    decode_jxa_result_with_policy(status, stdout.as_slice(), stderr.as_slice(), mark_read_only)
}

pub(super) fn read_limited<R: Read>(mut reader: R, label: &str, limit: usize) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .with_context(|| format!("read Computer Use observer {label}"))?;
        if read == 0 {
            return Ok(output);
        }
        if output.len().saturating_add(read) > limit {
            return Err(anyhow!(
                "Computer Use observer {label} exceeded {limit} bytes"
            ));
        }
        output.extend_from_slice(&buffer[..read]);
    }
}

pub(super) fn join_reader(
    handle: thread::JoinHandle<Result<Vec<u8>>>,
    label: &str,
) -> Result<Vec<u8>> {
    handle
        .join()
        .map_err(|_| anyhow!("Computer Use observer {label} reader panicked"))?
}

#[cfg(test)]
pub(super) fn decode_jxa_result(status: ExitStatus, stdout: &[u8], stderr: &[u8]) -> Result<Value> {
    decode_jxa_result_with_policy(status, stdout, stderr, true)
}

fn decode_jxa_result_with_policy(
    status: ExitStatus,
    stdout: &[u8],
    stderr: &[u8],
    mark_read_only: bool,
) -> Result<Value> {
    let stdout = String::from_utf8_lossy(stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(stderr).trim().to_string();
    if !status.success() {
        return Err(classify_macos_observer_error(stderr.as_str(), status));
    }
    if stdout.is_empty() {
        return Err(anyhow!("Computer Use observer returned no JSON output"));
    }
    let mut value: Value = serde_json::from_str(stdout.as_str())
        .context("decode Computer Use observer JSON output")?;
    let map = value
        .as_object_mut()
        .ok_or_else(|| anyhow!("Computer Use observer output must be a JSON object"))?;
    if mark_read_only {
        map.insert("success".to_string(), Value::Bool(true));
        map.insert("mode".to_string(), Value::String("read_only".to_string()));
        map.insert(
            "sensitive_text_policy".to_string(),
            Value::String("editable_values_redacted".to_string()),
        );
    }
    Ok(value)
}

pub(super) fn classify_macos_observer_error(stderr: &str, status: ExitStatus) -> anyhow::Error {
    let normalized = stderr.to_ascii_lowercase();
    if [
        "not authorized",
        "not allowed assistive access",
        "not permitted",
        "(-1719)",
        "(-1743)",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
    {
        return anyhow!("macOS Accessibility permission is required for Computer Use observation");
    }
    if stderr.is_empty() {
        anyhow!("Computer Use observer failed with status {status}")
    } else {
        anyhow!("Computer Use observer failed: {stderr}")
    }
}

pub(super) fn classify_macos_screenshot_error(stderr: &str, status: ExitStatus) -> anyhow::Error {
    let normalized = stderr.to_ascii_lowercase();
    if normalized.contains("not authorized")
        || normalized.contains("not permitted")
        || normalized.contains("screen recording")
        || normalized.contains("could not create image from display")
    {
        return anyhow!(
            "macOS Screen Recording permission is required for Computer Use screenshots"
        );
    }
    if stderr.is_empty() {
        anyhow!("Computer Use screenshot failed with status {status}")
    } else {
        anyhow!("Computer Use screenshot failed: {stderr}")
    }
}
