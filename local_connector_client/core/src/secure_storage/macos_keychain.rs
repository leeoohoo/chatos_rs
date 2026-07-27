// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;

const KEYCHAIN_COMMAND_TIMEOUT: Duration = Duration::from_secs(15);

pub(super) fn load(service: &str, account: &str) -> Result<Option<Vec<u8>>> {
    let output = run_command_with_timeout(
        "security",
        &["find-generic-password", "-a", account, "-s", service, "-w"],
        None,
        KEYCHAIN_COMMAND_TIMEOUT,
    )?;
    if !output.status.success() {
        if output.status.code() == Some(44) {
            return Ok(None);
        }
        return Err(command_failed("read macOS Keychain secret", &output));
    }
    let mut encoded = output.stdout;
    while encoded.last().is_some_and(u8::is_ascii_whitespace) {
        encoded.pop();
    }
    let decoded = URL_SAFE_NO_PAD
        .decode(encoded.as_slice())
        .map(Some)
        .map_err(|err| anyhow!("decode macOS Keychain secret failed: {err}"));
    encoded.fill(0);
    decoded
}

pub(super) fn save(service: &str, account: &str, value: &[u8]) -> Result<()> {
    let mut encoded = URL_SAFE_NO_PAD.encode(value).into_bytes();
    let output = run_command_with_timeout(
        "security",
        &[
            "add-generic-password",
            "-a",
            account,
            "-s",
            service,
            "-U",
            "-w",
        ],
        Some(encoded.as_slice()),
        KEYCHAIN_COMMAND_TIMEOUT,
    );
    encoded.fill(0);
    let output = output?;
    if !output.status.success() {
        return Err(command_failed("store macOS Keychain secret", &output));
    }
    Ok(())
}

pub(super) fn delete(service: &str, account: &str) -> Result<bool> {
    let output = run_command_with_timeout(
        "security",
        &["delete-generic-password", "-a", account, "-s", service],
        None,
        KEYCHAIN_COMMAND_TIMEOUT,
    )?;
    if output.status.success() {
        return Ok(true);
    }
    if output.status.code() == Some(44) {
        return Ok(false);
    }
    Err(command_failed("delete macOS Keychain secret", &output))
}

fn run_command_with_timeout(
    program: &str,
    args: &[&str],
    input: Option<&[u8]>,
    timeout: Duration,
) -> Result<Output> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("start {program}"))?;
    if let Some(input) = input {
        let write_result = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("open {program} stdin"))
            .and_then(|mut stdin| {
                stdin
                    .write_all(input)
                    .and_then(|_| stdin.write_all(b"\n"))
                    .map_err(anyhow::Error::from)
            });
        if let Err(error) = write_result {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error).with_context(|| format!("write {program} stdin"));
        }
    }

    let deadline = Instant::now() + timeout;
    loop {
        if child
            .try_wait()
            .with_context(|| format!("poll {program}"))?
            .is_some()
        {
            return child
                .wait_with_output()
                .with_context(|| format!("collect {program} output"));
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(anyhow!(
                "{program} timed out after {} seconds",
                timeout.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn command_failed(action: &str, output: &Output) -> anyhow::Error {
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if detail.is_empty() {
        anyhow!("{action} failed with status {}", output.status)
    } else {
        anyhow!("{action} failed: {detail}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_timeout_stops_a_hung_keychain_process() {
        let error = run_command_with_timeout(
            "/bin/sh",
            &["-c", "sleep 1"],
            None,
            Duration::from_millis(50),
        )
        .expect_err("command should time out");

        assert!(error.to_string().contains("timed out"));
    }

    #[test]
    fn command_input_is_sent_over_stdin_instead_of_arguments() {
        let output = run_command_with_timeout(
            "/bin/sh",
            &["-c", "read value; printf %s \"$value\""],
            Some(b"private-value"),
            Duration::from_secs(1),
        )
        .expect("command should succeed");

        assert!(output.status.success());
        assert_eq!(output.stdout, b"private-value");
    }
}
