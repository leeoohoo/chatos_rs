use super::{TerminalRuntimeError, TerminalScope};
use serde_json::json;
use std::{
    collections::VecDeque,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::{Child, ChildStdin, Command},
    sync::{Mutex, Notify},
    time::{sleep, timeout, Duration},
};
use uuid::Uuid;

#[cfg(windows)]
use crate::windows_appcontainer::SandboxedChild;

const MAXIMUM_LOGS: usize = 4_000;
const RETAINED_LOG_TARGET: usize = 3_500;
const MAXIMUM_LOG_BYTES: usize = 2 * 1_024 * 1_024;
const RETAINED_LOG_BYTES_TARGET: usize = 1_536 * 1_024;
const MAXIMUM_LOG_ENTRY_BYTES: usize = 2 * 1_024 * 1_024;
const MAXIMUM_WRITE_BYTES: usize = 2 * 1_024 * 1_024;

pub(super) struct ManagedProcess {
    id: String,
    sequence: u64,
    command: String,
    cwd: PathBuf,
    scope: TerminalScope,
    termination: ProcessTermination,
    state: Mutex<ProcessState>,
    changed: Notify,
}

struct ProcessState {
    status: ProcessStatus,
    exit_code: Option<i32>,
    stdin: Option<ManagedStdin>,
    started_at_unix_ms: i64,
    last_active_at_unix_ms: i64,
    logs: VecDeque<TerminalLog>,
    next_log_offset: u64,
    retained_log_bytes: usize,
    logs_truncated: bool,
    output_truncated: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ProcessStatus {
    Running,
    Exited,
}

#[derive(Clone)]
pub(super) struct TerminalLog {
    pub(super) offset: u64,
    pub(super) kind: &'static str,
    pub(super) content: String,
    pub(super) created_at_unix_ms: i64,
}

pub(super) struct ProcessSnapshot {
    pub(super) id: String,
    pub(super) command: String,
    pub(super) cwd: PathBuf,
    pub(super) project_root: PathBuf,
    pub(super) status: &'static str,
    pub(super) exit_code: Option<i32>,
    pub(super) started_at_unix_ms: i64,
    pub(super) last_active_at_unix_ms: i64,
    pub(super) logs: Vec<TerminalLog>,
    pub(super) next_log_offset: u64,
    pub(super) logs_truncated: bool,
    pub(super) output_truncated: bool,
}

pub(super) async fn spawn_process(
    scope: &TerminalScope,
    command: String,
    cwd: PathBuf,
    sequence: u64,
    #[cfg(windows)] sandbox: Option<Arc<crate::windows_appcontainer::WorkspaceLeaseManager>>,
) -> Result<Arc<ManagedProcess>, TerminalRuntimeError> {
    #[cfg(windows)]
    if let Some(sandbox) = sandbox {
        return spawn_sandboxed_process(scope, command, cwd, sequence, sandbox).await;
    }
    let mut shell = shell_command(&command, &cwd);
    let mut child = shell.spawn().map_err(|_| {
        TerminalRuntimeError::pre_effect("The terminal command could not be started.")
    })?;
    let termination = ProcessTermination::attach(&child).map_err(|_| {
        let _ = child.start_kill();
        TerminalRuntimeError::uncertain("The terminal process could not be isolated.")
    })?;
    let stdin = child.stdin.take().map(ManagedStdin::Tokio);
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let now = now_unix_ms();
    let process = Arc::new(ManagedProcess {
        id: Uuid::new_v4().to_string(),
        sequence,
        command: command.clone(),
        cwd,
        scope: scope.clone(),
        termination,
        state: Mutex::new(ProcessState {
            status: ProcessStatus::Running,
            exit_code: None,
            stdin,
            started_at_unix_ms: now,
            last_active_at_unix_ms: now,
            logs: VecDeque::new(),
            next_log_offset: 0,
            retained_log_bytes: 0,
            logs_truncated: false,
            output_truncated: false,
        }),
        changed: Notify::new(),
    });
    process
        .append("command", format!("{command}\n").into_bytes())
        .await;
    if let Some(stdout) = stdout {
        tokio::spawn(read_pipe(process.clone(), "stdout", stdout));
    }
    if let Some(stderr) = stderr {
        tokio::spawn(read_pipe(process.clone(), "stderr", stderr));
    }
    tokio::spawn(wait_for_child(process.clone(), child));
    Ok(process)
}

#[cfg(windows)]
async fn spawn_sandboxed_process(
    scope: &TerminalScope,
    command: String,
    cwd: PathBuf,
    sequence: u64,
    sandbox: Arc<crate::windows_appcontainer::WorkspaceLeaseManager>,
) -> Result<Arc<ManagedProcess>, TerminalRuntimeError> {
    use crate::windows_appcontainer::{restricted_runtime_environment, WorkspaceAccess};
    use std::sync::Mutex as StdMutex;

    let system_root = std::env::var_os("SystemRoot").ok_or_else(|| {
        TerminalRuntimeError::pre_effect("The Windows runtime environment is unavailable.")
    })?;
    let executable = PathBuf::from(system_root)
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    if !executable.is_file() {
        return Err(TerminalRuntimeError::pre_effect(
            "The Windows PowerShell runtime is unavailable.",
        ));
    }
    let arguments = vec![
        "-NoLogo".to_owned(),
        "-NoProfile".to_owned(),
        "-Command".to_owned(),
        command.clone(),
    ];
    let spawn_cwd = cwd.clone();
    let spawn_executable = executable.clone();
    let spawn_arguments = arguments.clone();
    let spawn_environment = restricted_runtime_environment();
    let mut child = tokio::task::spawn_blocking(move || {
        sandbox.spawn_workspace_command(
            &spawn_cwd,
            &spawn_executable,
            &spawn_arguments,
            WorkspaceAccess::ReadWrite,
            "disabled",
            &spawn_environment,
        )
    })
    .await
    .map_err(|_| {
        TerminalRuntimeError::pre_effect(
            "The sandboxed terminal launcher exited before it could start the command.",
        )
    })?
    .map_err(|error| {
        TerminalRuntimeError::pre_effect(format!(
            "The sandboxed terminal command could not be started: {error}"
        ))
    })?;
    let stdin = child
        .take_stdin()
        .map(|file| ManagedStdin::Sandbox(Arc::new(StdMutex::new(file))));
    let stdout = match child.take_stdout().map(blocking_reader) {
        Some(stdout) => stdout,
        None => {
            let _ = child.restore();
            return Err(TerminalRuntimeError::uncertain(
                "The sandboxed terminal output is unavailable.",
            ));
        }
    };
    let stderr = match child.take_stderr().map(blocking_reader) {
        Some(stderr) => stderr,
        None => {
            let _ = child.restore();
            return Err(TerminalRuntimeError::uncertain(
                "The sandboxed terminal error output is unavailable.",
            ));
        }
    };
    let child = Arc::new(StdMutex::new(Some(child)));
    let termination = ProcessTermination::attach_sandboxed(child.clone());
    let now = now_unix_ms();
    let process = Arc::new(ManagedProcess {
        id: Uuid::new_v4().to_string(),
        sequence,
        command: command.clone(),
        cwd,
        scope: scope.clone(),
        termination,
        state: Mutex::new(ProcessState {
            status: ProcessStatus::Running,
            exit_code: None,
            stdin,
            started_at_unix_ms: now,
            last_active_at_unix_ms: now,
            logs: VecDeque::new(),
            next_log_offset: 0,
            retained_log_bytes: 0,
            logs_truncated: false,
            output_truncated: false,
        }),
        changed: Notify::new(),
    });
    process
        .append("command", format!("{command}\n").into_bytes())
        .await;
    tokio::spawn(read_pipe(process.clone(), "stdout", stdout));
    tokio::spawn(read_pipe(process.clone(), "stderr", stderr));
    tokio::spawn(wait_for_sandboxed_child(process.clone(), child));
    Ok(process)
}

#[cfg(windows)]
fn blocking_reader(file: std::fs::File) -> BlockingReader {
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
    tokio::task::spawn_blocking(move || {
        let mut file = file;
        let mut buffer = vec![0_u8; 8 * 1_024];
        loop {
            match file.read(&mut buffer) {
                Ok(0) => return,
                Ok(count) => {
                    if sender.send(Ok(buffer[..count].to_vec())).is_err() {
                        return;
                    }
                }
                Err(error) => {
                    let _ = sender.send(Err(error));
                    return;
                }
            }
        }
    });
    BlockingReader {
        receiver,
        pending: None,
    }
}

#[cfg(windows)]
struct BlockingReader {
    receiver: tokio::sync::mpsc::UnboundedReceiver<io::Result<Vec<u8>>>,
    pending: Option<(Vec<u8>, usize)>,
}

#[cfg(windows)]
impl AsyncRead for BlockingReader {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        destination: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        loop {
            if let Some((bytes, offset)) = self.pending.as_mut() {
                let remaining = &bytes[*offset..];
                if remaining.is_empty() {
                    self.pending = None;
                    continue;
                }
                let count = remaining.len().min(destination.remaining());
                destination.put_slice(&remaining[..count]);
                *offset += count;
                if *offset >= bytes.len() {
                    self.pending = None;
                }
                return std::task::Poll::Ready(Ok(()));
            }
            match std::pin::Pin::new(&mut self.receiver).poll_recv(cx) {
                std::task::Poll::Ready(Some(Ok(bytes))) => self.pending = Some((bytes, 0)),
                std::task::Poll::Ready(Some(Err(error))) => {
                    return std::task::Poll::Ready(Err(error));
                }
                std::task::Poll::Ready(None) => return std::task::Poll::Ready(Ok(())),
                std::task::Poll::Pending => return std::task::Poll::Pending,
            }
        }
    }
}

impl ManagedProcess {
    pub(super) fn id(&self) -> &str {
        &self.id
    }

    pub(super) fn sequence(&self) -> u64 {
        self.sequence
    }

    pub(super) fn run_id(&self) -> &str {
        &self.scope.run_id
    }

    pub(super) fn belongs_to(&self, scope: &TerminalScope) -> bool {
        self.scope.run_id == scope.run_id
            && self.scope.workspace_scope_id == scope.workspace_scope_id
            && super::same_root(&self.scope.project_root, &scope.project_root)
    }

    pub(super) async fn snapshot(&self) -> ProcessSnapshot {
        let state = self.state.lock().await;
        ProcessSnapshot {
            id: self.id.clone(),
            command: self.command.clone(),
            cwd: self.cwd.clone(),
            project_root: self.scope.project_root.clone(),
            status: if state.status == ProcessStatus::Running {
                "running"
            } else {
                "exited"
            },
            exit_code: state.exit_code,
            started_at_unix_ms: state.started_at_unix_ms,
            last_active_at_unix_ms: state.last_active_at_unix_ms,
            logs: state.logs.iter().cloned().collect(),
            next_log_offset: state.next_log_offset,
            logs_truncated: state.logs_truncated,
            output_truncated: state.output_truncated,
        }
    }

    pub(super) async fn is_exited(&self) -> bool {
        self.state.lock().await.status == ProcessStatus::Exited
    }

    pub(super) async fn retained_log_bytes(&self) -> usize {
        self.state.lock().await.retained_log_bytes
    }

    pub(super) async fn wait(&self, timeout_ms: u64) -> bool {
        let wait = async {
            loop {
                let changed = self.changed.notified();
                if self.is_exited().await {
                    return;
                }
                changed.await;
            }
        };
        timeout(Duration::from_millis(timeout_ms), wait)
            .await
            .is_err()
    }

    pub(super) async fn write(
        &self,
        data: &str,
        submit: bool,
    ) -> Result<serde_json::Value, TerminalRuntimeError> {
        let mut bytes = data.as_bytes().to_vec();
        if submit {
            bytes.push(b'\n');
        }
        if bytes.len() > MAXIMUM_WRITE_BYTES {
            return Err(TerminalRuntimeError::pre_effect(
                "The local terminal field is invalid: data.",
            ));
        }
        let mut state = self.state.lock().await;
        if state.status == ProcessStatus::Exited {
            return Err(TerminalRuntimeError::pre_effect(
                "The terminal process has already exited.",
            ));
        }
        let stdin = state.stdin.as_mut().ok_or_else(|| {
            TerminalRuntimeError::pre_effect("The terminal process input is unavailable.")
        })?;
        stdin.write_all(&bytes).await.map_err(|_| {
            TerminalRuntimeError::uncertain("The terminal process input may not have been written.")
        })?;
        state.last_active_at_unix_ms = now_unix_ms();
        drop(state);
        self.append("input", bytes.clone()).await;
        Ok(json!({
            "ok": true,
            "terminal_id": self.id,
            "bytes_written": bytes.len(),
            "submit": submit
        }))
    }

    pub(super) async fn terminate_and_wait(
        &self,
        reason: &str,
    ) -> Result<(), TerminalRuntimeError> {
        if self.is_exited().await {
            return Ok(());
        }
        self.append("system", format!("[{reason}]\n").into_bytes())
            .await;
        let _ = self.termination.terminate(false);
        if self.wait(500).await {
            let _ = self.termination.terminate(true);
            if self.wait(1_000).await {
                return Err(TerminalRuntimeError::uncertain(
                    "The terminal process could not be confirmed stopped.",
                ));
            }
        }
        Ok(())
    }

    pub(super) fn terminate_now(&self) {
        let _ = self.termination.terminate(true);
    }

    async fn append(&self, kind: &'static str, mut bytes: Vec<u8>) {
        if bytes.is_empty() {
            return;
        }
        let mut state = self.state.lock().await;
        if bytes.len() > MAXIMUM_LOG_ENTRY_BYTES {
            bytes = bytes.split_off(bytes.len() - MAXIMUM_LOG_ENTRY_BYTES);
            state.logs_truncated = true;
            state.output_truncated = true;
        }
        let content = String::from_utf8_lossy(&bytes).into_owned();
        let byte_count = content.len();
        let offset = state.next_log_offset;
        state.next_log_offset = state.next_log_offset.saturating_add(1);
        state.logs.push_back(TerminalLog {
            offset,
            kind,
            content,
            created_at_unix_ms: now_unix_ms(),
        });
        state.retained_log_bytes = state.retained_log_bytes.saturating_add(byte_count);
        while state.logs.len() > 1
            && (state.logs.len() > MAXIMUM_LOGS || state.retained_log_bytes > MAXIMUM_LOG_BYTES)
        {
            if state.logs.len() <= RETAINED_LOG_TARGET
                && state.retained_log_bytes <= RETAINED_LOG_BYTES_TARGET
            {
                break;
            }
            if let Some(removed) = state.logs.pop_front() {
                state.retained_log_bytes = state
                    .retained_log_bytes
                    .saturating_sub(removed.content.len());
                state.logs_truncated = true;
            }
        }
        state.last_active_at_unix_ms = now_unix_ms();
        drop(state);
        self.changed.notify_waiters();
    }

    async fn finish(&self, exit_code: Option<i32>) {
        let mut state = self.state.lock().await;
        state.status = ProcessStatus::Exited;
        state.exit_code = exit_code;
        state.stdin = None;
        state.last_active_at_unix_ms = now_unix_ms();
        drop(state);
        self.changed.notify_waiters();
    }
}

async fn read_pipe(
    process: Arc<ManagedProcess>,
    kind: &'static str,
    mut pipe: impl AsyncRead + Unpin,
) {
    let mut buffer = vec![0_u8; 8 * 1_024];
    loop {
        match pipe.read(&mut buffer).await {
            Ok(0) => return,
            Ok(count) => process.append(kind, buffer[..count].to_vec()).await,
            Err(_) => {
                let mut state = process.state.lock().await;
                state.logs_truncated = true;
                state.output_truncated = true;
                return;
            }
        }
    }
}

async fn wait_for_child(process: Arc<ManagedProcess>, mut child: Child) {
    let exit_code = child.wait().await.ok().and_then(|status| status.code());
    process.finish(exit_code).await;
    sleep(Duration::from_secs(1)).await;
    let _ = process.termination.terminate(true);
}

#[cfg(windows)]
async fn wait_for_sandboxed_child(
    process: Arc<ManagedProcess>,
    child: Arc<std::sync::Mutex<Option<SandboxedChild>>>,
) {
    let exit_code = tokio::task::spawn_blocking(move || loop {
        let finished = child
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(SandboxedChild::try_wait));
        match finished {
            Some(Ok(Some(code))) => {
                let restored = child
                    .lock()
                    .ok()
                    .and_then(|mut guard| guard.take())
                    .map(SandboxedChild::restore)
                    .transpose();
                return match restored {
                    Ok(Some(())) | Ok(None) => Some(code),
                    Err(_) => None,
                };
            }
            Some(Ok(None)) => std::thread::sleep(std::time::Duration::from_millis(50)),
            Some(Err(_)) | None => return None,
        }
    })
    .await
    .ok()
    .flatten();
    process.finish(exit_code).await;
    sleep(Duration::from_secs(1)).await;
    let _ = process.termination.terminate(true);
}

enum ManagedStdin {
    Tokio(ChildStdin),
    #[cfg(windows)]
    Sandbox(Arc<std::sync::Mutex<std::fs::File>>),
}

impl ManagedStdin {
    async fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        match self {
            Self::Tokio(stdin) => stdin.write_all(bytes).await,
            #[cfg(windows)]
            Self::Sandbox(file) => {
                let file = file.clone();
                let bytes = bytes.to_vec();
                tokio::task::spawn_blocking(move || {
                    let mut file = file
                        .lock()
                        .map_err(|_| io::Error::other("sandboxed stdin lock poisoned"))?;
                    file.write_all(&bytes)
                })
                .await
                .map_err(|_| io::Error::other("sandboxed stdin worker stopped"))?
            }
        }
    }
}

fn shell_command(command: &str, cwd: &Path) -> Command {
    #[cfg(windows)]
    let mut shell = {
        let mut shell = Command::new("powershell.exe");
        shell.args(["-NoLogo", "-NoProfile", "-Command", command]);
        shell
    };
    #[cfg(not(windows))]
    let mut shell = {
        use std::os::unix::process::CommandExt;
        let mut shell = Command::new("/bin/zsh");
        shell.args(["-lc", command]);
        shell.as_std_mut().process_group(0);
        shell
    };
    shell
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(false);
    shell
}

pub(super) fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

pub(crate) enum ProcessTermination {
    #[cfg(windows)]
    WindowsJob(std::os::windows::io::OwnedHandle),
    #[cfg(windows)]
    AppContainer(std::sync::Arc<std::sync::Mutex<Option<SandboxedChild>>>),
    #[cfg(not(windows))]
    UnixProcessGroup(i32),
}

impl ProcessTermination {
    #[cfg(windows)]
    pub(crate) fn attach_sandboxed(
        child: std::sync::Arc<std::sync::Mutex<Option<SandboxedChild>>>,
    ) -> Self {
        Self::AppContainer(child)
    }

    pub(crate) fn attach(child: &Child) -> io::Result<Self> {
        #[cfg(windows)]
        {
            let raw = child
                .raw_handle()
                .ok_or_else(|| io::Error::other("missing child process handle"))?;
            Self::attach_raw_handle(raw)
        }
        #[cfg(not(windows))]
        {
            let pid = child
                .id()
                .ok_or_else(|| io::Error::other("missing child process id"))?;
            Ok(Self::UnixProcessGroup(pid as i32))
        }
    }

    #[cfg(windows)]
    pub(crate) fn attach_raw_handle(raw: std::os::windows::io::RawHandle) -> io::Result<Self> {
        use std::os::windows::io::{FromRawHandle, RawHandle};
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return Err(io::Error::last_os_error());
        }
        let owned = unsafe { std::os::windows::io::OwnedHandle::from_raw_handle(job as RawHandle) };
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
        let assigned = unsafe { AssignProcessToJobObject(job, raw as _) };
        if assigned == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self::WindowsJob(owned))
    }

    pub(crate) fn terminate(&self, force: bool) -> io::Result<()> {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::System::JobObjects::TerminateJobObject;
            let _ = force;
            let result = match self {
                Self::WindowsJob(job) => unsafe { TerminateJobObject(job.as_raw_handle() as _, 1) },
                Self::AppContainer(child) => child
                    .lock()
                    .map_err(|_| io::Error::other("sandboxed process lock poisoned"))?
                    .as_ref()
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::NotFound, "sandboxed process exited")
                    })
                    .and_then(|child| child.terminate().map_err(io::Error::other))
                    .map(|_| 1)
                    .unwrap_or(0),
            };
            if result == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
        #[cfg(not(windows))]
        {
            let Self::UnixProcessGroup(group) = self;
            let signal = if force { libc::SIGKILL } else { libc::SIGTERM };
            let result = unsafe { libc::kill(-*group, signal) };
            if result == 0 {
                return Ok(());
            }
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::ESRCH) {
                Ok(())
            } else {
                Err(error)
            }
        }
    }
}

impl Drop for ProcessTermination {
    fn drop(&mut self) {
        let _ = self.terminate(true);
    }
}
