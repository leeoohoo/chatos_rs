// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::Read;
use std::path::Path;
#[cfg(windows)]
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;

use super::{ensure_not_cancelled, render_error, COMMAND_POLL_INTERVAL, MAX_COMMAND_OUTPUT_BYTES};

#[derive(Debug)]
pub(super) struct CommandOutput {
    pub(super) status: ExitStatus,
    pub(super) stdout: Vec<u8>,
    pub(super) stderr: Vec<u8>,
    pub(super) output_truncated: bool,
}

pub(super) fn run_bounded_command(
    program: &Path,
    arguments: &[OsString],
    current_dir: &Path,
    environment: &BTreeMap<String, OsString>,
    timeout: Duration,
    action_cancelled: Option<&AtomicBool>,
    phase: &str,
) -> Result<CommandOutput> {
    ensure_not_cancelled(action_cancelled)?;
    let mut command = Command::new(program);
    command
        .args(arguments)
        .current_dir(current_dir)
        .env_clear()
        .envs(environment)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_process_group(&mut command);
    let mut child = command.spawn().map_err(|error| {
        render_error(
            "runtime_unavailable",
            format!("start packaged document {phase} runtime: {error}"),
        )
    })?;
    let pid = child.id();
    let stdout = child.stdout.take().ok_or_else(|| {
        render_error(
            "runtime_failed",
            format!("document {phase} stdout is unavailable"),
        )
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        render_error(
            "runtime_failed",
            format!("document {phase} stderr is unavailable"),
        )
    })?;
    let stdout_reader = thread::spawn(move || read_capped(stdout));
    let stderr_reader = thread::spawn(move || read_capped(stderr));
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|error| {
            render_error(
                "runtime_failed",
                format!("poll document {phase} runtime: {error}"),
            )
        })? {
            break status;
        }
        if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)) {
            terminate_process_tree(&mut child, pid);
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(render_error(
                "cancelled",
                format!("document {phase} was cancelled and its process tree was terminated"),
            ));
        }
        if started.elapsed() >= timeout {
            terminate_process_tree(&mut child, pid);
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(render_error(
                "timeout",
                format!("document {phase} timed out and its process tree was terminated"),
            ));
        }
        thread::sleep(COMMAND_POLL_INTERVAL);
    };
    let (stdout, stdout_truncated) = join_capped_reader(stdout_reader, phase, "stdout")?;
    let (stderr, stderr_truncated) = join_capped_reader(stderr_reader, phase, "stderr")?;
    Ok(CommandOutput {
        status,
        stdout,
        stderr,
        output_truncated: stdout_truncated || stderr_truncated,
    })
}

#[cfg(unix)]
fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    command.process_group(0);
}

#[cfg(windows)]
fn configure_process_group(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
}

#[cfg(not(any(unix, windows)))]
fn configure_process_group(_command: &mut Command) {}

#[cfg(unix)]
fn terminate_process_tree(child: &mut Child, pid: u32) {
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(windows)]
fn terminate_process_tree(child: &mut Child, pid: u32) {
    let system_root =
        std::env::var_os("SystemRoot").unwrap_or_else(|| OsString::from(r"C:\Windows"));
    let taskkill = PathBuf::from(system_root)
        .join("System32")
        .join("taskkill.exe");
    if taskkill.is_file() {
        let pid = pid.to_string();
        let _ = Command::new(taskkill)
            .args(["/PID", pid.as_str(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(not(any(unix, windows)))]
fn terminate_process_tree(child: &mut Child, _pid: u32) {
    let _ = child.kill();
    let _ = child.wait();
}

fn read_capped(mut reader: impl Read) -> std::io::Result<(Vec<u8>, bool)> {
    let mut stored = Vec::new();
    let mut buffer = [0_u8; 4096];
    let mut truncated = false;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let remaining = MAX_COMMAND_OUTPUT_BYTES.saturating_sub(stored.len());
        let keep = remaining.min(count);
        stored.extend_from_slice(&buffer[..keep]);
        truncated |= keep < count;
    }
    Ok((stored, truncated))
}

fn join_capped_reader(
    reader: thread::JoinHandle<std::io::Result<(Vec<u8>, bool)>>,
    phase: &str,
    stream: &str,
) -> Result<(Vec<u8>, bool)> {
    reader
        .join()
        .map_err(|_| {
            render_error(
                "runtime_failed",
                format!("document {phase} {stream} reader panicked"),
            )
        })?
        .map_err(|error| {
            render_error(
                "runtime_failed",
                format!("read document {phase} {stream}: {error}"),
            )
        })
}

pub(super) fn command_failure(code: &str, message: &str, output: &CommandOutput) -> anyhow::Error {
    let diagnostic = command_diagnostic(output);
    if diagnostic.is_empty() {
        render_error(code, format!("{message}; status={}", output.status))
    } else {
        render_error(
            code,
            format!(
                "{message}; status={}; diagnostic={diagnostic}",
                output.status
            ),
        )
    }
}

fn command_diagnostic(output: &CommandOutput) -> String {
    let bytes = if output.stderr.is_empty() {
        output.stdout.as_slice()
    } else {
        output.stderr.as_slice()
    };
    let mut diagnostic = String::from_utf8_lossy(bytes)
        .chars()
        .map(|character| {
            if character == '\n' || character == '\r' || character == '\t' {
                ' '
            } else if character.is_control() {
                '\u{fffd}'
            } else {
                character
            }
        })
        .take(2_000)
        .collect::<String>();
    diagnostic = diagnostic.split_whitespace().collect::<Vec<_>>().join(" ");
    if output.output_truncated && !diagnostic.is_empty() {
        diagnostic.push_str(" [truncated]");
    }
    diagnostic
}
