mod diagnostics;
mod process;
mod transport;

use chatos_local_agent_protocol::LOCAL_AGENT_PROTOCOL_VERSION;
use diagnostics::{classify_error, HostDiagnostics, HostDiagnosticsSnapshot};
use process::spawn;
use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex;
use transport::{health_check, terminate, ManagedHost};

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

#[derive(Default)]
pub struct HostManager {
    process: Mutex<Option<ManagedHost>>,
    diagnostics: HostDiagnostics,
}

impl HostManager {
    async fn terminate_process(&self) {
        let mut process = self.process.lock().await;
        if let Some(host) = process.as_mut() {
            terminate(host).await;
        }
        *process = None;
    }

    pub async fn shutdown(&self) {
        self.diagnostics.record("host_shutdown", "requested").await;
        self.terminate_process().await;
        self.diagnostics.record("host_shutdown", "stopped").await;
    }
}

#[tauri::command]
pub async fn host_diagnostics(
    manager: State<'_, HostManager>,
) -> Result<HostDiagnosticsSnapshot, String> {
    Ok(manager.diagnostics.snapshot().await)
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
        manager
            .diagnostics
            .record("host_process_exit", "unexpected")
            .await;
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
                .diagnostics
                .record("host_health_check", classify_error(&error))
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
        manager
            .diagnostics
            .record("host_start", "invalid_owner")
            .await;
        return Err(error);
    }
    manager.diagnostics.record("host_start", "requested").await;
    let mut process = manager.process.lock().await;
    if let Some(host) = process.as_mut() {
        if host.owner_user_id == owner_user_id && health_check(host).await.is_ok() {
            manager
                .diagnostics
                .record("host_start", "already_ready")
                .await;
            return Ok(HostLifecycleSnapshot::ready());
        }
        terminate(host).await;
        *process = None;
        manager
            .diagnostics
            .record("host_replace", "previous_stopped")
            .await;
    }

    let executable = match host_executable(&app) {
        Ok(executable) => executable,
        Err(error) => {
            manager
                .diagnostics
                .record("host_executable", "unavailable")
                .await;
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
    let mut host = match spawn(&executable, &database, owner_user_id).await {
        Ok(host) => host,
        Err(error) => {
            manager
                .diagnostics
                .record("host_process_spawn", "failed")
                .await;
            return Err(error.to_string());
        }
    };
    if let Err(error) = health_check(&mut host).await {
        terminate(&mut host).await;
        manager
            .diagnostics
            .record("host_start_handshake", classify_error(&error))
            .await;
        return Err(error);
    }
    *process = Some(host);
    manager
        .diagnostics
        .record("host_start_handshake", "ready")
        .await;
    Ok(HostLifecycleSnapshot::ready())
}

#[tauri::command]
pub async fn stop_local_agent_host(
    manager: State<'_, HostManager>,
) -> Result<HostLifecycleSnapshot, String> {
    manager.diagnostics.record("host_stop", "requested").await;
    manager.terminate_process().await;
    manager.diagnostics.record("host_stop", "stopped").await;
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
