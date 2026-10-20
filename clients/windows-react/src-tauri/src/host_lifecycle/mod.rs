mod diagnostics;
mod process;
pub(crate) mod transport;

use crate::{
    attachment_tool_worker::AttachmentToolWorker, authentication::AuthenticationManager,
    connector_state, control_plane, terminal_tool_worker::TerminalToolWorker,
    tool_approval::session::ApprovalSessionAllowlist,
};
use chatos_local_agent_protocol::{HostCommand, HostResult, LOCAL_AGENT_PROTOCOL_VERSION};
use diagnostics::{classify_error, HostDiagnostics, HostDiagnosticsSnapshot};
use process::spawn;
use serde::Serialize;
use std::{collections::BTreeMap, path::PathBuf};
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex;
use transport::{health_check, request, terminate, ManagedHost};

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
    model_catalog: Mutex<Option<(String, Vec<control_plane::ControlPlaneModelOption>)>>,
    diagnostics: HostDiagnostics,
    approval_sessions: ApprovalSessionAllowlist,
}

#[derive(Debug)]
pub(crate) struct HostExecutionError {
    pub(crate) code: Option<String>,
    pub(crate) message: String,
}

impl HostManager {
    async fn terminate_process(&self) {
        self.approval_sessions.reset();
        let mut process = self.process.lock().await;
        if let Some(host) = process.as_mut() {
            terminate(host).await;
        }
        *process = None;
        *self.model_catalog.lock().await = None;
    }

    pub async fn shutdown(&self) {
        self.diagnostics.record("host_shutdown", "requested").await;
        self.terminate_process().await;
        self.diagnostics.record("host_shutdown", "stopped").await;
    }

    pub(crate) async fn execute(&self, command: HostCommand) -> Result<HostResult, String> {
        self.execute_detailed(command)
            .await
            .map_err(|error| error.message)
    }

    pub(crate) async fn execute_detailed(
        &self,
        command: HostCommand,
    ) -> Result<HostResult, HostExecutionError> {
        let mut process = self.process.lock().await;
        let Some(host) = process.as_mut() else {
            return Err(HostExecutionError {
                code: Some("host_unavailable".into()),
                message: "Local Agent Host 尚未就绪。".into(),
            });
        };
        if command
            .owner_user_id()
            .is_some_and(|owner| owner != host.owner_user_id)
        {
            return Err(HostExecutionError {
                code: Some("owner_mismatch".into()),
                message: "Local Agent Host 当前账号与请求账号不一致。".into(),
            });
        }
        match request(host, command).await {
            Ok(result) => Ok(result),
            Err(error) => {
                if error.fatal {
                    terminate(host).await;
                    *process = None;
                    *self.model_catalog.lock().await = None;
                    self.approval_sessions.reset();
                }
                self.diagnostics
                    .record("host_request", classify_error(&error.message))
                    .await;
                Err(HostExecutionError {
                    code: error.code,
                    message: error.message,
                })
            }
        }
    }

    pub(crate) async fn model_options(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<control_plane::ControlPlaneModelOption>, String> {
        let catalog = self.model_catalog.lock().await;
        let Some((owner, options)) = catalog.as_ref() else {
            return Err("本地模型目录尚未就绪。".into());
        };
        if owner != owner_user_id {
            return Err("本地模型目录与当前账号不一致。".into());
        }
        Ok(options.clone())
    }

    pub(crate) fn approval_sessions(&self) -> &ApprovalSessionAllowlist {
        &self.approval_sessions
    }

    pub(crate) fn reset_approval_sessions(&self) {
        self.approval_sessions.reset();
    }

    pub(crate) async fn is_running_for(&self, owner_user_id: &str) -> bool {
        let mut process = self.process.lock().await;
        let running = process.as_mut().is_some_and(|host| {
            host.owner_user_id == owner_user_id
                && host
                    .child
                    .try_wait()
                    .ok()
                    .is_some_and(|status| status.is_none())
        });
        if !running {
            *process = None;
            *self.model_catalog.lock().await = None;
            self.approval_sessions.reset();
        }
        running
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
    attachment_worker: State<'_, AttachmentToolWorker>,
    terminal_worker: State<'_, TerminalToolWorker>,
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
        *manager.model_catalog.lock().await = None;
        manager.approval_sessions.reset();
        attachment_worker.reset().await;
        terminal_worker.reset().await;
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
            *manager.model_catalog.lock().await = None;
            manager.approval_sessions.reset();
            attachment_worker.reset().await;
            terminal_worker.reset().await;
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
    attachment_worker: State<'_, AttachmentToolWorker>,
    terminal_worker: State<'_, TerminalToolWorker>,
    authentication: State<'_, AuthenticationManager>,
    owner_user_id: String,
) -> Result<HostLifecycleSnapshot, String> {
    if let Err(error) = validate_owner(&owner_user_id) {
        manager
            .diagnostics
            .record("host_start", "invalid_owner")
            .await;
        return Err(error);
    }
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    manager.diagnostics.record("host_start", "requested").await;
    let mut process = manager.process.lock().await;
    if let Some(host) = process.as_mut() {
        let catalog_ready = manager
            .model_catalog
            .lock()
            .await
            .as_ref()
            .is_some_and(|(owner, options)| owner == &owner_user_id && !options.is_empty());
        if host.owner_user_id == owner_user_id && catalog_ready && health_check(host).await.is_ok()
        {
            attachment_worker
                .configure(app.clone(), owner_user_id.clone())
                .await;
            terminal_worker
                .configure(app.clone(), owner_user_id.clone())
                .await;
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
    manager.approval_sessions.reset();
    *manager.model_catalog.lock().await = None;

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
    let state_data_dir = data_dir.clone();
    let state_owner = owner_user_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        connector_state::ensure_owner_state(&state_data_dir, &state_owner)
    })
    .await
    .map_err(|_| "本地工作区状态准备任务异常退出。".to_owned())??;
    let database = data_dir.join("local-agent").join("agent.sqlite3");
    if let Some(parent) = database.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let empty_environment = BTreeMap::new();
    let mut bootstrap_host = match spawn(
        &executable,
        &database,
        owner_user_id.clone(),
        &empty_environment,
        false,
    )
    .await
    {
        Ok(host) => host,
        Err(error) => {
            manager
                .diagnostics
                .record("host_process_spawn", "failed")
                .await;
            return Err(error.to_string());
        }
    };
    if let Err(error) = health_check(&mut bootstrap_host).await {
        terminate(&mut bootstrap_host).await;
        manager
            .diagnostics
            .record("host_start_handshake", classify_error(&error))
            .await;
        return Err(error);
    }
    manager
        .diagnostics
        .record("host_bootstrap", "synchronizing")
        .await;
    let prepared = match control_plane::synchronize(
        authentication.inner(),
        &mut bootstrap_host,
        &owner_user_id,
    )
    .await
    {
        Ok(prepared) => prepared,
        Err(error) => {
            terminate(&mut bootstrap_host).await;
            manager
                .diagnostics
                .record("host_bootstrap", classify_error(&error))
                .await;
            return Err(error);
        }
    };
    terminate(&mut bootstrap_host).await;
    manager
        .diagnostics
        .record(
            "host_bootstrap",
            if prepared.restored_offline {
                "offline_restored"
            } else {
                "refreshed"
            },
        )
        .await;

    let model_options = prepared.model_options;
    let mut credential_environment = prepared.credential_environment;
    let spawn_result = spawn(
        &executable,
        &database,
        owner_user_id,
        &credential_environment,
        true,
    )
    .await;
    for credential in credential_environment.values_mut() {
        credential.clear();
    }
    let mut host = match spawn_result {
        Ok(host) => host,
        Err(error) => {
            manager
                .diagnostics
                .record("host_worker_spawn", "failed")
                .await;
            return Err(error);
        }
    };
    if let Err(error) = health_check(&mut host).await {
        terminate(&mut host).await;
        manager
            .diagnostics
            .record("host_worker_handshake", classify_error(&error))
            .await;
        return Err(error);
    }
    let worker_owner_user_id = host.owner_user_id.clone();
    *manager.model_catalog.lock().await = Some((worker_owner_user_id.clone(), model_options));
    *process = Some(host);
    attachment_worker
        .configure(app.clone(), worker_owner_user_id.clone())
        .await;
    terminal_worker
        .configure(app.clone(), worker_owner_user_id)
        .await;
    manager
        .diagnostics
        .record("host_start_handshake", "ready")
        .await;
    Ok(HostLifecycleSnapshot::ready())
}

#[tauri::command]
pub async fn stop_local_agent_host(
    manager: State<'_, HostManager>,
    attachment_worker: State<'_, AttachmentToolWorker>,
    terminal_worker: State<'_, TerminalToolWorker>,
) -> Result<HostLifecycleSnapshot, String> {
    manager.diagnostics.record("host_stop", "requested").await;
    attachment_worker.reset().await;
    terminal_worker.reset().await;
    manager.terminate_process().await;
    manager.diagnostics.record("host_stop", "stopped").await;
    Ok(HostLifecycleSnapshot::stopped(
        "Local Agent Host 已停止。",
        false,
    ))
}

pub(crate) fn validate_owner(owner: &str) -> Result<(), String> {
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

    #[tokio::test]
    async fn shutdown_clears_in_memory_approval_session() {
        let manager = HostManager::default();
        let generation = manager.approval_sessions().generation();
        assert!(manager
            .approval_sessions()
            .insert_if_current(generation, "v2:test-scope".into()));
        manager.shutdown().await;
        assert!(manager.approval_sessions().is_empty());
    }
}
