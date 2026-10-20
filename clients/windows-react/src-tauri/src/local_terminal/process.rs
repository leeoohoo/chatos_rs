use super::{
    session_unavailable, terminal_io_error, terminal_launch_error, LocalTerminalIdentity,
    LocalTerminalLaunch, MAXIMUM_INPUT_BYTES, MAXIMUM_OUTPUT_EVENTS, MAXIMUM_POLL_BYTES,
    MAXIMUM_POLL_EVENTS, MAXIMUM_POLL_TIMEOUT_MS, MAXIMUM_RETAINED_OUTPUT_BYTES, OUTPUT_READ_BYTES,
};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::{
    collections::VecDeque,
    io::{self, Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex as StandardMutex,
    },
    time::Duration,
};
use tokio::sync::Notify;

pub(super) struct ManagedLocalTerminal {
    pub(super) identity: LocalTerminalIdentity,
    pub(super) initial_working_directory: String,
    pub(super) shell_name: String,
    master: StandardMutex<Option<Box<dyn MasterPty + Send>>>,
    writer: StandardMutex<Option<Box<dyn Write + Send>>>,
    termination: Arc<PtyProcessTermination>,
    state: StandardMutex<LocalTerminalSessionState>,
    changed: Notify,
    closed: AtomicBool,
    _launch: LocalTerminalLaunch,
}

pub(super) struct LocalTerminalSessionState {
    pub(super) running: bool,
    pub(super) exit_code: Option<i32>,
    pub(super) events: VecDeque<LocalTerminalEvent>,
    pub(super) next_cursor: u64,
    pub(super) retained_output_bytes: usize,
}

#[derive(Clone)]
pub(super) enum LocalTerminalEvent {
    Output { cursor: u64, bytes: Vec<u8> },
    Terminated { cursor: u64, exit_code: Option<i32> },
}

impl LocalTerminalEvent {
    pub(super) fn cursor(&self) -> u64 {
        match self {
            Self::Output { cursor, .. } | Self::Terminated { cursor, .. } => *cursor,
        }
    }

    pub(super) fn output_bytes(&self) -> usize {
        match self {
            Self::Output { bytes, .. } => bytes.len(),
            Self::Terminated { .. } => 0,
        }
    }
}

pub(super) struct LocalTerminalPollSnapshot {
    pub(super) running: bool,
    pub(super) exit_code: Option<i32>,
    pub(super) events: Vec<LocalTerminalEvent>,
    pub(super) next_cursor: u64,
    pub(super) truncated: bool,
}

impl ManagedLocalTerminal {
    pub(super) fn spawn(
        identity: LocalTerminalIdentity,
        launch: LocalTerminalLaunch,
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
                return Err("无法隔离本机终端进程树。".into());
            }
        };
        let session = Arc::new(Self {
            identity,
            initial_working_directory: launch.initial_working_directory.clone(),
            shell_name: launch.shell_name.clone(),
            master: StandardMutex::new(Some(pty.master)),
            writer: StandardMutex::new(Some(writer)),
            termination: termination.clone(),
            state: StandardMutex::new(LocalTerminalSessionState {
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
            .name("chatos-local-terminal-output".into())
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
            .name("chatos-local-terminal-wait".into())
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

    pub(super) fn belongs_to(&self, owner_user_id: &str, presentation_id: &str) -> bool {
        self.identity.owner_user_id == owner_user_id
            && self.identity.presentation_id == presentation_id
    }

    pub(super) fn send(&self, bytes: &[u8]) -> Result<(), String> {
        if bytes.is_empty() || bytes.len() > MAXIMUM_INPUT_BYTES {
            return Err("本机终端输入无效。".into());
        }
        if self.closed.load(Ordering::SeqCst) || !self.is_running() {
            return Err(session_unavailable());
        }
        let mut writer = self.writer.lock().map_err(|_| terminal_io_error())?;
        let writer = writer.as_mut().ok_or_else(session_unavailable)?;
        writer.write_all(bytes).map_err(|_| terminal_io_error())?;
        writer.flush().map_err(|_| terminal_io_error())
    }

    pub(super) fn resize(&self, columns: u16, rows: u16) -> Result<(), String> {
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
            .map_err(|_| "无法调整本机终端窗口大小。".into())
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
            .push_back(LocalTerminalEvent::Output { cursor, bytes });
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
            .push_back(LocalTerminalEvent::Terminated { cursor, exit_code });
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

    pub(super) fn close(&self) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        // Kill the process tree before taking I/O locks so lifecycle teardown
        // cannot be held open by a concurrent bounded write.
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

    fn snapshot(&self, after_cursor: u64) -> Result<LocalTerminalPollSnapshot, String> {
        let state = self.state.lock().map_err(|_| terminal_io_error())?;
        Ok(snapshot_state(&state, after_cursor))
    }

    pub(super) async fn poll(
        &self,
        after_cursor: u64,
        timeout_ms: u64,
    ) -> Result<(LocalTerminalPollSnapshot, bool), String> {
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

impl Drop for ManagedLocalTerminal {
    fn drop(&mut self) {
        self.close();
    }
}

pub(super) fn trim_events(state: &mut LocalTerminalSessionState) {
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

pub(super) fn snapshot_state(
    state: &LocalTerminalSessionState,
    after_cursor: u64,
) -> LocalTerminalPollSnapshot {
    let oldest_cursor = state
        .events
        .front()
        .map(LocalTerminalEvent::cursor)
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
        .map(LocalTerminalEvent::cursor)
        .unwrap_or(effective_after.min(state.next_cursor.saturating_sub(1)));
    LocalTerminalPollSnapshot {
        running: state.running,
        exit_code: state.exit_code,
        events,
        next_cursor,
        truncated,
    }
}

fn read_terminal_output(
    session: std::sync::Weak<ManagedLocalTerminal>,
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
