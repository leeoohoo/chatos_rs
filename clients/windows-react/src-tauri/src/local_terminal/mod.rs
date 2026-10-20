use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::Serialize;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use tauri::State;
use tokio::sync::Mutex;
use uuid::Uuid;

const MAXIMUM_ACTIVE_SESSIONS_PER_OWNER: usize = 4;
const MAXIMUM_INPUT_BYTES: usize = 64 * 1_024;
const MAXIMUM_OUTPUT_EVENTS: usize = 512;
const MAXIMUM_RETAINED_OUTPUT_BYTES: usize = 2 * 1_024 * 1_024;
const MAXIMUM_POLL_EVENTS: usize = 128;
const MAXIMUM_POLL_BYTES: usize = 256 * 1_024;
const MAXIMUM_POLL_TIMEOUT_MS: u64 = 30_000;
const OUTPUT_READ_BYTES: usize = 8 * 1_024;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct PresentationKey {
    owner_user_id: String,
    presentation_id: String,
}

#[derive(Clone, Debug)]
struct LocalTerminalStartToken {
    key: PresentationKey,
    epoch: u64,
    generation: u64,
    request_id: Uuid,
}

#[derive(Clone)]
struct LocalTerminalIdentity {
    session_id: String,
    owner_user_id: String,
    presentation_id: String,
    epoch: u64,
    generation: u64,
}

#[derive(Default)]
struct PresentationLifecycle {
    generation: u64,
    start_request_id: Option<Uuid>,
    session_id: Option<String>,
}

#[derive(Default)]
struct LocalTerminalRegistryState {
    epoch: u64,
    presentations: HashMap<PresentationKey, PresentationLifecycle>,
    sessions: HashMap<String, Arc<ManagedLocalTerminal>>,
}

#[derive(Default)]
pub(crate) struct LocalTerminalRegistry {
    state: Mutex<LocalTerminalRegistryState>,
}

impl LocalTerminalRegistry {
    async fn begin_start(
        &self,
        owner_user_id: &str,
        presentation_id: &str,
    ) -> Result<LocalTerminalStartToken, String> {
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
            let active_for_owner = state
                .sessions
                .values()
                .filter(|session| session.identity.owner_user_id == owner_user_id)
                .count();
            let pending_for_owner = state
                .presentations
                .iter()
                .filter(|(candidate, lifecycle)| {
                    *candidate != &key
                        && candidate.owner_user_id == owner_user_id
                        && lifecycle.start_request_id.is_some()
                })
                .count();
            if previous.is_none()
                && active_for_owner.saturating_add(pending_for_owner)
                    >= MAXIMUM_ACTIVE_SESSIONS_PER_OWNER
            {
                return Err("同时打开的本机终端过多，请先关闭其他终端。".into());
            }
            let epoch = state.epoch;
            let lifecycle = state.presentations.entry(key.clone()).or_default();
            lifecycle.generation = lifecycle.generation.wrapping_add(1);
            lifecycle.session_id = None;
            let request_id = Uuid::new_v4();
            lifecycle.start_request_id = Some(request_id);
            (
                LocalTerminalStartToken {
                    key,
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

    async fn accepts_start(&self, token: &LocalTerminalStartToken) -> bool {
        let state = self.state.lock().await;
        state.epoch == token.epoch
            && state
                .presentations
                .get(&token.key)
                .is_some_and(|lifecycle| {
                    lifecycle.generation == token.generation
                        && lifecycle.start_request_id == Some(token.request_id)
                })
    }

    async fn activate(
        &self,
        token: &LocalTerminalStartToken,
        session: Arc<ManagedLocalTerminal>,
    ) -> Result<(), String> {
        let mut state = self.state.lock().await;
        let accepted = state.epoch == token.epoch
            && session.identity.owner_user_id == token.key.owner_user_id
            && session.identity.presentation_id == token.key.presentation_id
            && session.identity.epoch == token.epoch
            && session.identity.generation == token.generation
            && state
                .presentations
                .get(&token.key)
                .is_some_and(|lifecycle| {
                    lifecycle.generation == token.generation
                        && lifecycle.start_request_id == Some(token.request_id)
                });
        if !accepted {
            drop(state);
            session.close();
            return Err("本机终端启动请求已失效。".into());
        }
        let active_for_owner = state
            .sessions
            .values()
            .filter(|candidate| candidate.identity.owner_user_id == token.key.owner_user_id)
            .count();
        if active_for_owner >= MAXIMUM_ACTIVE_SESSIONS_PER_OWNER {
            drop(state);
            session.close();
            return Err("同时打开的本机终端过多，请先关闭其他终端。".into());
        }
        let session_id = session.identity.session_id.clone();
        state.sessions.insert(session_id.clone(), session);
        let lifecycle = state
            .presentations
            .get_mut(&token.key)
            .expect("accepted local terminal lifecycle");
        lifecycle.start_request_id = None;
        lifecycle.session_id = Some(session_id);
        Ok(())
    }

    async fn finish_start(&self, token: &LocalTerminalStartToken) {
        let mut state = self.state.lock().await;
        if state.epoch != token.epoch {
            return;
        }
        if let Some(lifecycle) = state.presentations.get_mut(&token.key) {
            if lifecycle.generation == token.generation
                && lifecycle.start_request_id == Some(token.request_id)
            {
                lifecycle.start_request_id = None;
            }
        }
    }

    async fn session(
        &self,
        owner_user_id: &str,
        presentation_id: &str,
        session_id: &str,
    ) -> Result<Arc<ManagedLocalTerminal>, String> {
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
                lifecycle.start_request_id = None;
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

    pub(crate) async fn reset(&self) {
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

#[derive(Clone)]
struct LocalTerminalLaunch {
    executable: PathBuf,
    arguments: Vec<String>,
    environment: Vec<(String, String)>,
    working_directory: PathBuf,
    initial_working_directory: String,
    shell_name: String,
}

impl LocalTerminalLaunch {
    fn system() -> Result<Self, String> {
        let home = dirs::home_dir().ok_or_else(terminal_launch_error)?;
        let working_directory = home.canonicalize().map_err(|_| terminal_launch_error())?;
        if !working_directory.is_dir() {
            return Err(terminal_launch_error());
        }
        Self::for_platform(working_directory)
    }

    #[cfg(windows)]
    fn for_platform(working_directory: PathBuf) -> Result<Self, String> {
        use std::os::windows::ffi::OsStringExt;
        use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;

        let mut buffer = vec![0_u16; 32_768];
        let length = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) };
        if length == 0 || length as usize >= buffer.len() {
            return Err(terminal_launch_error());
        }
        let system_directory =
            PathBuf::from(std::ffi::OsString::from_wide(&buffer[..length as usize]));
        let windows_directory = system_directory
            .parent()
            .ok_or_else(terminal_launch_error)?
            .to_path_buf();
        let powershell_directory = system_directory.join("WindowsPowerShell").join("v1.0");
        let executable = powershell_directory.join("powershell.exe");
        if !executable.is_file() {
            return Err(terminal_launch_error());
        }
        let home = working_directory.to_string_lossy().into_owned();
        let path = [
            system_directory.as_path(),
            windows_directory.as_path(),
            powershell_directory.as_path(),
        ]
        .into_iter()
        .map(Path::to_string_lossy)
        .collect::<Vec<_>>()
        .join(";");
        let environment = vec![
            ("COLORTERM".into(), "truecolor".into()),
            ("HOME".into(), home.clone()),
            ("PATH".into(), path),
            (
                "SystemRoot".into(),
                windows_directory.to_string_lossy().into_owned(),
            ),
            ("TERM".into(), "xterm-256color".into()),
            ("USERPROFILE".into(), home),
            (
                "WINDIR".into(),
                windows_directory.to_string_lossy().into_owned(),
            ),
        ];
        Ok(Self {
            executable,
            arguments: vec!["-NoLogo".into()],
            environment,
            initial_working_directory: working_directory.to_string_lossy().into_owned(),
            working_directory,
            shell_name: "PowerShell".into(),
        })
    }

    #[cfg(not(windows))]
    fn for_platform(working_directory: PathBuf) -> Result<Self, String> {
        let executable = if Path::new("/bin/zsh").is_file() {
            PathBuf::from("/bin/zsh")
        } else {
            PathBuf::from("/bin/sh")
        };
        if !executable.is_file() {
            return Err(terminal_launch_error());
        }
        let home = working_directory.to_string_lossy().into_owned();
        Ok(Self {
            shell_name: executable
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("Shell")
                .to_owned(),
            executable,
            arguments: vec!["-l".into()],
            environment: vec![
                ("COLORTERM".into(), "truecolor".into()),
                ("HOME".into(), home.clone()),
                ("LANG".into(), "en_US.UTF-8".into()),
                ("PATH".into(), "/usr/bin:/bin:/usr/sbin:/sbin".into()),
                ("TERM".into(), "xterm-256color".into()),
            ],
            initial_working_directory: home,
            working_directory,
        })
    }
}

#[path = "process.rs"]
mod process;
use process::{LocalTerminalEvent, ManagedLocalTerminal};

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PresentedLocalTerminalStart {
    session_id: String,
    initial_working_directory: String,
    shell_name: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PresentedLocalTerminalEvent {
    Output { cursor: u64, data_base64: String },
    Terminated { cursor: u64, exit_code: Option<i32> },
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PresentedLocalTerminalPoll {
    session_id: String,
    running: bool,
    exit_code: Option<i32>,
    events: Vec<PresentedLocalTerminalEvent>,
    next_cursor: u64,
    truncated: bool,
    timed_out: bool,
}

#[tauri::command]
pub async fn start_local_terminal(
    host: State<'_, crate::host_lifecycle::HostManager>,
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    registry: State<'_, LocalTerminalRegistry>,
    owner_user_id: String,
    presentation_id: String,
    columns: u32,
    rows: u32,
) -> Result<PresentedLocalTerminalStart, String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    if !host.is_running_for(&owner_user_id).await {
        return Err("Local Agent Host 尚未就绪，无法启动本机终端。".into());
    }
    validate_presentation_id(&presentation_id)?;
    let (columns, rows) = validate_dimensions(columns, rows)?;
    let token = registry
        .begin_start(&owner_user_id, &presentation_id)
        .await?;
    let result = async {
        let launch = tauri::async_runtime::spawn_blocking(LocalTerminalLaunch::system)
            .await
            .map_err(|_| terminal_launch_error())??;
        if !registry.accepts_start(&token).await {
            return Err("本机终端启动请求已失效。".into());
        }
        let identity = LocalTerminalIdentity {
            session_id: format!("local-terminal-{}", Uuid::new_v4()),
            owner_user_id: owner_user_id.clone(),
            presentation_id: presentation_id.clone(),
            epoch: token.epoch,
            generation: token.generation,
        };
        let session = tauri::async_runtime::spawn_blocking(move || {
            ManagedLocalTerminal::spawn(identity, launch, columns, rows)
        })
        .await
        .map_err(|_| terminal_launch_error())??;
        let response = PresentedLocalTerminalStart {
            session_id: session.identity.session_id.clone(),
            initial_working_directory: session.initial_working_directory.clone(),
            shell_name: session.shell_name.clone(),
        };
        registry.activate(&token, session).await?;
        Ok(response)
    }
    .await;
    if result.is_err() {
        registry.finish_start(&token).await;
    }
    result
}

#[tauri::command]
pub async fn send_local_terminal_input(
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    registry: State<'_, LocalTerminalRegistry>,
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
        return Err("本机终端输入无效。".into());
    }
    let bytes = BASE64
        .decode(data_base64)
        .map_err(|_| "本机终端输入无效。".to_owned())?;
    if bytes.is_empty() || bytes.len() > MAXIMUM_INPUT_BYTES {
        return Err("本机终端输入无效。".into());
    }
    let session = registry
        .session(&owner_user_id, &presentation_id, &session_id)
        .await?;
    tauri::async_runtime::spawn_blocking(move || session.send(&bytes))
        .await
        .map_err(|_| terminal_io_error())?
}

#[tauri::command]
pub async fn resize_local_terminal(
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    registry: State<'_, LocalTerminalRegistry>,
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
    let session = registry
        .session(&owner_user_id, &presentation_id, &session_id)
        .await?;
    tauri::async_runtime::spawn_blocking(move || session.resize(columns, rows))
        .await
        .map_err(|_| terminal_io_error())?
}

#[tauri::command]
pub async fn interrupt_local_terminal(
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    registry: State<'_, LocalTerminalRegistry>,
    owner_user_id: String,
    presentation_id: String,
    session_id: String,
) -> Result<(), String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    validate_presentation_id(&presentation_id)?;
    let session = registry
        .session(&owner_user_id, &presentation_id, &session_id)
        .await?;
    tauri::async_runtime::spawn_blocking(move || session.send(&[0x03]))
        .await
        .map_err(|_| terminal_io_error())?
}

#[tauri::command]
pub async fn poll_local_terminal(
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    registry: State<'_, LocalTerminalRegistry>,
    owner_user_id: String,
    presentation_id: String,
    session_id: String,
    after_cursor: u64,
    timeout_ms: u64,
) -> Result<PresentedLocalTerminalPoll, String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    validate_presentation_id(&presentation_id)?;
    let session = registry
        .session(&owner_user_id, &presentation_id, &session_id)
        .await?;
    let (snapshot, timed_out) = session.poll(after_cursor, timeout_ms).await?;
    Ok(PresentedLocalTerminalPoll {
        session_id,
        running: snapshot.running,
        exit_code: snapshot.exit_code,
        events: snapshot
            .events
            .into_iter()
            .map(|event| match event {
                LocalTerminalEvent::Output { cursor, bytes } => {
                    PresentedLocalTerminalEvent::Output {
                        cursor,
                        data_base64: BASE64.encode(bytes),
                    }
                }
                LocalTerminalEvent::Terminated { cursor, exit_code } => {
                    PresentedLocalTerminalEvent::Terminated { cursor, exit_code }
                }
            })
            .collect(),
        next_cursor: snapshot.next_cursor,
        truncated: snapshot.truncated,
        timed_out,
    })
}

#[tauri::command]
pub async fn close_local_terminal(
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    registry: State<'_, LocalTerminalRegistry>,
    owner_user_id: String,
    presentation_id: String,
    session_id: String,
) -> Result<(), String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    validate_presentation_id(&presentation_id)?;
    registry
        .close(&owner_user_id, &presentation_id, &session_id)
        .await
}

#[tauri::command]
pub async fn discard_local_terminal_presentation(
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    registry: State<'_, LocalTerminalRegistry>,
    owner_user_id: String,
    presentation_id: String,
) -> Result<(), String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    validate_presentation_id(&presentation_id)?;
    registry
        .discard_presentation(&owner_user_id, &presentation_id)
        .await;
    Ok(())
}

fn validate_dimensions(columns: u32, rows: u32) -> Result<(u16, u16), String> {
    if !(1..=1_000).contains(&columns) || !(1..=1_000).contains(&rows) {
        return Err("本机终端窗口大小无效。".into());
    }
    Ok((columns as u16, rows as u16))
}

fn validate_presentation_id(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        Err("本机终端窗口标识无效。".into())
    } else {
        Ok(())
    }
}

fn validate_session_id(value: &str) -> Result<(), String> {
    if value.starts_with("local-terminal-")
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
    "无法启动 Windows PowerShell 本机终端。".into()
}

fn terminal_io_error() -> String {
    "本机终端通信失败。".into()
}

fn session_unavailable() -> String {
    "本机终端会话已结束，请新建终端。".into()
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
