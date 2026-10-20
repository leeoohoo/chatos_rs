use chatos_local_agent_protocol::{
    HostCommand, HostRequestEnvelope, HostResponseEnvelope, HostResult,
    LOCAL_AGENT_MAX_FRAME_BYTES, LOCAL_AGENT_MAX_MESSAGE_BYTES, LOCAL_AGENT_PROTOCOL_VERSION,
};
use serde::Serialize;
use std::{
    collections::VecDeque,
    path::PathBuf,
    process::Stdio,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager, State};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex,
    time::timeout,
};

const HEALTH_TIMEOUT: Duration = Duration::from_secs(5);
const FRAME_CONTINUATION_BIT: u32 = 1 << 31;
const FRAME_LENGTH_MASK: u32 = !FRAME_CONTINUATION_BIT;
const DIAGNOSTIC_CAPACITY: usize = 64;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostLifecycleSnapshot {
    phase: &'static str,
    ready: bool,
    protocol_version: u32,
    detail: String,
    retryable: bool,
}

impl HostLifecycleSnapshot {
    fn stopped(detail: impl Into<String>, retryable: bool) -> Self {
        Self {
            phase: "stopped",
            ready: false,
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            detail: detail.into(),
            retryable,
        }
    }

    fn ready() -> Self {
        Self {
            phase: "ready",
            ready: true,
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            detail: "Local Agent Host 已完成协议握手。".to_string(),
            retryable: false,
        }
    }
}

struct ManagedHost {
    child: Child,
    input: ChildStdin,
    output: ChildStdout,
    owner_user_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostDiagnosticEntry {
    sequence: u64,
    timestamp_unix_ms: u64,
    event: &'static str,
    outcome: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostDiagnosticsSnapshot {
    capacity: usize,
    entries: Vec<HostDiagnosticEntry>,
}

#[derive(Default)]
struct HostDiagnosticsState {
    next_sequence: u64,
    entries: VecDeque<HostDiagnosticEntry>,
}

#[derive(Default)]
pub struct HostManager {
    process: Mutex<Option<ManagedHost>>,
    diagnostics: Mutex<HostDiagnosticsState>,
}

impl HostManager {
    async fn record(&self, event: &'static str, outcome: &'static str) {
        let mut diagnostics = self.diagnostics.lock().await;
        diagnostics.next_sequence = diagnostics.next_sequence.saturating_add(1);
        let sequence = diagnostics.next_sequence;
        if diagnostics.entries.len() == DIAGNOSTIC_CAPACITY {
            diagnostics.entries.pop_front();
        }
        diagnostics.entries.push_back(HostDiagnosticEntry {
            sequence,
            timestamp_unix_ms: unix_timestamp_milliseconds(),
            event,
            outcome,
        });
    }
}

#[tauri::command]
pub async fn host_diagnostics(
    manager: State<'_, HostManager>,
) -> Result<HostDiagnosticsSnapshot, String> {
    let diagnostics = manager.diagnostics.lock().await;
    Ok(HostDiagnosticsSnapshot {
        capacity: DIAGNOSTIC_CAPACITY,
        entries: diagnostics.entries.iter().cloned().collect(),
    })
}

#[tauri::command]
pub async fn host_lifecycle_snapshot(
    manager: State<'_, HostManager>,
) -> Result<HostLifecycleSnapshot, String> {
    let mut process = manager.process.lock().await;
    let Some(host) = process.as_mut() else {
        return Ok(HostLifecycleSnapshot::stopped(
            "登录账号确定后才能启动本地服务。",
            false,
        ));
    };
    if host
        .child
        .try_wait()
        .map_err(|error| error.to_string())?
        .is_some()
    {
        *process = None;
        manager.record("host_process_exit", "unexpected").await;
        return Ok(HostLifecycleSnapshot::stopped(
            "Local Agent Host 已退出，可以重新启动。",
            true,
        ));
    }
    match health_check(host).await {
        Ok(()) => Ok(HostLifecycleSnapshot::ready()),
        Err(error) => {
            terminate(host).await;
            *process = None;
            manager
                .record("host_health_check", classify_diagnostic_error(&error))
                .await;
            Ok(HostLifecycleSnapshot {
                phase: "degraded",
                ready: false,
                protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
                detail: error,
                retryable: true,
            })
        }
    }
}

#[tauri::command]
pub async fn start_local_agent_host(
    app: AppHandle,
    manager: State<'_, HostManager>,
    owner_user_id: String,
) -> Result<HostLifecycleSnapshot, String> {
    if let Err(error) = validate_owner(&owner_user_id) {
        manager.record("host_start", "invalid_owner").await;
        return Err(error);
    }
    manager.record("host_start", "requested").await;
    let mut process = manager.process.lock().await;
    if let Some(host) = process.as_mut() {
        if host.owner_user_id == owner_user_id && health_check(host).await.is_ok() {
            manager.record("host_start", "already_ready").await;
            return Ok(HostLifecycleSnapshot::ready());
        }
        terminate(host).await;
        *process = None;
        manager.record("host_replace", "previous_stopped").await;
    }

    let executable = match host_executable(&app) {
        Ok(executable) => executable,
        Err(error) => {
            manager.record("host_executable", "unavailable").await;
            return Err(error);
        }
    };
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;
    let database = data_dir.join("local-agent").join("agent.sqlite3");
    if let Some(parent) = database.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut command = Command::new(executable);
    command
        .arg("--database")
        .arg(database)
        .arg("--owner-user-id")
        .arg(&owner_user_id)
        .arg("--stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            manager.record("host_process_spawn", "failed").await;
            return Err(error.to_string());
        }
    };
    let input = match child.stdin.take() {
        Some(input) => input,
        None => {
            manager.record("host_transport", "stdin_unavailable").await;
            return Err("Host stdin is unavailable".to_string());
        }
    };
    let output = match child.stdout.take() {
        Some(output) => output,
        None => {
            manager.record("host_transport", "stdout_unavailable").await;
            return Err("Host stdout is unavailable".to_string());
        }
    };
    let mut host = ManagedHost {
        child,
        input,
        output,
        owner_user_id,
    };
    if let Err(error) = health_check(&mut host).await {
        terminate(&mut host).await;
        manager
            .record("host_start_handshake", classify_diagnostic_error(&error))
            .await;
        return Err(error);
    }
    *process = Some(host);
    manager.record("host_start_handshake", "ready").await;
    Ok(HostLifecycleSnapshot::ready())
}

#[tauri::command]
pub async fn stop_local_agent_host(
    manager: State<'_, HostManager>,
) -> Result<HostLifecycleSnapshot, String> {
    manager.record("host_stop", "requested").await;
    let mut process = manager.process.lock().await;
    if let Some(host) = process.as_mut() {
        terminate(host).await;
    }
    *process = None;
    manager.record("host_stop", "stopped").await;
    Ok(HostLifecycleSnapshot::stopped(
        "Local Agent Host 已停止。",
        false,
    ))
}

fn unix_timestamp_milliseconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn classify_diagnostic_error(error: &str) -> &'static str {
    if error.contains("timed out") {
        "timeout"
    } else if error.contains("response frame") || error.contains("early eof") {
        "invalid_transport"
    } else if error.contains("protocol v44") || error.contains("protocol_version") {
        "protocol_mismatch"
    } else {
        "failed"
    }
}

fn validate_owner(owner: &str) -> Result<(), String> {
    let trimmed = owner.trim();
    if trimmed.is_empty() || trimmed.len() > 512 || trimmed.chars().any(char::is_control) {
        return Err("ownerUserId is invalid".to_string());
    }
    Ok(())
}

fn host_executable(app: &AppHandle) -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("CHATOS_LOCAL_AGENT_HOST_PATH") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
    }
    let filename = if cfg!(windows) {
        "chatos_local_agent_host.exe"
    } else {
        "chatos_local_agent_host"
    };
    let bundled = app
        .path()
        .resource_dir()
        .map_err(|error| error.to_string())?
        .join(filename);
    if bundled.is_file() {
        return Ok(bundled);
    }
    if let Ok(current_executable) = std::env::current_exe() {
        if let Some(directory) = current_executable.parent() {
            let development = directory.join(filename);
            if development.is_file() {
                return Ok(development);
            }
        }
    }
    Err("Local Agent Host executable is unavailable.".to_string())
}

async fn health_check(host: &mut ManagedHost) -> Result<(), String> {
    timeout(HEALTH_TIMEOUT, async {
        let request = HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: format!("windows-health-{}", std::process::id()),
            command: HostCommand::Health,
        };
        let body = serde_json::to_vec(&request).map_err(|error| error.to_string())?;
        write_message(&mut host.input, &body).await?;
        let response: HostResponseEnvelope =
            serde_json::from_slice(&read_message(&mut host.output).await?)
                .map_err(|error| error.to_string())?;
        if response.protocol_version != LOCAL_AGENT_PROTOCOL_VERSION
            || response.command_id != request.command_id
            || !response.ok
        {
            return Err(response
                .error
                .map(|error| error.message)
                .unwrap_or_else(|| "Host health response is invalid.".to_string()));
        }
        match response.result {
            Some(HostResult::Health {
                service,
                storage_ready: true,
                max_frame_bytes: LOCAL_AGENT_MAX_FRAME_BYTES,
                max_message_bytes: LOCAL_AGENT_MAX_MESSAGE_BYTES,
                ..
            }) if service == "chatos-local-agent-host" => Ok(()),
            _ => Err("Host health response does not match protocol v44.".to_string()),
        }
    })
    .await
    .map_err(|_| "Host health check timed out.".to_string())?
}

async fn write_message<W>(writer: &mut W, body: &[u8]) -> Result<(), String>
where
    W: AsyncWrite + Unpin,
{
    if body.is_empty() || body.len() > LOCAL_AGENT_MAX_MESSAGE_BYTES {
        return Err("Host request message is invalid.".to_string());
    }
    let mut chunks = body.chunks(LOCAL_AGENT_MAX_FRAME_BYTES).peekable();
    while let Some(chunk) = chunks.next() {
        let length = u32::try_from(chunk.len())
            .map_err(|_| "Host request frame is too large.".to_string())?;
        let prefix = if chunks.peek().is_some() {
            length | FRAME_CONTINUATION_BIT
        } else {
            length
        };
        writer
            .write_all(&prefix.to_be_bytes())
            .await
            .map_err(|error| error.to_string())?;
        writer
            .write_all(chunk)
            .await
            .map_err(|error| error.to_string())?;
    }
    writer.flush().await.map_err(|error| error.to_string())
}

async fn read_message<R>(reader: &mut R) -> Result<Vec<u8>, String>
where
    R: AsyncRead + Unpin,
{
    let mut message = Vec::new();
    loop {
        let mut prefix = [0_u8; 4];
        reader
            .read_exact(&mut prefix)
            .await
            .map_err(|error| error.to_string())?;
        let encoded = u32::from_be_bytes(prefix);
        let has_more = encoded & FRAME_CONTINUATION_BIT != 0;
        let length = (encoded & FRAME_LENGTH_MASK) as usize;
        if length == 0
            || length > LOCAL_AGENT_MAX_FRAME_BYTES
            || message.len().saturating_add(length) > LOCAL_AGENT_MAX_MESSAGE_BYTES
        {
            return Err("Host response frame is invalid.".to_string());
        }
        let start = message.len();
        message.resize(start + length, 0);
        reader
            .read_exact(&mut message[start..])
            .await
            .map_err(|error| error.to_string())?;
        if !has_more {
            return Ok(message);
        }
    }
}

async fn terminate(host: &mut ManagedHost) {
    let _ = host.input.shutdown().await;
    if timeout(Duration::from_secs(2), host.child.wait())
        .await
        .is_err()
    {
        let _ = host.child.kill().await;
        let _ = host.child.wait().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{duplex, AsyncWriteExt};

    #[test]
    fn owner_validation_rejects_unscoped_or_control_values() {
        assert!(validate_owner("").is_err());
        assert!(validate_owner("user\nother").is_err());
        assert!(validate_owner("user-1").is_ok());
    }

    #[tokio::test]
    async fn message_codec_round_trips_continuation_frames() {
        let payload = vec![0x5a; LOCAL_AGENT_MAX_FRAME_BYTES + 257];
        let (mut writer, mut reader) = duplex(payload.len() + 16);

        write_message(&mut writer, &payload).await.expect("write");
        let received = read_message(&mut reader).await.expect("read");

        assert_eq!(received, payload);
    }

    #[tokio::test]
    async fn message_codec_rejects_truncated_continuation() {
        let (mut writer, mut reader) = duplex(16);
        writer
            .write_all(&(1_u32 | FRAME_CONTINUATION_BIT).to_be_bytes())
            .await
            .expect("prefix");
        writer.write_all(&[0x5a]).await.expect("body");
        writer.shutdown().await.expect("shutdown");

        assert!(read_message(&mut reader).await.is_err());
    }

    #[tokio::test]
    async fn diagnostics_are_bounded_and_monotonic() {
        let manager = HostManager::default();
        for _ in 0..(DIAGNOSTIC_CAPACITY + 3) {
            manager.record("test", "ok").await;
        }

        let diagnostics = manager.diagnostics.lock().await;
        assert_eq!(diagnostics.entries.len(), DIAGNOSTIC_CAPACITY);
        assert_eq!(
            diagnostics.entries.front().map(|entry| entry.sequence),
            Some(4)
        );
        assert_eq!(
            diagnostics.entries.back().map(|entry| entry.sequence),
            Some((DIAGNOSTIC_CAPACITY + 3) as u64),
        );
    }

    #[test]
    fn diagnostic_errors_are_reduced_to_safe_categories() {
        assert_eq!(
            classify_diagnostic_error("Host health check timed out."),
            "timeout"
        );
        assert_eq!(classify_diagnostic_error("token=secret-value"), "failed");
    }
}
