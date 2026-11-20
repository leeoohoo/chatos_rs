use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::{
    collections::VecDeque,
    ffi::OsString,
    io::{self, Read, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex as StandardMutex,
    },
    time::Duration,
};
use tokio::sync::Notify;

use crate::machine_resource_governor::MachineResourcePermit;

pub(crate) const MAXIMUM_INPUT_BYTES: usize = 64 * 1_024;
pub(crate) const MAXIMUM_OUTPUT_EVENTS: usize = 512;
pub(crate) const MAXIMUM_RETAINED_OUTPUT_BYTES: usize = 2 * 1_024 * 1_024;
pub(crate) const MAXIMUM_POLL_EVENTS: usize = 128;
pub(crate) const MAXIMUM_POLL_BYTES: usize = 256 * 1_024;
pub(crate) const MAXIMUM_POLL_TIMEOUT_MS: u64 = 30_000;
pub(crate) const OUTPUT_READ_BYTES: usize = 8 * 1_024;

pub(crate) struct PtyCommand {
    pub(crate) executable: PathBuf,
    pub(crate) arguments: Vec<OsString>,
    pub(crate) environment: Vec<(OsString, OsString)>,
    pub(crate) working_directory: PathBuf,
}

#[derive(Clone, Copy)]
pub(crate) struct PtyMessages {
    pub(crate) launch_error: &'static str,
    pub(crate) isolation_error: &'static str,
    pub(crate) invalid_input: &'static str,
    pub(crate) io_error: &'static str,
    pub(crate) session_unavailable: &'static str,
    pub(crate) resize_error: &'static str,
    pub(crate) thread_name: &'static str,
}

pub(crate) struct ManagedPtySession {
    master: StandardMutex<Option<Box<dyn MasterPty + Send>>>,
    writer: StandardMutex<Option<Box<dyn Write + Send>>>,
    #[cfg(windows)]
    slave: StandardMutex<Option<Box<dyn portable_pty::SlavePty + Send>>>,
    termination: Arc<PtyProcessTermination>,
    state: StandardMutex<PtySessionState>,
    changed: Notify,
    closed: AtomicBool,
    resource_permit: StandardMutex<Option<MachineResourcePermit>>,
    messages: PtyMessages,
}

pub(crate) struct PtySessionState {
    pub(crate) running: bool,
    pub(crate) exit_code: Option<i32>,
    pub(crate) events: VecDeque<PtyEvent>,
    pub(crate) next_cursor: u64,
    pub(crate) retained_output_bytes: usize,
}

#[derive(Clone)]
pub(crate) enum PtyEvent {
    Output { cursor: u64, bytes: Vec<u8> },
    Terminated { cursor: u64, exit_code: Option<i32> },
}

impl PtyEvent {
    pub(crate) fn cursor(&self) -> u64 {
        match self {
            Self::Output { cursor, .. } | Self::Terminated { cursor, .. } => *cursor,
        }
    }

    pub(crate) fn output_bytes(&self) -> usize {
        match self {
            Self::Output { bytes, .. } => bytes.len(),
            Self::Terminated { .. } => 0,
        }
    }
}

pub(crate) struct PtyPollSnapshot {
    pub(crate) running: bool,
    pub(crate) exit_code: Option<i32>,
    pub(crate) events: Vec<PtyEvent>,
    pub(crate) next_cursor: u64,
    pub(crate) truncated: bool,
}

impl ManagedPtySession {
    pub(crate) fn spawn(
        command: PtyCommand,
        columns: u16,
        rows: u16,
        messages: PtyMessages,
    ) -> Result<Arc<Self>, String> {
        Self::spawn_with_permit(command, columns, rows, messages, None)
    }

    pub(crate) fn spawn_with_permit(
        command: PtyCommand,
        columns: u16,
        rows: u16,
        messages: PtyMessages,
        resource_permit: Option<MachineResourcePermit>,
    ) -> Result<Arc<Self>, String> {
        let pty = native_pty_system()
            .openpty(PtySize {
                rows,
                cols: columns,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|_| messages.launch_error.to_owned())?;
        let reader = pty
            .master
            .try_clone_reader()
            .map_err(|_| messages.launch_error.to_owned())?;
        let writer = pty
            .master
            .take_writer()
            .map_err(|_| messages.launch_error.to_owned())?;
        let mut builder = CommandBuilder::new(&command.executable);
        builder.args(&command.arguments);
        builder.env_clear();
        for (key, value) in &command.environment {
            builder.env(key, value);
        }
        builder.cwd(&command.working_directory);
        let slave = pty.slave;
        let mut child = slave
            .spawn_command(builder)
            .map_err(|_| messages.launch_error.to_owned())?;
        #[cfg(not(windows))]
        drop(slave);
        let process_group = process_group(&*pty.master);
        let termination = match PtyProcessTermination::attach(&*child, process_group) {
            Ok(termination) => Arc::new(termination),
            Err(_) => {
                let _ = child.kill();
                return Err(messages.isolation_error.to_owned());
            }
        };
        let session = Arc::new(Self {
            master: StandardMutex::new(Some(pty.master)),
            writer: StandardMutex::new(Some(writer)),
            #[cfg(windows)]
            slave: StandardMutex::new(Some(slave)),
            termination: termination.clone(),
            state: StandardMutex::new(PtySessionState {
                running: true,
                exit_code: None,
                events: VecDeque::new(),
                next_cursor: 1,
                retained_output_bytes: 0,
            }),
            changed: Notify::new(),
            closed: AtomicBool::new(false),
            resource_permit: StandardMutex::new(resource_permit),
            messages,
        });
        let (reader_done_tx, reader_done_rx) = std::sync::mpsc::channel();
        let weak_reader = Arc::downgrade(&session);
        std::thread::Builder::new()
            .name(format!("chatos-{}-terminal-output", messages.thread_name))
            .spawn(move || {
                read_terminal_output(weak_reader, reader);
                let _ = reader_done_tx.send(());
            })
            .map_err(|_| {
                session.close();
                messages.launch_error.to_owned()
            })?;
        let weak_wait = Arc::downgrade(&session);
        std::thread::Builder::new()
            .name(format!("chatos-{}-terminal-wait", messages.thread_name))
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
                messages.launch_error.to_owned()
            })?;
        Ok(session)
    }

    pub(crate) fn send(&self, bytes: &[u8]) -> Result<(), String> {
        if bytes.is_empty() || bytes.len() > MAXIMUM_INPUT_BYTES {
            return Err(self.messages.invalid_input.into());
        }
        if self.closed.load(Ordering::SeqCst) || !self.is_running() {
            return Err(self.messages.session_unavailable.into());
        }
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| self.messages.io_error.to_owned())?;
        let writer = writer
            .as_mut()
            .ok_or_else(|| self.messages.session_unavailable.to_owned())?;
        writer
            .write_all(bytes)
            .map_err(|_| self.messages.io_error.to_owned())?;
        writer
            .flush()
            .map_err(|_| self.messages.io_error.to_owned())
    }

    pub(crate) fn resize(&self, columns: u16, rows: u16) -> Result<(), String> {
        if self.closed.load(Ordering::SeqCst) || !self.is_running() {
            return Err(self.messages.session_unavailable.into());
        }
        let master = self
            .master
            .lock()
            .map_err(|_| self.messages.io_error.to_owned())?;
        let master = master
            .as_ref()
            .ok_or_else(|| self.messages.session_unavailable.to_owned())?;
        master
            .resize(PtySize {
                rows,
                cols: columns,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|_| self.messages.resize_error.into())
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
        state.events.push_back(PtyEvent::Output { cursor, bytes });
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
            .push_back(PtyEvent::Terminated { cursor, exit_code });
        trim_events(&mut state);
        if let Ok(mut writer) = self.writer.lock() {
            writer.take();
        }
        if let Ok(mut master) = self.master.lock() {
            master.take();
        }
        #[cfg(windows)]
        if let Ok(mut slave) = self.slave.lock() {
            slave.take();
        }
        self.release_resource();
        drop(state);
        self.changed.notify_waiters();
    }

    pub(crate) fn close(&self) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        self.termination.finalize();
        if let Ok(mut master) = self.master.lock() {
            master.take();
        }
        if let Ok(mut writer) = self.writer.lock() {
            writer.take();
        }
        #[cfg(windows)]
        if let Ok(mut slave) = self.slave.lock() {
            slave.take();
        }
        self.release_resource();
        self.changed.notify_waiters();
    }

    fn release_resource(&self) {
        if let Ok(mut permit) = self.resource_permit.lock() {
            permit.take();
        }
    }

    fn is_running(&self) -> bool {
        self.state.lock().is_ok_and(|state| state.running)
    }

    fn snapshot(&self, after_cursor: u64) -> Result<PtyPollSnapshot, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| self.messages.io_error.to_owned())?;
        Ok(snapshot_state(&state, after_cursor))
    }

    pub(crate) async fn poll(
        &self,
        after_cursor: u64,
        timeout_ms: u64,
    ) -> Result<(PtyPollSnapshot, bool), String> {
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

impl Drop for ManagedPtySession {
    fn drop(&mut self) {
        self.close();
    }
}

pub(crate) fn trim_events(state: &mut PtySessionState) {
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

pub(crate) fn snapshot_state(state: &PtySessionState, after_cursor: u64) -> PtyPollSnapshot {
    let oldest_cursor = state
        .events
        .front()
        .map(PtyEvent::cursor)
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
        .map(PtyEvent::cursor)
        .unwrap_or(effective_after.min(state.next_cursor.saturating_sub(1)));
    PtyPollSnapshot {
        running: state.running,
        exit_code: state.exit_code,
        events,
        next_cursor,
        truncated,
    }
}

fn read_terminal_output(
    session: std::sync::Weak<ManagedPtySession>,
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
