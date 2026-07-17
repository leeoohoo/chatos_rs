// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use sha2::Digest as _;

const KEYCHAIN_COMMAND_TIMEOUT: Duration = Duration::from_secs(15);
const KEYCHAIN_SERVICE: &str = "Chat OS Local Connector Device Key";

pub(super) fn load(path: &Path) -> Result<Option<Vec<u8>>> {
    let account = keychain_account(path);
    let output = run_command_with_timeout(
        "security",
        &[
            "find-generic-password",
            "-a",
            account.as_str(),
            "-s",
            KEYCHAIN_SERVICE,
            "-w",
        ],
        None,
        KEYCHAIN_COMMAND_TIMEOUT,
    )?;
    if !output.status.success() {
        if output.status.code() == Some(44) {
            return Ok(None);
        }
        return Err(command_failed("read macOS Keychain device key", &output));
    }
    let value = String::from_utf8(output.stdout)?;
    URL_SAFE_NO_PAD
        .decode(value.trim().as_bytes())
        .map(Some)
        .map_err(|err| anyhow!("decode macOS Keychain local connector device key failed: {err}"))
}

pub(super) fn save(path: &Path, value: &[u8]) -> Result<()> {
    let account = keychain_account(path);
    let encoded = URL_SAFE_NO_PAD.encode(value);
    let output = run_command_with_timeout(
        "security",
        &[
            "add-generic-password",
            "-a",
            account.as_str(),
            "-s",
            KEYCHAIN_SERVICE,
            "-U",
            "-w",
        ],
        Some(encoded.as_bytes()),
        KEYCHAIN_COMMAND_TIMEOUT,
    )?;
    if !output.status.success() {
        return Err(command_failed("store macOS Keychain device key", &output));
    }
    Ok(())
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

fn keychain_account(path: &Path) -> String {
    let digest = sha2::Sha256::digest(path.to_string_lossy().as_bytes());
    format!("chatos-local-connector-{}", hex::encode(digest))
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
