mod process;
mod projection;
#[cfg(test)]
mod tests;

use process::{spawn_process, ManagedProcess};
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::Mutex;

const DEFAULT_FOREGROUND_TIMEOUT_MS: u64 = 5 * 60 * 1_000;
const MINIMUM_FOREGROUND_TIMEOUT_MS: u64 = 1_000;
const MAXIMUM_FOREGROUND_TIMEOUT_MS: u64 = 15 * 60 * 1_000;
const MAXIMUM_WAIT_MS: u64 = 2 * 60 * 60 * 1_000;
const MAXIMUM_RETAINED_EXITED_PROCESSES: usize = 24;
const MAXIMUM_RETAINED_EXITED_LOG_BYTES: usize = 32 * 1_024 * 1_024;

#[derive(Clone)]
pub(crate) struct TerminalProcessStore {
    state: Arc<Mutex<StoreState>>,
}

#[derive(Default)]
struct StoreState {
    processes: HashMap<String, Arc<ManagedProcess>>,
    next_sequence: u64,
}

#[derive(Clone)]
pub(crate) struct TerminalScope {
    pub(crate) run_id: String,
    pub(crate) workspace_scope_id: String,
    pub(crate) project_root: PathBuf,
}

pub(crate) struct ExecuteRequest {
    pub(crate) command: String,
    pub(crate) cwd: PathBuf,
    pub(crate) background: bool,
    pub(crate) timeout_ms: Option<u64>,
}

#[derive(Debug)]
pub(crate) struct TerminalRuntimeError {
    pub(crate) message: String,
    pub(crate) known_pre_effect: bool,
}

impl TerminalRuntimeError {
    pub(crate) fn pre_effect(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            known_pre_effect: true,
        }
    }

    pub(crate) fn uncertain(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            known_pre_effect: false,
        }
    }
}

impl Default for TerminalProcessStore {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(StoreState::default())),
        }
    }
}

impl TerminalProcessStore {
    pub(crate) async fn execute(
        &self,
        scope: &TerminalScope,
        request: ExecuteRequest,
    ) -> Result<Value, TerminalRuntimeError> {
        let sequence = {
            let mut state = self.state.lock().await;
            let sequence = state.next_sequence;
            state.next_sequence = state.next_sequence.saturating_add(1);
            sequence
        };
        let process = spawn_process(scope, request.command, request.cwd, sequence).await?;
        let id = process.id().to_owned();
        {
            let mut state = self.state.lock().await;
            state.processes.insert(id, process.clone());
            prune_exited(&mut state).await;
        }
        if request.background {
            return Ok(projection::background_result(&process).await);
        }
        let mut cancellation_guard = ProcessCancellationGuard::new(process.clone());
        let timeout = request
            .timeout_ms
            .unwrap_or(DEFAULT_FOREGROUND_TIMEOUT_MS)
            .clamp(MINIMUM_FOREGROUND_TIMEOUT_MS, MAXIMUM_FOREGROUND_TIMEOUT_MS);
        let timed_out = process.wait(timeout).await;
        if timed_out {
            process
                .terminate_and_wait("foreground terminal timed out")
                .await?;
        }
        let result = projection::foreground_result(&process, timed_out, timeout).await;
        cancellation_guard.disarm();
        Ok(result)
    }

    pub(crate) async fn poll(
        &self,
        scope: &TerminalScope,
        terminal_id: &str,
        offset: Option<u64>,
        limit: usize,
    ) -> Result<Value, TerminalRuntimeError> {
        let process = self.process(scope, terminal_id).await?;
        Ok(projection::poll_result(&process, offset, limit.clamp(1, 200)).await)
    }

    pub(crate) async fn log(
        &self,
        scope: &TerminalScope,
        terminal_id: &str,
        offset: Option<u64>,
        limit: usize,
    ) -> Result<Value, TerminalRuntimeError> {
        let process = self.process(scope, terminal_id).await?;
        Ok(projection::log_result(&process, offset, limit.clamp(1, 200)).await)
    }

    pub(crate) async fn wait(
        &self,
        scope: &TerminalScope,
        terminal_id: &str,
        timeout_ms: u64,
    ) -> Result<Value, TerminalRuntimeError> {
        let process = self.process(scope, terminal_id).await?;
        let timeout = timeout_ms.clamp(1_000, MAXIMUM_WAIT_MS);
        let started = process::now_unix_ms();
        let timed_out = process.wait(timeout).await;
        Ok(projection::wait_result(
            &process,
            timed_out,
            timeout,
            process::now_unix_ms().saturating_sub(started),
        )
        .await)
    }

    pub(crate) async fn write(
        &self,
        scope: &TerminalScope,
        terminal_id: &str,
        data: &str,
        submit: bool,
    ) -> Result<Value, TerminalRuntimeError> {
        let process = self.process(scope, terminal_id).await?;
        process.write(data, submit).await
    }

    pub(crate) async fn kill(
        &self,
        scope: &TerminalScope,
        terminal_id: &str,
    ) -> Result<Value, TerminalRuntimeError> {
        let process = self.process(scope, terminal_id).await?;
        process.terminate_and_wait("terminal killed").await?;
        Ok(serde_json::json!({"ok": true, "terminal_id": terminal_id, "killed": true}))
    }

    pub(crate) async fn release_run(&self, run_id: &str) {
        let processes = {
            let state = self.state.lock().await;
            state
                .processes
                .values()
                .filter(|process| process.run_id() == run_id)
                .cloned()
                .collect::<Vec<_>>()
        };
        for process in processes {
            let _ = process
                .terminate_and_wait("terminal cancelled with executor run")
                .await;
        }
    }

    pub(crate) async fn reset(&self) {
        let processes = {
            let mut state = self.state.lock().await;
            state
                .processes
                .drain()
                .map(|(_, process)| process)
                .collect::<Vec<_>>()
        };
        for process in processes {
            let _ = process
                .terminate_and_wait("terminal cancelled with executor reset")
                .await;
        }
    }

    async fn process(
        &self,
        scope: &TerminalScope,
        terminal_id: &str,
    ) -> Result<Arc<ManagedProcess>, TerminalRuntimeError> {
        if terminal_id.trim() != terminal_id || terminal_id.is_empty() || terminal_id.len() > 160 {
            return Err(TerminalRuntimeError::pre_effect(
                "The local terminal field is invalid: terminal_id.",
            ));
        }
        let state = self.state.lock().await;
        state
            .processes
            .get(terminal_id)
            .filter(|process| process.belongs_to(scope))
            .cloned()
            .ok_or_else(|| {
                TerminalRuntimeError::pre_effect(
                    "The terminal process is unavailable for this Run.",
                )
            })
    }
}

async fn prune_exited(state: &mut StoreState) {
    let mut exited = Vec::new();
    for process in state.processes.values() {
        if process.is_exited().await {
            exited.push((
                process.sequence(),
                process.retained_log_bytes().await,
                process.id().to_owned(),
            ));
        }
    }
    exited.sort_by_key(|(sequence, _, _)| std::cmp::Reverse(*sequence));
    let mut retained_count: usize = 0;
    let mut retained_bytes: usize = 0;
    for (_, bytes, id) in exited {
        if retained_count < MAXIMUM_RETAINED_EXITED_PROCESSES
            && retained_bytes.saturating_add(bytes) <= MAXIMUM_RETAINED_EXITED_LOG_BYTES
        {
            retained_count += 1;
            retained_bytes = retained_bytes.saturating_add(bytes);
        } else {
            state.processes.remove(&id);
        }
    }
}

fn same_root(left: &Path, right: &Path) -> bool {
    left == right
}

struct ProcessCancellationGuard {
    process: Arc<ManagedProcess>,
    armed: bool,
}

impl ProcessCancellationGuard {
    fn new(process: Arc<ManagedProcess>) -> Self {
        Self {
            process,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for ProcessCancellationGuard {
    fn drop(&mut self) {
        if self.armed {
            self.process.terminate_now();
        }
    }
}
