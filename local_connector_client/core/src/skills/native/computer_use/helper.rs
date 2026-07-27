// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::ffi::c_void;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tempfile::TempDir;

use super::{
    dependency_error_local, execute_approved_local, execute_local,
    macos_frontmost_window_control_target_local, preflight_window_layout_snapshot_local,
    screen_capture_dependency_error_local, ApprovedFrontmostWindowGuard, WindowLayoutSnapshot,
    CONTROL_OPERATIONS,
};

const HELPER_PROTOCOL_VERSION: u32 = 1;
const HELPER_PROTOCOL_ARGUMENT: &str = "--stdio-v1";
const HELPER_EXECUTABLE_NAME: &str = "chatos_computer_use_helper";
const HELPER_PATH_ENV: &str = "CHATOS_COMPUTER_USE_HELPER_PATH";
const HELPER_REQUIRE_SIGNED_ENV: &str = "CHATOS_COMPUTER_USE_HELPER_REQUIRE_SIGNED";
const MACOS_CODESIGN_PATH: &str = "/usr/bin/codesign";
const CORE_EXECUTABLE_NAME: &str = "local_connector_client_core";
const PROC_PIDPATHINFO_MAXSIZE: usize = 4 * 1024;
const MAX_REQUEST_BYTES: usize = 256 * 1024;
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const MAX_STDERR_BYTES: usize = 64 * 1024;
const MAX_OPERATION_BYTES: usize = 128;
const MAX_APPROVED_ARGUMENTS: usize = 64;
const MAX_APPROVED_ARGUMENT_BYTES: usize = 8 * 1024;
const MAX_APPROVED_ARGUMENTS_BYTES: usize = 32 * 1024;
const HELPER_TIMEOUT: Duration = Duration::from_secs(12);
const HELPER_CANCEL_GRACE: Duration = Duration::from_secs(2);
const HELPER_POLL_INTERVAL: Duration = Duration::from_millis(20);

#[link(name = "proc")]
extern "C" {
    fn proc_pidpath(pid: i32, buffer: *mut c_void, buffersize: u32) -> i32;
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HelperRequest {
    protocol_version: u32,
    #[serde(flatten)]
    command: HelperCommand,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum HelperCommand {
    ProtocolProbe,
    FrontmostWindowControlTarget,
    WindowLayoutPreflight {
        snapshot: WindowLayoutSnapshot,
    },
    DependencyProbe {
        screen_capture_only: bool,
    },
    Execute {
        operation: String,
        arguments: Value,
    },
    ExecuteApproved {
        operation: String,
        arguments: Value,
        approved_command_args: Option<Vec<String>>,
        cancellation_marker: PathBuf,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HelperResponse {
    protocol_version: u32,
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl HelperResponse {
    fn success(result: Value) -> Self {
        Self {
            protocol_version: HELPER_PROTOCOL_VERSION,
            success: true,
            result: Some(result),
            error: None,
        }
    }

    fn error(error: impl Into<String>) -> Self {
        Self {
            protocol_version: HELPER_PROTOCOL_VERSION,
            success: false,
            result: None,
            error: Some(error.into()),
        }
    }
}

pub(super) fn dependency_error() -> Option<String> {
    dependency_probe(false)
}

pub(super) fn screen_capture_dependency_error() -> Option<String> {
    dependency_probe(true)
}

pub(super) fn frontmost_window_control_target() -> Result<ApprovedFrontmostWindowGuard> {
    let request = HelperRequest {
        protocol_version: HELPER_PROTOCOL_VERSION,
        command: HelperCommand::FrontmostWindowControlTarget,
    };
    let result = invoke_helper(&request, None, None)?;
    let target = serde_json::from_value::<ApprovedFrontmostWindowGuard>(result)
        .context("decode helper frontmost window control target")?;
    target.validate()?;
    Ok(target)
}

pub(super) fn preflight_window_layout(snapshot: &WindowLayoutSnapshot) -> Result<()> {
    let request = HelperRequest {
        protocol_version: HELPER_PROTOCOL_VERSION,
        command: HelperCommand::WindowLayoutPreflight {
            snapshot: snapshot.clone(),
        },
    };
    let result = invoke_helper(&request, None, None)?;
    if result.get("validated").and_then(Value::as_bool) != Some(true) {
        bail!("macOS Computer Use helper did not validate the window layout snapshot");
    }
    Ok(())
}

fn dependency_probe(screen_capture_only: bool) -> Option<String> {
    let request = HelperRequest {
        protocol_version: HELPER_PROTOCOL_VERSION,
        command: HelperCommand::DependencyProbe {
            screen_capture_only,
        },
    };
    match invoke_helper(&request, None, None) {
        Ok(result) => result
            .get("dependency_error")
            .and_then(Value::as_str)
            .map(str::to_string),
        Err(error) => Some(format!("macOS Computer Use helper is unavailable: {error}")),
    }
}

pub(super) fn execute(operation: &str, arguments: &Value) -> Result<Value> {
    validate_operation(operation)?;
    let request = HelperRequest {
        protocol_version: HELPER_PROTOCOL_VERSION,
        command: HelperCommand::Execute {
            operation: operation.to_string(),
            arguments: arguments.clone(),
        },
    };
    invoke_helper(&request, None, None)
}

pub(super) fn execute_approved(
    operation: &str,
    arguments: &Value,
    approved_command_args: Option<&[String]>,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    validate_operation(operation)?;
    if !CONTROL_OPERATIONS.contains(&operation) {
        bail!("Computer Use operation does not support approved control: {operation}");
    }
    if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)) {
        bail!("Computer Use action was cancelled");
    }
    validate_approved_arguments(approved_command_args)?;
    let cancellation_directory = tempfile::Builder::new()
        .prefix("chatos-computer-use-cancel-")
        .tempdir()
        .context("create private Computer Use cancellation directory")?;
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(
        cancellation_directory.path(),
        fs::Permissions::from_mode(0o700),
    )
    .context("secure Computer Use cancellation directory")?;
    validate_private_cancellation_directory(cancellation_directory.path())?;
    let cancellation_marker = cancellation_directory.path().join("cancel");
    let request = HelperRequest {
        protocol_version: HELPER_PROTOCOL_VERSION,
        command: HelperCommand::ExecuteApproved {
            operation: operation.to_string(),
            arguments: arguments.clone(),
            approved_command_args: approved_command_args.map(<[String]>::to_vec),
            cancellation_marker: cancellation_marker.clone(),
        },
    };
    invoke_helper(
        &request,
        action_cancelled,
        Some(CancellationClient {
            _directory: cancellation_directory,
            marker: cancellation_marker,
        }),
    )
}

struct CancellationClient {
    _directory: TempDir,
    marker: PathBuf,
}

fn invoke_helper(
    request: &HelperRequest,
    action_cancelled: Option<&AtomicBool>,
    cancellation: Option<CancellationClient>,
) -> Result<Value> {
    let helper_path = helper_path()?;
    validate_helper_binary(helper_path.as_path())?;
    validate_helper_signature(helper_path.as_path())?;

    let request_bytes =
        serde_json::to_vec(request).context("encode Computer Use helper request")?;
    if request_bytes.len() > MAX_REQUEST_BYTES {
        bail!("Computer Use helper request exceeded {MAX_REQUEST_BYTES} bytes");
    }

    let mut child = Command::new(helper_path.as_path())
        .arg(HELPER_PROTOCOL_ARGUMENT)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("start macOS Computer Use helper {}", helper_path.display()))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("Computer Use helper stdin is unavailable"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("Computer Use helper stdout is unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("Computer Use helper stderr is unavailable"))?;
    let stdout_reader = thread::spawn(move || {
        read_limited_stream(stdout, "stdout", MAX_RESPONSE_BYTES.saturating_add(4))
    });
    let stderr_reader =
        thread::spawn(move || read_limited_stream(stderr, "stderr", MAX_STDERR_BYTES));
    if let Err(error) = write_frame(&mut stdin, request_bytes.as_slice(), MAX_REQUEST_BYTES) {
        let _ = child.kill();
        let _ = child.wait();
        let _ = stdout_reader.join();
        let _ = stderr_reader.join();
        return Err(error).context("write Computer Use helper request");
    }
    drop(stdin);

    let started = Instant::now();
    let mut cancellation_started = None;
    let mut timed_out = false;
    let mut cancelled_by_caller = false;
    let mut terminal_error = None;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                terminal_error = Some(anyhow!(error).context("poll Computer Use helper process"));
                cancellation_started.get_or_insert_with(Instant::now);
            }
        }
        let cancelled = action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst));
        if cancellation_started.is_none() && (cancelled || started.elapsed() >= HELPER_TIMEOUT) {
            timed_out = !cancelled;
            cancelled_by_caller = cancelled;
            if let Some(cancellation) = cancellation.as_ref() {
                if let Err(error) = create_cancellation_marker(cancellation.marker.as_path()) {
                    terminal_error = Some(error.context("signal Computer Use helper cancellation"));
                }
            }
            cancellation_started = Some(Instant::now());
        }
        if cancellation_started.is_some_and(|instant| instant.elapsed() >= HELPER_CANCEL_GRACE) {
            let _ = child.kill();
            break child
                .wait()
                .context("wait for cancelled Computer Use helper")?;
        }
        thread::sleep(HELPER_POLL_INTERVAL);
    };
    decode_child_result(
        &mut child,
        status,
        stdout_reader,
        stderr_reader,
        timed_out,
        cancelled_by_caller,
        terminal_error,
    )
}

fn decode_child_result(
    child: &mut Child,
    status: ExitStatus,
    stdout_reader: thread::JoinHandle<Result<Vec<u8>>>,
    stderr_reader: thread::JoinHandle<Result<Vec<u8>>>,
    timed_out: bool,
    cancelled_by_caller: bool,
    terminal_error: Option<anyhow::Error>,
) -> Result<Value> {
    let _ = child.wait();
    let stdout = join_reader(stdout_reader, "stdout")?;
    let stderr = join_reader(stderr_reader, "stderr")?;
    if let Some(error) = terminal_error {
        return Err(error);
    }
    if timed_out {
        bail!(
            "Computer Use helper timed out after {} seconds",
            HELPER_TIMEOUT.as_secs()
        );
    }
    if cancelled_by_caller && !status.success() {
        bail!("Computer Use action was cancelled");
    }
    if !status.success() {
        let stderr = String::from_utf8_lossy(stderr.as_slice())
            .trim()
            .to_string();
        if stderr.is_empty() {
            bail!("Computer Use helper failed with status {status}");
        }
        bail!("Computer Use helper failed: {stderr}");
    }
    let response: HelperResponse = decode_frame(stdout.as_slice(), MAX_RESPONSE_BYTES)
        .context("decode Computer Use helper response")?;
    if response.protocol_version != HELPER_PROTOCOL_VERSION {
        bail!(
            "Computer Use helper protocol mismatch: expected {}, received {}",
            HELPER_PROTOCOL_VERSION,
            response.protocol_version
        );
    }
    if response.success {
        response
            .result
            .ok_or_else(|| anyhow!("Computer Use helper returned no result"))
    } else {
        Err(anyhow!(
            "{}",
            response
                .error
                .unwrap_or_else(|| "Computer Use helper failed without an error".to_string())
        ))
    }
}

fn helper_path() -> Result<PathBuf> {
    if let Some(configured) = std::env::var_os(HELPER_PATH_ENV) {
        let path = PathBuf::from(configured);
        if !path.is_absolute() {
            bail!("{HELPER_PATH_ENV} must be an absolute path");
        }
        return Ok(path);
    }
    let current_executable =
        std::env::current_exe().context("resolve Local Connector Core path")?;
    let parent = current_executable
        .parent()
        .ok_or_else(|| anyhow!("Local Connector Core executable directory is unavailable"))?;
    Ok(parent.join(HELPER_EXECUTABLE_NAME))
}

fn validate_helper_binary(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("Computer Use helper is missing: {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("Computer Use helper must be a regular non-symlink file");
    }
    use std::os::unix::fs::PermissionsExt;
    if metadata.permissions().mode() & 0o111 == 0 {
        bail!("Computer Use helper is not executable");
    }
    Ok(())
}

fn validate_helper_signature(path: &Path) -> Result<()> {
    if !helper_signature_required() {
        return Ok(());
    }
    if !Path::new(MACOS_CODESIGN_PATH).is_file() {
        bail!("macOS codesign verification runtime is missing: {MACOS_CODESIGN_PATH}");
    }
    verify_codesign(path)?;
    let current_executable =
        std::env::current_exe().context("resolve Local Connector Core path")?;
    verify_codesign(current_executable.as_path())?;
    let helper_team = codesign_team_identifier(path)?;
    let core_team = codesign_team_identifier(current_executable.as_path())?;
    if helper_team != core_team {
        bail!("Computer Use helper signing team does not match Local Connector Core");
    }
    Ok(())
}

fn helper_signature_required() -> bool {
    if env_flag(HELPER_REQUIRE_SIGNED_ENV) {
        return true;
    }
    !cfg!(debug_assertions)
}

fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "TRUE" | "yes" | "YES"))
}

fn verify_codesign(path: &Path) -> Result<()> {
    let output = Command::new(MACOS_CODESIGN_PATH)
        .args(["--verify", "--strict", "--verbose=2"])
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("verify code signature for {}", path.display()))?;
    ensure_codesign_output_bounded(&output.stdout, &output.stderr)?;
    if output.status.success() {
        Ok(())
    } else {
        bail!("Computer Use helper code signature verification failed")
    }
}

fn codesign_team_identifier(path: &Path) -> Result<String> {
    let output = Command::new(MACOS_CODESIGN_PATH)
        .args(["-d", "--verbose=4"])
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("read code signature identity for {}", path.display()))?;
    ensure_codesign_output_bounded(&output.stdout, &output.stderr)?;
    if !output.status.success() {
        bail!("Computer Use helper code signature identity is unavailable");
    }
    let details = String::from_utf8_lossy(output.stderr.as_slice());
    let team = details
        .lines()
        .find_map(|line| line.trim().strip_prefix("TeamIdentifier="))
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "not set")
        .ok_or_else(|| anyhow!("Computer Use helper requires a Developer ID team signature"))?;
    if team.len() > 256
        || !team
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        bail!("Computer Use helper signing team identifier is invalid");
    }
    Ok(team.to_string())
}

fn ensure_codesign_output_bounded(stdout: &[u8], stderr: &[u8]) -> Result<()> {
    if stdout.len() > MAX_STDERR_BYTES || stderr.len() > MAX_STDERR_BYTES {
        bail!("macOS codesign output exceeded the safety limit");
    }
    Ok(())
}

fn create_cancellation_marker(path: &Path) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
    {
        Ok(mut file) => file
            .write_all(b"cancel\n")
            .context("write Computer Use cancellation marker"),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error).context("create Computer Use cancellation marker"),
    }
}

fn validate_private_cancellation_directory(path: &Path) -> Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("read cancellation directory {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!("Computer Use cancellation directory must be a non-symlink directory");
    }
    // SAFETY: geteuid has no preconditions and only returns the current process identity.
    let effective_uid = unsafe { libc::geteuid() };
    if metadata.uid() != effective_uid || metadata.permissions().mode() & 0o077 != 0 {
        bail!("Computer Use cancellation directory must be private to the current user");
    }
    Ok(())
}

fn validate_cancellation_marker(path: &Path) -> Result<()> {
    if !path.is_absolute() || path.file_name().and_then(|value| value.to_str()) != Some("cancel") {
        bail!("Computer Use cancellation marker path is invalid");
    }
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("Computer Use cancellation marker parent is unavailable"))?;
    validate_private_cancellation_directory(parent)?;
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            bail!("Computer Use cancellation marker must be a regular non-symlink file")
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).context("read Computer Use cancellation marker"),
    }
}

struct CancellationWatcher {
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Drop for CancellationWatcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn cancellation_watcher(path: PathBuf) -> Result<(Arc<AtomicBool>, CancellationWatcher)> {
    validate_cancellation_marker(path.as_path())?;
    let cancelled = Arc::new(AtomicBool::new(path.exists()));
    let stop = Arc::new(AtomicBool::new(false));
    let watcher_cancelled = cancelled.clone();
    let watcher_stop = stop.clone();
    let watcher = thread::spawn(move || {
        while !watcher_stop.load(Ordering::SeqCst) {
            match fs::symlink_metadata(path.as_path()) {
                Ok(_) => {
                    watcher_cancelled.store(true, Ordering::SeqCst);
                    return;
                }
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(_) => {
                    watcher_cancelled.store(true, Ordering::SeqCst);
                    return;
                }
            }
            thread::sleep(HELPER_POLL_INTERVAL);
        }
    });
    Ok((
        cancelled,
        CancellationWatcher {
            stop,
            thread: Some(watcher),
        },
    ))
}

fn validate_operation(operation: &str) -> Result<()> {
    if operation.is_empty()
        || operation.len() > MAX_OPERATION_BYTES
        || operation.chars().any(char::is_control)
    {
        bail!("Computer Use helper operation is invalid");
    }
    Ok(())
}

fn validate_approved_arguments(arguments: Option<&[String]>) -> Result<()> {
    let Some(arguments) = arguments else {
        return Ok(());
    };
    if arguments.len() > MAX_APPROVED_ARGUMENTS {
        bail!("Computer Use approved arguments exceeded the item limit");
    }
    let mut total = 0usize;
    for argument in arguments {
        if argument.len() > MAX_APPROVED_ARGUMENT_BYTES || argument.chars().any(char::is_control) {
            bail!("Computer Use approved argument is invalid");
        }
        total = total.saturating_add(argument.len());
    }
    if total > MAX_APPROVED_ARGUMENTS_BYTES {
        bail!("Computer Use approved arguments exceeded the byte limit");
    }
    Ok(())
}

pub(super) fn run() -> Result<()> {
    let mut arguments = std::env::args_os();
    let _executable = arguments.next();
    if arguments.next().as_deref() != Some(std::ffi::OsStr::new(HELPER_PROTOCOL_ARGUMENT))
        || arguments.next().is_some()
    {
        bail!("Computer Use helper requires the versioned stdio protocol argument");
    }
    validate_parent_caller()?;
    let request_bytes = read_frame(&mut std::io::stdin().lock(), MAX_REQUEST_BYTES)
        .context("read Computer Use helper request")?;
    let request = serde_json::from_slice::<HelperRequest>(request_bytes.as_slice())
        .context("decode Computer Use helper request")?;
    let response = match dispatch(request) {
        Ok(result) => HelperResponse::success(result),
        Err(error) => HelperResponse::error(error.to_string()),
    };
    let response_bytes =
        serde_json::to_vec(&response).context("encode Computer Use helper response")?;
    write_frame(
        &mut std::io::stdout().lock(),
        response_bytes.as_slice(),
        MAX_RESPONSE_BYTES,
    )
    .context("write Computer Use helper response")
}

fn validate_parent_caller() -> Result<()> {
    let parent_path = parent_executable_path()?;
    validate_helper_binary(parent_path.as_path())
        .context("validate Computer Use helper parent executable")?;
    if parent_path.file_name().and_then(|value| value.to_str()) != Some(CORE_EXECUTABLE_NAME) {
        bail!("Computer Use helper must be launched directly by Local Connector Core");
    }
    if !helper_signature_required() {
        return Ok(());
    }
    let helper_path =
        std::env::current_exe().context("resolve running Computer Use helper path")?;
    verify_codesign(helper_path.as_path())?;
    verify_codesign(parent_path.as_path())?;
    if codesign_team_identifier(helper_path.as_path())?
        != codesign_team_identifier(parent_path.as_path())?
    {
        bail!("Computer Use helper parent signing team does not match the helper");
    }
    Ok(())
}

fn parent_executable_path() -> Result<PathBuf> {
    use std::os::unix::ffi::OsStringExt;

    // SAFETY: getppid has no preconditions and returns the direct parent process identifier.
    let parent_pid = unsafe { libc::getppid() };
    if parent_pid <= 1 {
        bail!("Computer Use helper parent process is unavailable");
    }
    let mut buffer = vec![0_u8; PROC_PIDPATHINFO_MAXSIZE];
    // SAFETY: proc_pidpath receives a writable buffer with the exact length supplied here and
    // writes at most that many bytes for the live direct parent process.
    let written = unsafe {
        proc_pidpath(
            parent_pid,
            buffer.as_mut_ptr().cast::<c_void>(),
            buffer.len() as u32,
        )
    };
    if written <= 0 {
        return Err(std::io::Error::last_os_error())
            .context("resolve Computer Use helper parent executable");
    }
    buffer.truncate(written as usize);
    if buffer.last() == Some(&0) {
        buffer.pop();
    }
    if buffer.is_empty() || buffer.contains(&0) {
        bail!("Computer Use helper parent executable path is invalid");
    }
    let path = PathBuf::from(std::ffi::OsString::from_vec(buffer));
    if !path.is_absolute() {
        bail!("Computer Use helper parent executable path must be absolute");
    }
    Ok(path)
}

fn dispatch(request: HelperRequest) -> Result<Value> {
    if request.protocol_version != HELPER_PROTOCOL_VERSION {
        bail!(
            "Computer Use helper protocol mismatch: expected {}, received {}",
            HELPER_PROTOCOL_VERSION,
            request.protocol_version
        );
    }
    match request.command {
        HelperCommand::ProtocolProbe => Ok(json!({
            "protocol_version": HELPER_PROTOCOL_VERSION,
            "transport": "single_process_length_prefixed_stdio",
            "network_listener": false,
        })),
        HelperCommand::FrontmostWindowControlTarget => {
            serde_json::to_value(macos_frontmost_window_control_target_local()?)
                .context("encode helper frontmost window control target")
        }
        HelperCommand::WindowLayoutPreflight { snapshot } => {
            preflight_window_layout_snapshot_local(&snapshot)
        }
        HelperCommand::DependencyProbe {
            screen_capture_only,
        } => Ok(json!({
            "dependency_error": if screen_capture_only {
                screen_capture_dependency_error_local()
            } else {
                dependency_error_local()
            },
        })),
        HelperCommand::Execute {
            operation,
            arguments,
        } => {
            validate_operation(operation.as_str())?;
            execute_local(operation.as_str(), &arguments)
        }
        HelperCommand::ExecuteApproved {
            operation,
            arguments,
            approved_command_args,
            cancellation_marker,
        } => {
            validate_operation(operation.as_str())?;
            if !CONTROL_OPERATIONS.contains(&operation.as_str()) {
                bail!("Computer Use operation does not support approved control: {operation}");
            }
            validate_approved_arguments(approved_command_args.as_deref())?;
            let (cancelled, _watcher) = cancellation_watcher(cancellation_marker)?;
            execute_approved_local(
                operation.as_str(),
                &arguments,
                approved_command_args.as_deref(),
                Some(cancelled.as_ref()),
            )
        }
    }
}

fn write_frame<W: Write>(writer: &mut W, payload: &[u8], limit: usize) -> Result<()> {
    if payload.len() > limit || payload.len() > u32::MAX as usize {
        bail!("Computer Use helper frame exceeded {limit} bytes");
    }
    writer
        .write_all(&(payload.len() as u32).to_le_bytes())
        .context("write Computer Use helper frame length")?;
    writer
        .write_all(payload)
        .context("write Computer Use helper frame payload")?;
    writer.flush().context("flush Computer Use helper frame")
}

fn read_frame<R: Read>(reader: &mut R, limit: usize) -> Result<Vec<u8>> {
    let mut length = [0_u8; 4];
    reader
        .read_exact(&mut length)
        .context("read Computer Use helper frame length")?;
    let length = u32::from_le_bytes(length) as usize;
    if length > limit {
        bail!("Computer Use helper frame exceeded {limit} bytes");
    }
    let mut payload = vec![0_u8; length];
    reader
        .read_exact(payload.as_mut_slice())
        .context("read Computer Use helper frame payload")?;
    let mut trailing = [0_u8; 1];
    if reader
        .read(&mut trailing)
        .context("read Computer Use helper trailing data")?
        != 0
    {
        bail!("Computer Use helper received trailing protocol data");
    }
    Ok(payload)
}

fn decode_frame(bytes: &[u8], limit: usize) -> Result<HelperResponse> {
    let mut cursor = std::io::Cursor::new(bytes);
    let payload = read_frame(&mut cursor, limit)?;
    serde_json::from_slice(payload.as_slice()).context("decode Computer Use helper JSON frame")
}

fn read_limited_stream<R: Read>(mut reader: R, label: &str, limit: usize) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .with_context(|| format!("read Computer Use helper {label}"))?;
        if read == 0 {
            return Ok(output);
        }
        if output.len().saturating_add(read) > limit {
            bail!("Computer Use helper {label} exceeded {limit} bytes");
        }
        output.extend_from_slice(&buffer[..read]);
    }
}

fn join_reader(handle: thread::JoinHandle<Result<Vec<u8>>>, label: &str) -> Result<Vec<u8>> {
    handle
        .join()
        .map_err(|_| anyhow!("Computer Use helper {label} reader panicked"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_frame_is_versioned_bounded_and_rejects_trailing_data() {
        let request = HelperRequest {
            protocol_version: HELPER_PROTOCOL_VERSION,
            command: HelperCommand::ProtocolProbe,
        };
        let payload = serde_json::to_vec(&request).unwrap();
        let mut frame = Vec::new();
        write_frame(&mut frame, payload.as_slice(), MAX_REQUEST_BYTES).unwrap();
        let decoded = read_frame(
            &mut std::io::Cursor::new(frame.as_slice()),
            MAX_REQUEST_BYTES,
        )
        .unwrap();
        assert_eq!(decoded, payload);

        frame.push(0);
        assert!(read_frame(&mut std::io::Cursor::new(frame), MAX_REQUEST_BYTES).is_err());
        assert!(write_frame(&mut Vec::new(), &[0; 17], 16).is_err());
    }

    #[test]
    fn request_contract_rejects_unknown_fields_and_protocol_mismatch() {
        let unknown = br#"{"protocol_version":1,"kind":"protocol_probe","extra":true}"#;
        assert!(serde_json::from_slice::<HelperRequest>(unknown).is_err());
        let request = HelperRequest {
            protocol_version: HELPER_PROTOCOL_VERSION + 1,
            command: HelperCommand::ProtocolProbe,
        };
        assert!(dispatch(request).is_err());
    }

    #[test]
    fn cancellation_directory_and_approved_arguments_fail_closed() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
        validate_private_cancellation_directory(directory.path()).unwrap();
        let marker = directory.path().join("cancel");
        validate_cancellation_marker(marker.as_path()).unwrap();
        create_cancellation_marker(marker.as_path()).unwrap();
        validate_cancellation_marker(marker.as_path()).unwrap();

        let oversized = vec!["x".repeat(MAX_APPROVED_ARGUMENT_BYTES + 1)];
        assert!(validate_approved_arguments(Some(oversized.as_slice())).is_err());
        assert!(validate_operation("").is_err());

        let watcher_directory = tempfile::tempdir().unwrap();
        fs::set_permissions(watcher_directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let watcher_marker = watcher_directory.path().join("cancel");
        let (cancelled, watcher) = cancellation_watcher(watcher_marker.clone()).unwrap();
        assert!(!cancelled.load(Ordering::SeqCst));
        create_cancellation_marker(watcher_marker.as_path()).unwrap();
        let started = Instant::now();
        while !cancelled.load(Ordering::SeqCst) && started.elapsed() < Duration::from_secs(1) {
            thread::sleep(HELPER_POLL_INTERVAL);
        }
        assert!(cancelled.load(Ordering::SeqCst));
        drop(watcher);
    }

    #[test]
    fn parent_process_path_resolution_is_absolute_and_bounded() {
        let parent = parent_executable_path().unwrap();
        assert!(parent.is_absolute());
        assert!(parent.as_os_str().len() <= PROC_PIDPATHINFO_MAXSIZE);
        assert!(parent.is_file());
    }
}
