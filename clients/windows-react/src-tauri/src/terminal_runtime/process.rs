use super::{TerminalRuntimeError, TerminalScope};
use serde_json::json;
use std::{
    collections::VecDeque,
    io,
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
    stdin: Option<ChildStdin>,
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
) -> Result<Arc<ManagedProcess>, TerminalRuntimeError> {
    let mut shell = shell_command(&command, &cwd);
    let mut child = shell.spawn().map_err(|_| {
        TerminalRuntimeError::pre_effect("The terminal command could not be started.")
    })?;
    let termination = ProcessTermination::attach(&child).map_err(|_| {
        let _ = child.start_kill();
        TerminalRuntimeError::uncertain("The terminal process could not be isolated.")
    })?;
    let stdin = child.stdin.take();
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
    #[cfg(not(windows))]
    UnixProcessGroup(i32),
}

impl ProcessTermination {
    pub(crate) fn attach(child: &Child) -> io::Result<Self> {
        #[cfg(windows)]
        {
            use std::os::windows::io::{FromRawHandle, RawHandle};
            use windows_sys::Win32::System::JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
                SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            };
            let raw = child
                .raw_handle()
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
            let assigned = unsafe { AssignProcessToJobObject(job, raw as _) };
            if configured == 0 || assigned == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self::WindowsJob(owned))
        }
        #[cfg(not(windows))]
        {
            let pid = child
                .id()
                .ok_or_else(|| io::Error::other("missing child process id"))?;
            Ok(Self::UnixProcessGroup(pid as i32))
        }
    }

    pub(crate) fn terminate(&self, force: bool) -> io::Result<()> {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::System::JobObjects::TerminateJobObject;
            let Self::WindowsJob(job) = self;
            let result = unsafe { TerminateJobObject(job.as_raw_handle() as _, 1) };
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
