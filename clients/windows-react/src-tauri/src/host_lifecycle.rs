use chatos_local_agent_protocol::{
    HostCommand, HostRequestEnvelope, HostResponseEnvelope, HostResult,
    LOCAL_AGENT_MAX_FRAME_BYTES, LOCAL_AGENT_MAX_MESSAGE_BYTES, LOCAL_AGENT_PROTOCOL_VERSION,
};
use serde::Serialize;
use std::{path::PathBuf, process::Stdio, time::Duration};
use tauri::{AppHandle, Manager, State};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex,
    time::timeout,
};

const HEALTH_TIMEOUT: Duration = Duration::from_secs(5);

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

#[derive(Default)]
pub struct HostManager {
    process: Mutex<Option<ManagedHost>>,
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
    validate_owner(&owner_user_id)?;
    let mut process = manager.process.lock().await;
    if let Some(host) = process.as_mut() {
        if host.owner_user_id == owner_user_id && health_check(host).await.is_ok() {
            return Ok(HostLifecycleSnapshot::ready());
        }
        terminate(host).await;
        *process = None;
    }

    let executable = host_executable(&app)?;
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
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let input = child.stdin.take().ok_or("Host stdin is unavailable")?;
    let output = child.stdout.take().ok_or("Host stdout is unavailable")?;
    let mut host = ManagedHost {
        child,
        input,
        output,
        owner_user_id,
    };
    if let Err(error) = health_check(&mut host).await {
        terminate(&mut host).await;
        return Err(error);
    }
    *process = Some(host);
    Ok(HostLifecycleSnapshot::ready())
}

#[tauri::command]
pub async fn stop_local_agent_host(
    manager: State<'_, HostManager>,
) -> Result<HostLifecycleSnapshot, String> {
    let mut process = manager.process.lock().await;
    if let Some(host) = process.as_mut() {
        terminate(host).await;
    }
    *process = None;
    Ok(HostLifecycleSnapshot::stopped(
        "Local Agent Host 已停止。",
        false,
    ))
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
        write_frame(&mut host.input, &body).await?;
        let response: HostResponseEnvelope =
            serde_json::from_slice(&read_frame(&mut host.output).await?)
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

async fn write_frame(writer: &mut ChildStdin, body: &[u8]) -> Result<(), String> {
    if body.is_empty() || body.len() > LOCAL_AGENT_MAX_FRAME_BYTES {
        return Err("Host request frame is invalid.".to_string());
    }
    let length = u32::try_from(body.len()).map_err(|_| "Host request is too large".to_string())?;
    writer
        .write_all(&length.to_be_bytes())
        .await
        .map_err(|error| error.to_string())?;
    writer
        .write_all(body)
        .await
        .map_err(|error| error.to_string())?;
    writer.flush().await.map_err(|error| error.to_string())
}

async fn read_frame(reader: &mut ChildStdout) -> Result<Vec<u8>, String> {
    let mut prefix = [0_u8; 4];
    reader
        .read_exact(&mut prefix)
        .await
        .map_err(|error| error.to_string())?;
    let length = u32::from_be_bytes(prefix) as usize;
    if length == 0 || length > LOCAL_AGENT_MAX_FRAME_BYTES {
        return Err("Host response frame is invalid.".to_string());
    }
    let mut body = vec![0; length];
    reader
        .read_exact(&mut body)
        .await
        .map_err(|error| error.to_string())?;
    Ok(body)
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

    #[test]
    fn owner_validation_rejects_unscoped_or_control_values() {
        assert!(validate_owner("").is_err());
        assert!(validate_owner("user\nother").is_err());
        assert!(validate_owner("user-1").is_ok());
    }
}
