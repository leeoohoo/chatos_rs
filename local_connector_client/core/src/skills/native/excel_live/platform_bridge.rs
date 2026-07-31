// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

use super::macos_range_write_script::macos_range_write_script;
use super::platform_snapshot_scripts::{
    MACOS_SNAPSHOT_SCRIPT, MACOS_STATUS_SCRIPT, WINDOWS_SNAPSHOT_SCRIPT, WINDOWS_STATUS_SCRIPT,
};
use super::range_read_scripts::{macos_range_read_script, windows_range_read_script};
use super::windows_range_write_script::windows_range_write_script;

const MAX_BRIDGE_OUTPUT_BYTES: u64 = 512 * 1024;
const BRIDGE_TIMEOUT: Duration = Duration::from_secs(8);
const WRITE_BRIDGE_TIMEOUT: Duration = Duration::from_secs(20);
const MACOS_OSASCRIPT_PATH: &str = "/usr/bin/osascript";
const MACOS_EXCEL_APPLICATION_PATH: &str = "/Applications/Microsoft Excel.app";

pub(super) fn dependency_error() -> Option<String> {
    match std::env::consts::OS {
        "macos" if !regular_non_symlink_file(Path::new(MACOS_OSASCRIPT_PATH)) => Some(
            "Excel Live Control requires the system osascript automation bridge on macOS"
                .to_string(),
        ),
        "macos" => None,
        "windows" => windows_powershell_path()
            .err()
            .map(|error| error.to_string()),
        _ => {
            Some("Excel Live Control is currently available only on macOS and Windows".to_string())
        }
    }
}

pub(super) fn read_platform_snapshot() -> Result<Value> {
    match std::env::consts::OS {
        "macos" => read_macos_snapshot(),
        "windows" => read_windows_snapshot(),
        _ => Err(anyhow!(
            "Excel Live Control is currently available only on macOS and Windows"
        )),
    }
}

pub(super) fn read_platform_status() -> Result<Value> {
    match std::env::consts::OS {
        "macos" => read_macos_status(),
        "windows" => read_windows_status(),
        _ => Err(anyhow!(
            "Excel Live Control is currently available only on macOS and Windows"
        )),
    }
}

pub(super) fn read_platform_range(request: &Value) -> Result<Value> {
    match std::env::consts::OS {
        "macos" => {
            let script = macos_range_read_script();
            run_json_command_with_stdin(
                MACOS_OSASCRIPT_PATH,
                &["-l", "JavaScript", "-e", script.as_str()],
                request,
                "macOS Excel bounded range bridge",
            )
        }
        "windows" => {
            let powershell = windows_powershell_path()?;
            let script = windows_range_read_script();
            run_json_command_with_stdin(
                powershell.as_path(),
                &[
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    script.as_str(),
                ],
                request,
                "Windows Excel bounded range bridge",
            )
        }
        _ => Err(anyhow!(
            "Excel Live Control is currently available only on macOS and Windows"
        )),
    }
}

pub(super) fn write_platform_range(request: &Value) -> Result<Value> {
    match std::env::consts::OS {
        "macos" => {
            let script = macos_range_write_script();
            run_json_command_with_stdin(
                MACOS_OSASCRIPT_PATH,
                &["-l", "JavaScript", "-e", script.as_str()],
                request,
                "macOS Excel bounded range write bridge",
            )
        }
        "windows" => {
            let powershell = windows_powershell_path()?;
            let script = windows_range_write_script();
            run_json_command_with_stdin(
                powershell.as_path(),
                &[
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    script.as_str(),
                ],
                request,
                "Windows Excel bounded range write bridge",
            )
        }
        _ => Err(anyhow!(
            "Excel Live Control is currently available only on macOS and Windows"
        )),
    }
}

fn read_macos_snapshot() -> Result<Value> {
    if !macos_excel_installed() {
        return Ok(json!({
            "schema_version": 1,
            "installed": false,
            "running": false,
            "runtime_instance": null,
            "application_version": null,
            "workbooks_total": 0,
            "workbooks_truncated": false,
            "workbooks": [],
        }));
    }
    run_json_command(
        MACOS_OSASCRIPT_PATH,
        &["-l", "JavaScript", "-e", MACOS_SNAPSHOT_SCRIPT],
        "macOS Excel automation bridge",
    )
}

fn read_macos_status() -> Result<Value> {
    if !macos_excel_installed() {
        return Ok(stopped_platform_snapshot(false));
    }
    run_json_command(
        MACOS_OSASCRIPT_PATH,
        &["-l", "JavaScript", "-e", MACOS_STATUS_SCRIPT],
        "macOS Excel status bridge",
    )
}

fn read_windows_snapshot() -> Result<Value> {
    let powershell = windows_powershell_path()?;
    run_json_command(
        powershell.as_path(),
        &[
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            WINDOWS_SNAPSHOT_SCRIPT,
        ],
        "Windows Excel automation bridge",
    )
}

fn read_windows_status() -> Result<Value> {
    let powershell = windows_powershell_path()?;
    run_json_command(
        powershell.as_path(),
        &[
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            WINDOWS_STATUS_SCRIPT,
        ],
        "Windows Excel status bridge",
    )
}

fn stopped_platform_snapshot(installed: bool) -> Value {
    json!({
        "schema_version": 1,
        "installed": installed,
        "running": false,
        "runtime_instance": null,
        "application_version": null,
        "workbooks_total": 0,
        "workbooks_truncated": false,
        "workbook_metadata_omitted": true,
        "workbooks": [],
    })
}

fn macos_excel_installed() -> bool {
    if regular_non_symlink_dir(Path::new(MACOS_EXCEL_APPLICATION_PATH)) {
        return true;
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| {
            regular_non_symlink_dir(home.join("Applications/Microsoft Excel.app").as_path())
        })
        .unwrap_or(false)
}

fn windows_powershell_path() -> Result<PathBuf> {
    let system_root = std::env::var_os("SystemRoot")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| std::ffi::OsString::from(r"C:\Windows"));
    let candidate = PathBuf::from(system_root)
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    if !candidate.is_absolute() || !regular_non_symlink_file(candidate.as_path()) {
        bail!("Excel Live Control requires the fixed Windows PowerShell system executable");
    }
    Ok(candidate)
}

fn regular_non_symlink_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_file() && !metadata.file_type().is_symlink())
        .unwrap_or(false)
}

fn regular_non_symlink_dir(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_dir() && !metadata.file_type().is_symlink())
        .unwrap_or(false)
}

fn run_json_command<P: AsRef<Path>>(program: P, args: &[&str], label: &str) -> Result<Value> {
    run_json_command_bytes(program, args, None, label)
}

fn run_json_command_with_stdin<P: AsRef<Path>>(
    program: P,
    args: &[&str],
    request: &Value,
    label: &str,
) -> Result<Value> {
    let request = serde_json::to_vec(request).context("encode private Excel bridge request")?;
    if request.len() as u64 > MAX_BRIDGE_OUTPUT_BYTES {
        bail!("private Excel bridge request exceeds the bounded input limit");
    }
    run_json_command_bytes(program, args, Some(request.as_slice()), label)
}

fn run_json_command_bytes<P: AsRef<Path>>(
    program: P,
    args: &[&str],
    stdin: Option<&[u8]>,
    label: &str,
) -> Result<Value> {
    let is_write_bridge = label.contains("range write bridge");
    let mut child = Command::new(program.as_ref())
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("start {label}"))?;
    if let Some(stdin) = stdin {
        let write_result = child
            .stdin
            .take()
            .context("open private Excel bridge stdin")?
            .write_all(stdin)
            .with_context(|| format!("write private request to {label}"));
        if let Err(error) = write_result {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    }
    let stdout_reader = child.stdout.take().context("read Excel bridge stdout")?;
    let stderr_reader = child.stderr.take().context("read Excel bridge stderr")?;
    let stdout_thread =
        thread::spawn(move || read_bounded_pipe(stdout_reader, "Excel bridge stdout"));
    let stderr_thread =
        thread::spawn(move || read_bounded_pipe(stderr_reader, "Excel bridge stderr"));
    let deadline = Instant::now()
        + if is_write_bridge {
            WRITE_BRIDGE_TIMEOUT
        } else {
            BRIDGE_TIMEOUT
        };
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("wait for {label}"))?
        {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            if is_write_bridge {
                bail!("{label} timed out; exact mutation and rollback state could not be verified, so inspect the target range before any retry");
            }
            bail!("{label} timed out without launching or closing Microsoft Excel");
        }
        thread::sleep(Duration::from_millis(20));
    };

    let stdout = stdout_thread
        .join()
        .map_err(|_| anyhow!("Excel bridge stdout reader failed"))?;
    let stderr = stderr_thread
        .join()
        .map_err(|_| anyhow!("Excel bridge stderr reader failed"))?;
    let stdout = match stdout {
        Ok(stdout) => stdout,
        Err(_) if is_write_bridge => bail!(
            "{label} returned an oversized or unreadable result; inspect the target range before any retry"
        ),
        Err(error) => return Err(error),
    };
    let stderr = match stderr {
        Ok(stderr) => stderr,
        Err(_) if is_write_bridge => bail!(
            "{label} returned oversized diagnostics; inspect the target range before any retry"
        ),
        Err(error) => return Err(error),
    };
    if !status.success() {
        let stderr = String::from_utf8_lossy(&stderr);
        if stderr.contains("-1743") || stderr.to_ascii_lowercase().contains("not authorized") {
            bail!(
                "macOS denied Microsoft Excel Automation access; allow ChatOS to control Microsoft Excel in System Settings"
            );
        }
        if is_write_bridge {
            bail!("{label} failed; exact mutation and rollback state could not be verified, so inspect the target range before any retry");
        }
        bail!("{label} failed without changing Microsoft Excel");
    }
    if !stderr.is_empty() {
        if is_write_bridge {
            bail!("{label} returned unexpected diagnostics; inspect the target range before any retry");
        }
        bail!("{label} returned unexpected diagnostic output");
    }
    match serde_json::from_slice(stdout.as_slice()) {
        Ok(value) => Ok(value),
        Err(_) if is_write_bridge => {
            bail!("{label} returned an invalid result; inspect the target range before any retry")
        }
        Err(error) => Err(error).with_context(|| format!("decode bounded {label} response")),
    }
}

fn read_bounded_pipe<R: Read>(reader: R, label: &str) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_BRIDGE_OUTPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .with_context(|| format!("read {label}"))?;
    if bytes.len() as u64 > MAX_BRIDGE_OUTPUT_BYTES {
        bail!("{label} exceeds the bounded output limit");
    }
    Ok(bytes)
}
