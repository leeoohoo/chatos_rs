use super::{
    ssh::{InteractiveSshLaunch, NativeOpenSshTester, SshConnectionMaterial},
    RemoteConnectionManager, RemoteConnectionMetadataClient,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    io::{self, Read, Write},
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex as StandardMutex,
    },
    time::Duration,
};
use tauri::State;
use tokio::sync::{Mutex, Notify};
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

pub(super) struct ManagedRemoteTerminal {
    identity: RemoteTerminalIdentity,
    initial_working_directory: String,
    master: StandardMutex<Option<Box<dyn MasterPty + Send>>>,
    writer: StandardMutex<Option<Box<dyn Write + Send>>>,
    termination: Arc<PtyProcessTermination>,
    state: StandardMutex<RemoteTerminalSessionState>,
    changed: Notify,
    closed: AtomicBool,
    _launch: InteractiveSshLaunch,
}

struct RemoteTerminalSessionState {
    running: bool,
    exit_code: Option<i32>,
    events: VecDeque<RemoteTerminalEvent>,
    next_cursor: u64,
    retained_output_bytes: usize,
}

#[derive(Clone)]
enum RemoteTerminalEvent {
    Output { cursor: u64, bytes: Vec<u8> },
    Terminated { cursor: u64, exit_code: Option<i32> },
}

impl RemoteTerminalEvent {
    fn cursor(&self) -> u64 {
        match self {
            Self::Output { cursor, .. } | Self::Terminated { cursor, .. } => *cursor,
        }
    }

    fn output_bytes(&self) -> usize {
        match self {
            Self::Output { bytes, .. } => bytes.len(),
            Self::Terminated { .. } => 0,
        }
    }
}

struct RemoteTerminalPollSnapshot {
    running: bool,
    exit_code: Option<i32>,
    events: Vec<RemoteTerminalEvent>,
    next_cursor: u64,
    truncated: bool,
}

impl ManagedRemoteTerminal {
    fn spawn(
        identity: RemoteTerminalIdentity,
        launch: InteractiveSshLaunch,
        columns: u16,
        rows: u16,
    ) -> Result<Arc<Self>, String> {
        let pty = native_pty_system()
            .openpty(PtySize {
                rows,
                cols: columns,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|_| terminal_launch_error())?;
        let reader = pty
            .master
            .try_clone_reader()
            .map_err(|_| terminal_launch_error())?;
        let writer = pty
            .master
            .take_writer()
            .map_err(|_| terminal_launch_error())?;
        let mut command = CommandBuilder::new(&launch.executable);
        command.args(&launch.arguments);
        command.env_clear();
        for (key, value) in &launch.environment {
            command.env(key, value);
        }
        command.cwd(&launch.working_directory);
        let mut child = pty
            .slave
            .spawn_command(command)
            .map_err(|_| terminal_launch_error())?;
        drop(pty.slave);
        let process_group = process_group(&*pty.master);
        let termination = match PtyProcessTermination::attach(&*child, process_group) {
            Ok(termination) => Arc::new(termination),
            Err(_) => {
                let _ = child.kill();
                return Err("无法隔离远端终端进程树。".into());
            }
        };
        let session = Arc::new(Self {
            identity,
            initial_working_directory: launch.initial_working_directory.clone(),
            master: StandardMutex::new(Some(pty.master)),
            writer: StandardMutex::new(Some(writer)),
            termination: termination.clone(),
            state: StandardMutex::new(RemoteTerminalSessionState {
                running: true,
                exit_code: None,
                events: VecDeque::new(),
                next_cursor: 1,
                retained_output_bytes: 0,
            }),
            changed: Notify::new(),
            closed: AtomicBool::new(false),
            _launch: launch,
        });
        let (reader_done_tx, reader_done_rx) = std::sync::mpsc::channel();
        let weak_reader = Arc::downgrade(&session);
        std::thread::Builder::new()
            .name("chatos-remote-terminal-output".into())
            .spawn(move || {
                read_terminal_output(weak_reader, reader);
                let _ = reader_done_tx.send(());
            })
            .map_err(|_| {
                session.close();
                terminal_launch_error()
            })?;
        let weak_wait = Arc::downgrade(&session);
        std::thread::Builder::new()
            .name("chatos-remote-terminal-wait".into())
            .spawn(move || {
                let exit_code = child
                    .wait()
                    .ok()
                    .and_then(|status| i32::try_from(status.exit_code()).ok());
                termination.finalize();
                let _ = reader_done_rx.recv_timeout(Duration::from_secs(1));
                if let Some(session) = weak_wait.upgrade() {
                    session.finish(exit_code);
                }
            })
            .map_err(|_| {
                session.close();
                terminal_launch_error()
            })?;
        Ok(session)
    }

    fn belongs_to(&self, owner_user_id: &str, presentation_id: &str) -> bool {
        self.identity.owner_user_id == owner_user_id
            && self.identity.presentation_id == presentation_id
    }

    fn send(&self, bytes: &[u8]) -> Result<(), String> {
        if bytes.is_empty() || bytes.len() > MAXIMUM_INPUT_BYTES {
            return Err("远端终端输入无效。".into());
        }
        if self.closed.load(Ordering::SeqCst) || !self.is_running() {
            return Err(session_unavailable());
        }
        let mut writer = self.writer.lock().map_err(|_| terminal_io_error())?;
        let writer = writer.as_mut().ok_or_else(session_unavailable)?;
        writer.write_all(bytes).map_err(|_| terminal_io_error())?;
        writer.flush().map_err(|_| terminal_io_error())
    }

    fn resize(&self, columns: u16, rows: u16) -> Result<(), String> {
        if self.closed.load(Ordering::SeqCst) || !self.is_running() {
            return Err(session_unavailable());
        }
        let master = self.master.lock().map_err(|_| terminal_io_error())?;
        let master = master.as_ref().ok_or_else(session_unavailable)?;
        master
            .resize(PtySize {
                rows,
                cols: columns,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|_| "无法调整远端终端窗口大小。".into())
    }

    fn append_output(&self, bytes: Vec<u8>) {
        if bytes.is_empty() || self.closed.load(Ordering::SeqCst) {
            return;
        }
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if !state.running {
            return;
        }
        let cursor = state.next_cursor;
        state.next_cursor = state.next_cursor.saturating_add(1);
        state.retained_output_bytes = state.retained_output_bytes.saturating_add(bytes.len());
        state
            .events
            .push_back(RemoteTerminalEvent::Output { cursor, bytes });
        trim_events(&mut state);
        drop(state);
        self.changed.notify_waiters();
    }

    fn finish(&self, exit_code: Option<i32>) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if !state.running {
            return;
        }
        state.running = false;
        state.exit_code = exit_code;
        let cursor = state.next_cursor;
        state.next_cursor = state.next_cursor.saturating_add(1);
        state
            .events
            .push_back(RemoteTerminalEvent::Terminated { cursor, exit_code });
        trim_events(&mut state);
        if let Ok(mut writer) = self.writer.lock() {
            writer.take();
        }
        if let Ok(mut master) = self.master.lock() {
            master.take();
        }
        drop(state);
        self.changed.notify_waiters();
    }

    fn close(&self) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        // Terminate first so a concurrent bounded write cannot keep reset,
        // logout or application shutdown waiting on the writer mutex.
        self.termination.finalize();
        if let Ok(mut master) = self.master.lock() {
            master.take();
        }
        if let Ok(mut writer) = self.writer.lock() {
            writer.take();
        }
        self.changed.notify_waiters();
    }

    fn is_running(&self) -> bool {
        self.state.lock().is_ok_and(|state| state.running)
    }

    fn snapshot(&self, after_cursor: u64) -> Result<RemoteTerminalPollSnapshot, String> {
        let state = self.state.lock().map_err(|_| terminal_io_error())?;
        Ok(snapshot_state(&state, after_cursor))
    }

    async fn poll(
        &self,
        after_cursor: u64,
        timeout_ms: u64,
    ) -> Result<(RemoteTerminalPollSnapshot, bool), String> {
        let timeout_ms = timeout_ms.min(MAXIMUM_POLL_TIMEOUT_MS);
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            let changed = self.changed.notified();
            let snapshot = self.snapshot(after_cursor)?;
            if !snapshot.events.is_empty() || !snapshot.running || timeout_ms == 0 {
                return Ok((snapshot, false));
            }
            if tokio::time::timeout_at(deadline, changed).await.is_err() {
                return Ok((self.snapshot(after_cursor)?, true));
            }
        }
    }
}

impl Drop for ManagedRemoteTerminal {
    fn drop(&mut self) {
        self.close();
    }
}

fn trim_events(state: &mut RemoteTerminalSessionState) {
    while state.events.len() > MAXIMUM_OUTPUT_EVENTS
        || state.retained_output_bytes > MAXIMUM_RETAINED_OUTPUT_BYTES
    {
        let Some(removed) = state.events.pop_front() else {
            break;
        };
        state.retained_output_bytes = state
            .retained_output_bytes
            .saturating_sub(removed.output_bytes());
    }
}

fn snapshot_state(
    state: &RemoteTerminalSessionState,
    after_cursor: u64,
) -> RemoteTerminalPollSnapshot {
    let oldest_cursor = state
        .events
        .front()
        .map(RemoteTerminalEvent::cursor)
        .unwrap_or(state.next_cursor);
    let truncated = after_cursor.saturating_add(1) < oldest_cursor;
    let effective_after = if truncated {
        oldest_cursor.saturating_sub(1)
    } else {
        after_cursor
    };
    let mut events = Vec::new();
    let mut output_bytes = 0_usize;
    for event in state
        .events
        .iter()
        .filter(|event| event.cursor() > effective_after)
    {
        let next_bytes = output_bytes.saturating_add(event.output_bytes());
        if !events.is_empty()
            && (events.len() >= MAXIMUM_POLL_EVENTS || next_bytes > MAXIMUM_POLL_BYTES)
        {
            break;
        }
        output_bytes = next_bytes;
        events.push(event.clone());
    }
    let next_cursor = events
        .last()
        .map(RemoteTerminalEvent::cursor)
        .unwrap_or(effective_after.min(state.next_cursor.saturating_sub(1)));
    RemoteTerminalPollSnapshot {
        running: state.running,
        exit_code: state.exit_code,
        events,
        next_cursor,
        truncated,
    }
}

fn read_terminal_output(
    session: std::sync::Weak<ManagedRemoteTerminal>,
    mut reader: Box<dyn Read + Send>,
) {
    let mut buffer = vec![0_u8; OUTPUT_READ_BYTES];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return,
            Ok(count) => {
                let Some(session) = session.upgrade() else {
                    return;
                };
                session.append_output(buffer[..count].to_vec());
            }
            Err(_) => return,
        }
    }
}

struct PtyProcessTermination {
    inner: PtyProcessTerminationInner,
    finalized: AtomicBool,
}

enum PtyProcessTerminationInner {
    #[cfg(windows)]
    WindowsJob(std::os::windows::io::OwnedHandle),
    #[cfg(unix)]
    UnixProcessGroup(i32),
}

impl PtyProcessTermination {
    fn attach(child: &dyn portable_pty::Child, _process_group: Option<i32>) -> io::Result<Self> {
        #[cfg(windows)]
        {
            use std::os::windows::io::{FromRawHandle, RawHandle};
            use windows_sys::Win32::System::JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
                SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            };
            let raw = child
                .as_raw_handle()
                .ok_or_else(|| io::Error::other("missing child process handle"))?;
            let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if job.is_null() {
                return Err(io::Error::last_os_error());
            }
            let owned =
                unsafe { std::os::windows::io::OwnedHandle::from_raw_handle(job as RawHandle) };
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let configured = unsafe {
                SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    &limits as *const _ as *const std::ffi::c_void,
                    std::mem::size_of_val(&limits) as u32,
                )
            };
            if configured == 0 {
                return Err(io::Error::last_os_error());
            }
            if unsafe { AssignProcessToJobObject(job, raw as _) } == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self {
                inner: PtyProcessTerminationInner::WindowsJob(owned),
                finalized: AtomicBool::new(false),
            })
        }
        #[cfg(unix)]
        {
            let group = _process_group
                .or_else(|| child.process_id().and_then(|id| i32::try_from(id).ok()))
                .ok_or_else(|| io::Error::other("missing child process group"))?;
            Ok(Self {
                inner: PtyProcessTerminationInner::UnixProcessGroup(group),
                finalized: AtomicBool::new(false),
            })
        }
    }

    fn finalize(&self) {
        if self.finalized.swap(true, Ordering::SeqCst) {
            return;
        }
        match &self.inner {
            #[cfg(windows)]
            PtyProcessTerminationInner::WindowsJob(job) => {
                use std::os::windows::io::AsRawHandle;
                use windows_sys::Win32::System::JobObjects::TerminateJobObject;
                let _ = unsafe { TerminateJobObject(job.as_raw_handle() as _, 1) };
            }
            #[cfg(unix)]
            PtyProcessTerminationInner::UnixProcessGroup(group) => {
                let result = unsafe { libc::kill(-*group, libc::SIGKILL) };
                if result != 0 && io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH) {
                    // Cleanup is best effort after the primary process has ended.
                }
            }
        }
    }
}

impl Drop for PtyProcessTermination {
    fn drop(&mut self) {
        self.finalize();
    }
}

#[cfg(unix)]
fn process_group(master: &dyn MasterPty) -> Option<i32> {
    master.process_group_leader()
}

#[cfg(windows)]
fn process_group(_master: &dyn MasterPty) -> Option<i32> {
    None
}

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
    Output { cursor: u64, data_base64: String },
    Terminated { cursor: u64, exit_code: Option<i32> },
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
mod tests {
    use super::*;
    use crate::remote_connection::{
        credentials::{tests_support::memory_vault, RemoteCredentialVault},
        ssh::{RemoteConnectionTester, ResetFuture, TestFuture},
        MetadataFuture, RemoteConnectorRoute, RemoteConnectorRouteProvider,
    };
    use chatos_local_agent_protocol::{
        HostCommand, HostResult, LocalRemoteAuthenticationType, LocalRemoteConnection,
        LocalRemoteHostKeyPolicy,
    };
    use std::sync::atomic::AtomicUsize;

    fn state() -> RemoteTerminalSessionState {
        RemoteTerminalSessionState {
            running: true,
            exit_code: None,
            events: VecDeque::new(),
            next_cursor: 1,
            retained_output_bytes: 0,
        }
    }

    fn append(state: &mut RemoteTerminalSessionState, bytes: Vec<u8>) {
        let cursor = state.next_cursor;
        state.next_cursor += 1;
        state.retained_output_bytes += bytes.len();
        state
            .events
            .push_back(RemoteTerminalEvent::Output { cursor, bytes });
        trim_events(state);
    }

    struct FixedRoute;

    impl RemoteConnectorRouteProvider for FixedRoute {
        fn route(&self, _owner_user_id: &str) -> Result<RemoteConnectorRoute, String> {
            Ok(RemoteConnectorRoute {
                device_id: "device-1".into(),
                workspace_id: "workspace-1".into(),
            })
        }
    }

    struct FixedMetadata;

    impl RemoteConnectionMetadataClient for FixedMetadata {
        fn execute(&self, command: HostCommand) -> MetadataFuture<'_> {
            Box::pin(async move {
                assert!(matches!(command, HostCommand::GetRemoteConnection(_)));
                Ok(HostResult::RemoteConnection {
                    connection: Some(LocalRemoteConnection {
                        connection_id: "connection-1".into(),
                        owner_user_id: "owner-1".into(),
                        name: "Server".into(),
                        host: "server.example.com".into(),
                        port: 22,
                        username: "deploy".into(),
                        authentication_type: LocalRemoteAuthenticationType::Password,
                        has_password: true,
                        has_private_key_path: false,
                        has_certificate_path: false,
                        default_remote_path: Some("/srv/app".into()),
                        host_key_policy: LocalRemoteHostKeyPolicy::Strict,
                        local_connector_device_id: "device-1".into(),
                        local_connector_workspace_id: "workspace-1".into(),
                        jump_enabled: false,
                        jump_connection_id: None,
                        jump_host: None,
                        jump_port: None,
                        jump_username: None,
                        has_jump_private_key_path: false,
                        has_jump_certificate_path: false,
                        has_jump_password: false,
                        last_active_at_unix_ms: None,
                        version: 1,
                        created_at_unix_ms: 1,
                        updated_at_unix_ms: 1,
                    }),
                })
            })
        }
    }

    struct VerificationTester {
        calls: AtomicUsize,
    }

    impl RemoteConnectionTester for VerificationTester {
        fn test(
            &self,
            _connection: SshConnectionMaterial,
            _verification_code: Option<String>,
        ) -> TestFuture<'_> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async {
                Ok(
                    super::super::models::PresentedRemoteConnectionTestResult::verification(
                        "OTP".into(),
                    ),
                )
            })
        }

        fn reset(&self) -> ResetFuture<'_> {
            Box::pin(async {})
        }
    }

    fn manager_with_verification_tester(
        tester: Arc<VerificationTester>,
    ) -> RemoteConnectionManager {
        let directory = tempfile::tempdir().unwrap().keep();
        let vault = Arc::new(memory_vault(directory.join("credentials.json")));
        vault
            .save(
                "owner-1",
                "connection-1",
                &super::super::models::RemoteCredentialBundle {
                    password: Some("secret".into()),
                    ..super::super::models::RemoteCredentialBundle::default()
                },
            )
            .unwrap();
        RemoteConnectionManager::with_dependencies(vault, Arc::new(FixedRoute), tester)
    }

    #[test]
    fn dimensions_match_macos_bounds() {
        assert_eq!(validate_dimensions(1, 1).unwrap(), (1, 1));
        assert_eq!(validate_dimensions(1_000, 1_000).unwrap(), (1_000, 1_000));
        assert!(validate_dimensions(0, 24).is_err());
        assert!(validate_dimensions(80, 1_001).is_err());
    }

    #[test]
    fn terminal_output_ring_is_bounded_and_reports_cursor_truncation() {
        let mut state = state();
        for _ in 0..=MAXIMUM_OUTPUT_EVENTS {
            append(&mut state, vec![b'x'; 8]);
        }
        assert_eq!(state.events.len(), MAXIMUM_OUTPUT_EVENTS);
        assert_eq!(state.events.front().unwrap().cursor(), 2);
        assert_eq!(state.retained_output_bytes, MAXIMUM_OUTPUT_EVENTS * 8);

        let snapshot = snapshot_state(&state, 0);
        assert!(snapshot.truncated);
        assert_eq!(snapshot.events.first().unwrap().cursor(), 2);
        assert_eq!(snapshot.next_cursor, 129);
        assert_eq!(snapshot.events.len(), MAXIMUM_POLL_EVENTS);
    }

    #[test]
    fn output_ring_enforces_byte_budget() {
        let mut state = state();
        for _ in 0..300 {
            append(&mut state, vec![b'x'; OUTPUT_READ_BYTES]);
        }
        assert!(state.retained_output_bytes <= MAXIMUM_RETAINED_OUTPUT_BYTES);
        assert!(state.events.len() <= MAXIMUM_OUTPUT_EVENTS);
    }

    #[test]
    fn input_and_opaque_identity_validation_fail_closed() {
        assert!(validate_presentation_id("terminal-window-1").is_ok());
        assert!(validate_presentation_id("bad\nwindow").is_err());
        assert!(validate_session_id("remote-terminal-1234-abcd").is_ok());
        assert!(validate_session_id("terminal-1234").is_err());
    }

    #[tokio::test]
    async fn newer_connect_generation_rejects_an_older_completion() {
        let registry = RemoteTerminalRegistry::default();
        let first = registry
            .begin_connect("owner-1", "window-1", "connection-1")
            .await
            .unwrap();
        let second = registry
            .begin_connect("owner-1", "window-1", "connection-2")
            .await
            .unwrap();

        registry.finish_connect(&first).await;
        let state = registry.state.lock().await;
        let lifecycle = state.presentations.get(&second.key).unwrap();
        assert_eq!(lifecycle.generation, second.generation);
        assert_eq!(lifecycle.connect_request_id, Some(second.request_id));
    }

    #[tokio::test]
    async fn reset_invalidates_every_in_flight_connect() {
        let registry = RemoteTerminalRegistry::default();
        let token = registry
            .begin_connect("owner-1", "window-1", "connection-1")
            .await
            .unwrap();
        registry.reset().await;
        registry.finish_connect(&token).await;

        let state = registry.state.lock().await;
        assert!(state.presentations.is_empty());
        assert!(state.sessions.is_empty());
        assert_ne!(state.epoch, token.epoch);
    }

    #[tokio::test]
    async fn in_flight_connects_are_included_in_the_native_session_limit() {
        let registry = RemoteTerminalRegistry::default();
        for index in 0..MAXIMUM_ACTIVE_SESSIONS {
            registry
                .begin_connect("owner-1", &format!("window-{index}"), "connection-1")
                .await
                .unwrap();
        }

        let error = registry
            .begin_connect("owner-1", "one-too-many", "connection-1")
            .await
            .unwrap_err();
        assert!(error.contains("过多"));
    }

    #[tokio::test]
    async fn terminal_start_uses_the_saved_connection_probe_for_mfa() {
        let tester = Arc::new(VerificationTester {
            calls: AtomicUsize::new(0),
        });
        let manager = manager_with_verification_tester(tester.clone());

        let response = manager
            .start_terminal(
                &FixedMetadata,
                "owner-1",
                "window-1",
                "connection-1",
                None,
                80,
                24,
            )
            .await
            .unwrap();

        assert_eq!(
            response,
            PresentedRemoteTerminalStart::VerificationRequired {
                prompt: "OTP".into()
            }
        );
        assert_eq!(tester.calls.load(Ordering::SeqCst), 1);
        let state = manager.terminal_sessions.state.lock().await;
        assert!(state.sessions.is_empty());
        assert!(state
            .presentations
            .values()
            .all(|lifecycle| lifecycle.connect_request_id.is_none()));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn native_pty_streams_output_and_termination_as_ordered_events() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = NativeOpenSshTester::new(directory.path().to_path_buf());
        let material = SshConnectionMaterial {
            spec: chatos_local_agent_protocol::LocalRemoteConnectionSpec {
                name: Some("Server".into()),
                host: "server.example.com".into(),
                port: 22,
                username: "deploy".into(),
                authentication_type: LocalRemoteAuthenticationType::Password,
                default_remote_path: Some("/srv/app".into()),
                host_key_policy: LocalRemoteHostKeyPolicy::Strict,
                local_connector_device_id: "device-1".into(),
                local_connector_workspace_id: "workspace-1".into(),
                jump_enabled: false,
                jump_connection_id: None,
                jump_host: None,
                jump_port: None,
                jump_username: None,
            },
            credentials: super::super::models::RemoteCredentialBundle {
                password: Some("secret".into()),
                ..super::super::models::RemoteCredentialBundle::default()
            },
            jump_authentication_type: None,
            jump_credentials: None,
        };
        let mut launch = runtime.prepare_terminal_launch(&material, None).unwrap();
        assert_eq!(
            launch.arguments.first().and_then(|value| value.to_str()),
            Some("-q")
        );
        launch.executable = "/bin/sh".into();
        launch.arguments = vec![
            "-lc".into(),
            "printf '__CHATOS_PTY_TEST__\\n'; exit 7".into(),
        ];
        let session = ManagedRemoteTerminal::spawn(
            RemoteTerminalIdentity {
                session_id: "remote-terminal-test".into(),
                owner_user_id: "owner-1".into(),
                presentation_id: "window-1".into(),
                connection_id: "connection-1".into(),
                epoch: 0,
                generation: 1,
            },
            launch,
            80,
            24,
        )
        .unwrap();

        let mut cursor = 0;
        let mut output = Vec::new();
        let exit_code = loop {
            let (snapshot, _) = session.poll(cursor, 2_000).await.unwrap();
            for event in snapshot.events {
                cursor = event.cursor();
                match event {
                    RemoteTerminalEvent::Output { bytes, .. } => output.extend(bytes),
                    RemoteTerminalEvent::Terminated { .. } => {}
                }
            }
            if !snapshot.running {
                break snapshot.exit_code;
            }
        };

        assert!(String::from_utf8_lossy(&output).contains("__CHATOS_PTY_TEST__"));
        assert_eq!(exit_code, Some(7));
        session.close();
    }
}
