use super::{
    ssh::{InteractiveSshLaunch, NativeOpenSshTester, SshConnectionMaterial},
    RemoteConnectionManager, RemoteConnectionMetadataClient,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::Serialize;
use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};
use tauri::State;
use tokio::sync::Mutex;
use uuid::Uuid;

const MAXIMUM_ACTIVE_SESSIONS: usize = 4;
const MAXIMUM_INPUT_BYTES: usize = 64 * 1_024;
const MAXIMUM_OUTPUT_EVENTS: usize = 512;
const MAXIMUM_RETAINED_OUTPUT_BYTES: usize = 2 * 1_024 * 1_024;
const MAXIMUM_POLL_EVENTS: usize = 128;
const MAXIMUM_POLL_BYTES: usize = 256 * 1_024;
const MAXIMUM_POLL_TIMEOUT_MS: u64 = 30_000;
const OUTPUT_READ_BYTES: usize = 8 * 1_024;

type TerminalLaunchFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Arc<ManagedRemoteTerminal>, String>> + Send + 'a>>;

pub(super) trait RemoteTerminalLauncher: Send + Sync {
    fn launch(
        &self,
        identity: RemoteTerminalIdentity,
        material: SshConnectionMaterial,
        verification_code: Option<String>,
        columns: u16,
        rows: u16,
    ) -> TerminalLaunchFuture<'_>;
}

impl RemoteTerminalLauncher for NativeOpenSshTester {
    fn launch(
        &self,
        identity: RemoteTerminalIdentity,
        material: SshConnectionMaterial,
        verification_code: Option<String>,
        columns: u16,
        rows: u16,
    ) -> TerminalLaunchFuture<'_> {
        let prepared = self.prepare_terminal_launch(&material, verification_code);
        Box::pin(async move {
            let prepared = prepared?;
            tauri::async_runtime::spawn_blocking(move || {
                ManagedRemoteTerminal::spawn(identity, prepared, columns, rows)
            })
            .await
            .map_err(|_| terminal_launch_error())?
        })
    }
}

#[cfg(test)]
pub(super) struct DisabledRemoteTerminalLauncher;

#[cfg(test)]
impl RemoteTerminalLauncher for DisabledRemoteTerminalLauncher {
    fn launch(
        &self,
        _identity: RemoteTerminalIdentity,
        _material: SshConnectionMaterial,
        _verification_code: Option<String>,
        _columns: u16,
        _rows: u16,
    ) -> TerminalLaunchFuture<'_> {
        Box::pin(async { Err("远端终端执行器未配置。".into()) })
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct PresentationKey {
    owner_user_id: String,
    presentation_id: String,
}

#[derive(Clone, Debug)]
struct RemoteTerminalConnectToken {
    key: PresentationKey,
    connection_id: String,
    epoch: u64,
    generation: u64,
    request_id: Uuid,
}

#[derive(Clone)]
pub(super) struct RemoteTerminalIdentity {
    session_id: String,
    owner_user_id: String,
    presentation_id: String,
    connection_id: String,
    epoch: u64,
    generation: u64,
}

#[derive(Default)]
struct PresentationLifecycle {
    generation: u64,
    connect_request_id: Option<Uuid>,
    session_id: Option<String>,
}

#[derive(Default)]
struct RemoteTerminalRegistryState {
    epoch: u64,
    presentations: HashMap<PresentationKey, PresentationLifecycle>,
    sessions: HashMap<String, Arc<ManagedRemoteTerminal>>,
}

#[derive(Default)]
pub(super) struct RemoteTerminalRegistry {
    state: Mutex<RemoteTerminalRegistryState>,
}

impl RemoteTerminalRegistry {
    async fn begin_connect(
        &self,
        owner_user_id: &str,
        presentation_id: &str,
        connection_id: &str,
    ) -> Result<RemoteTerminalConnectToken, String> {
        let key = PresentationKey {
            owner_user_id: owner_user_id.to_owned(),
            presentation_id: presentation_id.to_owned(),
        };
        let (token, previous) = {
            let mut state = self.state.lock().await;
            let previous_id = state
                .presentations
                .get(&key)
                .and_then(|lifecycle| lifecycle.session_id.as_ref())
                .cloned();
            let previous = previous_id
                .as_ref()
                .and_then(|session_id| state.sessions.remove(session_id));
            let other_connects = state
                .presentations
                .iter()
                .filter(|(candidate, lifecycle)| {
                    *candidate != &key && lifecycle.connect_request_id.is_some()
                })
                .count();
            if previous.is_none()
                && state.sessions.len().saturating_add(other_connects) >= MAXIMUM_ACTIVE_SESSIONS
            {
                return Err("同时打开的远端终端过多，请先关闭其他终端。".into());
            }
            let epoch = state.epoch;
            let lifecycle = state.presentations.entry(key.clone()).or_default();
            lifecycle.generation = lifecycle.generation.wrapping_add(1);
            lifecycle.session_id = None;
            let request_id = Uuid::new_v4();
            lifecycle.connect_request_id = Some(request_id);
            (
                RemoteTerminalConnectToken {
                    key,
                    connection_id: connection_id.to_owned(),
                    epoch,
                    generation: lifecycle.generation,
                    request_id,
                },
                previous,
            )
        };
        if let Some(previous) = previous {
            previous.close();
        }
        Ok(token)
    }

    async fn activate(
        &self,
        token: &RemoteTerminalConnectToken,
        session: Arc<ManagedRemoteTerminal>,
    ) -> Result<(), String> {
        let mut state = self.state.lock().await;
        let accepted = state.epoch == token.epoch
            && session.identity.owner_user_id == token.key.owner_user_id
            && session.identity.presentation_id == token.key.presentation_id
            && session.identity.connection_id == token.connection_id
            && session.identity.epoch == token.epoch
            && session.identity.generation == token.generation
            && state
                .presentations
                .get(&token.key)
                .is_some_and(|lifecycle| {
                    lifecycle.generation == token.generation
                        && lifecycle.connect_request_id == Some(token.request_id)
                });
        if !accepted {
            drop(state);
            session.close();
            return Err("远端终端连接请求已失效。".into());
        }
        if state.sessions.len() >= MAXIMUM_ACTIVE_SESSIONS {
            drop(state);
            session.close();
            return Err("同时打开的远端终端过多，请先关闭其他终端。".into());
        }
        let session_id = session.identity.session_id.clone();
        state.sessions.insert(session_id.clone(), session);
        let lifecycle = state
            .presentations
            .get_mut(&token.key)
            .expect("accepted presentation lifecycle");
        lifecycle.connect_request_id = None;
        lifecycle.session_id = Some(session_id);
        Ok(())
    }

    async fn finish_connect(&self, token: &RemoteTerminalConnectToken) {
        let mut state = self.state.lock().await;
        if state.epoch != token.epoch {
            return;
        }
        if let Some(lifecycle) = state.presentations.get_mut(&token.key) {
            if lifecycle.generation == token.generation
                && lifecycle.connect_request_id == Some(token.request_id)
            {
                lifecycle.connect_request_id = None;
            }
        }
    }

    async fn accepts_connect(&self, token: &RemoteTerminalConnectToken) -> bool {
        let state = self.state.lock().await;
        state.epoch == token.epoch
            && state
                .presentations
                .get(&token.key)
                .is_some_and(|lifecycle| {
                    lifecycle.generation == token.generation
                        && lifecycle.connect_request_id == Some(token.request_id)
                })
    }

    async fn session(
        &self,
        owner_user_id: &str,
        presentation_id: &str,
        session_id: &str,
    ) -> Result<Arc<ManagedRemoteTerminal>, String> {
        validate_session_id(session_id)?;
        let state = self.state.lock().await;
        state
            .sessions
            .get(session_id)
            .filter(|session| session.belongs_to(owner_user_id, presentation_id))
            .cloned()
            .ok_or_else(session_unavailable)
    }

    async fn close(
        &self,
        owner_user_id: &str,
        presentation_id: &str,
        session_id: &str,
    ) -> Result<(), String> {
        validate_session_id(session_id)?;
        let key = PresentationKey {
            owner_user_id: owner_user_id.to_owned(),
            presentation_id: presentation_id.to_owned(),
        };
        let session = {
            let mut state = self.state.lock().await;
            let session = state
                .sessions
                .get(session_id)
                .filter(|session| session.belongs_to(owner_user_id, presentation_id))
                .cloned()
                .ok_or_else(session_unavailable)?;
            state.sessions.remove(session_id);
            if let Some(lifecycle) = state.presentations.get_mut(&key) {
                lifecycle.generation = lifecycle.generation.wrapping_add(1);
                lifecycle.connect_request_id = None;
                if lifecycle.session_id.as_deref() == Some(session_id) {
                    lifecycle.session_id = None;
                }
            }
            session
        };
        session.close();
        Ok(())
    }

    async fn discard_presentation(&self, owner_user_id: &str, presentation_id: &str) {
        let key = PresentationKey {
            owner_user_id: owner_user_id.to_owned(),
            presentation_id: presentation_id.to_owned(),
        };
        let session = {
            let mut state = self.state.lock().await;
            let session_id = state
                .presentations
                .remove(&key)
                .and_then(|lifecycle| lifecycle.session_id);
            session_id.and_then(|session_id| state.sessions.remove(&session_id))
        };
        if let Some(session) = session {
            session.close();
        }
    }

    pub(super) async fn reset(&self) {
        let sessions = {
            let mut state = self.state.lock().await;
            state.epoch = state.epoch.wrapping_add(1);
            state.presentations.clear();
            state
                .sessions
                .drain()
                .map(|(_, session)| session)
                .collect::<Vec<_>>()
        };
        for session in sessions {
            session.close();
        }
    }
}

#[path = "terminal_process.rs"]
mod process;
use process::{ManagedRemoteTerminal, RemoteTerminalEvent};

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PresentedRemoteTerminalStart {
    Started {
        session_id: String,
        initial_working_directory: String,
    },
    VerificationRequired {
        prompt: String,
    },
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PresentedRemoteTerminalEvent {
    Output {
        cursor: u64,
        #[serde(rename = "dataBase64")]
        data_base64: String,
    },
    Terminated {
        cursor: u64,
        exit_code: Option<i32>,
    },
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PresentedRemoteTerminalPoll {
    session_id: String,
    running: bool,
    exit_code: Option<i32>,
    events: Vec<PresentedRemoteTerminalEvent>,
    next_cursor: u64,
    truncated: bool,
    timed_out: bool,
}

impl RemoteConnectionManager {
    async fn start_terminal(
        &self,
        client: &dyn RemoteConnectionMetadataClient,
        owner_user_id: &str,
        presentation_id: &str,
        connection_id: &str,
        verification_code: Option<String>,
        columns: u32,
        rows: u32,
    ) -> Result<PresentedRemoteTerminalStart, String> {
        let (columns, rows) = validate_dimensions(columns, rows)?;
        validate_presentation_id(presentation_id)?;
        let token = self
            .terminal_sessions
            .begin_connect(owner_user_id, presentation_id, connection_id)
            .await?;
        self.terminal_tester.reset().await;
        let result = async {
            let material = {
                // Metadata and device-local credentials form one security
                // snapshot. Serialize only this read with connection/file
                // mutations; PTY lifetime is governed by its own generation.
                let _mutation = self.mutation.lock().await;
                self.saved_material(client, owner_user_id, connection_id)
                    .await?
            };
            if !self.terminal_sessions.accepts_connect(&token).await {
                return Err("远端终端连接请求已失效。".into());
            }
            if verification_code.is_none() {
                let probe = self.terminal_tester.test(material.clone(), None).await?;
                if !self.terminal_sessions.accepts_connect(&token).await {
                    return Err("远端终端连接请求已失效。".into());
                }
                if probe.verification_required {
                    return Ok(PresentedRemoteTerminalStart::VerificationRequired {
                        prompt: probe
                            .verification_prompt
                            .unwrap_or_else(|| "请输入 SSH 二次验证码。".into()),
                    });
                }
                if !probe.success {
                    return Err(probe.message);
                }
            } else {
                // A one-time code belongs to the real PTY connection. The
                // retained probe must not consume it or keep a second SSH tree.
                self.terminal_tester.reset().await;
            }
            if !self.terminal_sessions.accepts_connect(&token).await {
                return Err("远端终端连接请求已失效。".into());
            }
            let identity = RemoteTerminalIdentity {
                session_id: format!("remote-terminal-{}", Uuid::new_v4()),
                owner_user_id: owner_user_id.to_owned(),
                presentation_id: presentation_id.to_owned(),
                connection_id: token.connection_id.clone(),
                epoch: token.epoch,
                generation: token.generation,
            };
            let session = self
                .terminal_launcher
                .launch(identity, material, verification_code, columns, rows)
                .await?;
            let response = PresentedRemoteTerminalStart::Started {
                session_id: session.identity.session_id.clone(),
                initial_working_directory: session.initial_working_directory.clone(),
            };
            self.terminal_sessions.activate(&token, session).await?;
            Ok(response)
        }
        .await;
        if !matches!(result, Ok(PresentedRemoteTerminalStart::Started { .. })) {
            self.terminal_sessions.finish_connect(&token).await;
        }
        result
    }
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn start_remote_terminal(
    host: State<'_, crate::host_lifecycle::HostManager>,
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    manager: State<'_, RemoteConnectionManager>,
    owner_user_id: String,
    presentation_id: String,
    connection_id: String,
    verification_code: Option<String>,
    columns: u32,
    rows: u32,
) -> Result<PresentedRemoteTerminalStart, String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    manager
        .start_terminal(
            &*host,
            &owner_user_id,
            &presentation_id,
            &connection_id,
            verification_code,
            columns,
            rows,
        )
        .await
}

#[tauri::command]
pub async fn send_remote_terminal_input(
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    manager: State<'_, RemoteConnectionManager>,
    owner_user_id: String,
    presentation_id: String,
    session_id: String,
    data_base64: String,
) -> Result<(), String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    validate_presentation_id(&presentation_id)?;
    if data_base64.len() > MAXIMUM_INPUT_BYTES.saturating_mul(2) {
        return Err("远端终端输入无效。".into());
    }
    let bytes = BASE64
        .decode(data_base64)
        .map_err(|_| "远端终端输入无效。".to_owned())?;
    if bytes.is_empty() || bytes.len() > MAXIMUM_INPUT_BYTES {
        return Err("远端终端输入无效。".into());
    }
    let session = manager
        .terminal_sessions
        .session(&owner_user_id, &presentation_id, &session_id)
        .await?;
    tauri::async_runtime::spawn_blocking(move || session.send(&bytes))
        .await
        .map_err(|_| terminal_io_error())?
}

#[tauri::command]
pub async fn resize_remote_terminal(
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    manager: State<'_, RemoteConnectionManager>,
    owner_user_id: String,
    presentation_id: String,
    session_id: String,
    columns: u32,
    rows: u32,
) -> Result<(), String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    validate_presentation_id(&presentation_id)?;
    let (columns, rows) = validate_dimensions(columns, rows)?;
    let session = manager
        .terminal_sessions
        .session(&owner_user_id, &presentation_id, &session_id)
        .await?;
    tauri::async_runtime::spawn_blocking(move || session.resize(columns, rows))
        .await
        .map_err(|_| terminal_io_error())?
}

#[tauri::command]
pub async fn interrupt_remote_terminal(
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    manager: State<'_, RemoteConnectionManager>,
    owner_user_id: String,
    presentation_id: String,
    session_id: String,
) -> Result<(), String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    validate_presentation_id(&presentation_id)?;
    let session = manager
        .terminal_sessions
        .session(&owner_user_id, &presentation_id, &session_id)
        .await?;
    tauri::async_runtime::spawn_blocking(move || session.send(&[0x03]))
        .await
        .map_err(|_| terminal_io_error())?
}

#[tauri::command]
pub async fn poll_remote_terminal(
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    manager: State<'_, RemoteConnectionManager>,
    owner_user_id: String,
    presentation_id: String,
    session_id: String,
    after_cursor: u64,
    timeout_ms: u64,
) -> Result<PresentedRemoteTerminalPoll, String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    validate_presentation_id(&presentation_id)?;
    let session = manager
        .terminal_sessions
        .session(&owner_user_id, &presentation_id, &session_id)
        .await?;
    let (snapshot, timed_out) = session.poll(after_cursor, timeout_ms).await?;
    Ok(PresentedRemoteTerminalPoll {
        session_id,
        running: snapshot.running,
        exit_code: snapshot.exit_code,
        events: snapshot
            .events
            .into_iter()
            .map(|event| match event {
                RemoteTerminalEvent::Output { cursor, bytes } => {
                    PresentedRemoteTerminalEvent::Output {
                        cursor,
                        data_base64: BASE64.encode(bytes),
                    }
                }
                RemoteTerminalEvent::Terminated { cursor, exit_code } => {
                    PresentedRemoteTerminalEvent::Terminated { cursor, exit_code }
                }
            })
            .collect(),
        next_cursor: snapshot.next_cursor,
        truncated: snapshot.truncated,
        timed_out,
    })
}

#[tauri::command]
pub async fn close_remote_terminal(
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    manager: State<'_, RemoteConnectionManager>,
    owner_user_id: String,
    presentation_id: String,
    session_id: String,
) -> Result<(), String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    validate_presentation_id(&presentation_id)?;
    manager
        .terminal_sessions
        .close(&owner_user_id, &presentation_id, &session_id)
        .await
}

#[tauri::command]
pub async fn discard_remote_terminal_presentation(
    manager: State<'_, RemoteConnectionManager>,
    owner_user_id: String,
    presentation_id: String,
) -> Result<(), String> {
    crate::host_lifecycle::validate_owner(&owner_user_id)?;
    validate_presentation_id(&presentation_id)?;
    manager
        .terminal_sessions
        .discard_presentation(&owner_user_id, &presentation_id)
        .await;
    Ok(())
}

fn validate_dimensions(columns: u32, rows: u32) -> Result<(u16, u16), String> {
    if !(1..=1_000).contains(&columns) || !(1..=1_000).contains(&rows) {
        return Err("远端终端窗口大小无效。".into());
    }
    Ok((columns as u16, rows as u16))
}

fn validate_presentation_id(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
        Err("远端终端窗口标识无效。".into())
    } else {
        Ok(())
    }
}

fn validate_session_id(value: &str) -> Result<(), String> {
    if value.starts_with("remote-terminal-")
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        Ok(())
    } else {
        Err(session_unavailable())
    }
}

fn terminal_launch_error() -> String {
    "无法启动 Windows OpenSSH 远端终端。".into()
}

fn terminal_io_error() -> String {
    "远端终端通信失败。".into()
}

fn session_unavailable() -> String {
    "远端终端会话已结束，请重新连接。".into()
}

#[cfg(test)]
#[path = "terminal_tests.rs"]
mod tests;
