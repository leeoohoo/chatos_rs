use super::models::{
    ProjectRunChangeSnapshot, ProjectRunInstance, ProjectRunState, ResolvedProjectRunTarget,
};
use crate::machine_resource_governor::{
    MachineResourceGovernor, MachineResourceKind, MachineResourcePermit, MachineResourcePriority,
};
use crate::terminal_runtime::process::ProcessTermination;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use tokio::{
    sync::{Mutex, Notify},
    time::{timeout, Duration},
};

#[path = "runtime_process.rs"]
mod process;
use process::{spawn_project_run, terminate_all};

const MAXIMUM_LOG_BYTES: usize = 512 * 1_024;
const MAXIMUM_RETAINED_EXITED_INSTANCES: usize = 100;
const MAXIMUM_CHANGE_WAIT_MS: u64 = 30_000;
const TERMINATION_GRACE_MS: u64 = 750;
const FORCE_TERMINATION_WAIT_MS: u64 = 1_500;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ProjectKey {
    owner_user_id: String,
    project_id: String,
}

#[derive(Clone, Debug)]
pub(crate) struct ProjectRunScope {
    pub(crate) owner_user_id: String,
    pub(crate) project_id: String,
    pub(crate) project_root: PathBuf,
}

impl ProjectRunScope {
    fn key(&self) -> ProjectKey {
        ProjectKey {
            owner_user_id: self.owner_user_id.clone(),
            project_id: self.project_id.clone(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct ProjectRunManager {
    store: Arc<Mutex<ProjectRunStore>>,
    resources: MachineResourceGovernor,
}

impl Default for ProjectRunManager {
    fn default() -> Self {
        Self::new_with_resources(MachineResourceGovernor::default())
    }
}

impl ProjectRunManager {
    pub(crate) fn new_with_resources(resources: MachineResourceGovernor) -> Self {
        Self {
            store: Arc::new(Mutex::new(ProjectRunStore::default())),
            resources,
        }
    }

    #[cfg(test)]
    pub(crate) fn used_resource_units(&self) -> usize {
        self.resources.used_resource_units()
    }
}

#[derive(Default)]
struct ProjectRunStore {
    instances: HashMap<String, Arc<ManagedProjectRun>>,
    changes: HashMap<ProjectKey, Arc<ProjectChangeSignal>>,
    next_sequence: u64,
}

#[derive(Default)]
struct ProjectChangeSignal {
    revision: AtomicU64,
    notify: Notify,
}

impl ProjectChangeSignal {
    fn current(&self) -> u64 {
        self.revision.load(Ordering::SeqCst)
    }

    fn bump(&self) {
        self.revision.fetch_add(1, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    async fn wait_after(&self, revision: u64, timeout_ms: u64) -> bool {
        if self.current() != revision {
            return false;
        }
        let changed = async {
            loop {
                let notified = self.notify.notified();
                if self.current() != revision {
                    return;
                }
                notified.await;
            }
        };
        timeout(Duration::from_millis(timeout_ms), changed)
            .await
            .is_err()
    }
}

struct ManagedProjectRun {
    id: String,
    sequence: u64,
    owner_user_id: String,
    project_id: String,
    project_root: PathBuf,
    target_id: String,
    name: String,
    cwd: String,
    redactions: Vec<String>,
    termination: ProcessTermination,
    state: Mutex<ManagedProjectRunState>,
    changed: Arc<ProjectChangeSignal>,
    finished: Notify,
    resource_permit: Mutex<Option<MachineResourcePermit>>,
}

struct ManagedProjectRunState {
    status: ManagedStatus,
    stop_requested: bool,
    log: Vec<u8>,
    log_truncated: bool,
    started_at_unix_ms: i64,
    exit_code: Option<i32>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ManagedStatus {
    Running,
    Stopping,
    Exited,
    Failed,
}

impl ProjectRunManager {
    pub(crate) async fn start(
        &self,
        scope: &ProjectRunScope,
        target: ResolvedProjectRunTarget,
    ) -> Result<ProjectRunState, String> {
        self.reconcile_scope(scope).await;
        self.prune_exited().await;
        let (sequence, changed) = {
            let mut store = self.store.lock().await;
            let sequence = store.next_sequence;
            store.next_sequence = store.next_sequence.saturating_add(1);
            let changed = store.changes.entry(scope.key()).or_default().clone();
            (sequence, changed)
        };
        let permit = self
            .resources
            .acquire(
                MachineResourceKind::CodeReadWorker,
                MachineResourcePriority::Interactive,
            )
            .await?;
        let process = spawn_project_run(scope, target, sequence, changed.clone(), permit).await?;
        {
            let mut store = self.store.lock().await;
            store.instances.insert(process.id.clone(), process.clone());
        }
        changed.bump();
        self.snapshot(scope).await
    }

    pub(crate) async fn snapshot(
        &self,
        scope: &ProjectRunScope,
    ) -> Result<ProjectRunState, String> {
        self.reconcile_scope(scope).await;
        self.prune_exited().await;
        Ok(self.snapshot_current(scope).await)
    }

    pub(crate) async fn poll_changes(
        &self,
        scope: &ProjectRunScope,
        after_revision: u64,
        timeout_ms: u64,
    ) -> Result<ProjectRunChangeSnapshot, String> {
        self.reconcile_scope(scope).await;
        let changed = self.change_signal(scope).await;
        let timed_out = changed
            .wait_after(after_revision, timeout_ms.min(MAXIMUM_CHANGE_WAIT_MS))
            .await;
        Ok(ProjectRunChangeSnapshot {
            timed_out,
            state: self.snapshot_current(scope).await,
        })
    }

    pub(crate) async fn stop(
        &self,
        scope: &ProjectRunScope,
        instance_id: &str,
    ) -> Result<ProjectRunState, String> {
        self.reconcile_scope(scope).await;
        let process = self.instance(scope, instance_id).await?;
        process.stop("项目运行已停止").await?;
        self.snapshot(scope).await
    }

    pub(crate) async fn delete(
        &self,
        scope: &ProjectRunScope,
        instance_id: &str,
    ) -> Result<ProjectRunState, String> {
        self.reconcile_scope(scope).await;
        let process = self.instance(scope, instance_id).await?;
        {
            let mut store = self.store.lock().await;
            store.instances.remove(instance_id);
        }
        process.changed.bump();
        process.stop("项目运行记录已删除").await?;
        self.snapshot(scope).await
    }

    pub(crate) async fn retain_owner(&self, owner_user_id: &str) {
        let (processes, retired_signals) = {
            let mut store = self.store.lock().await;
            let ids = store
                .instances
                .iter()
                .filter(|(_, process)| process.owner_user_id != owner_user_id)
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            let processes = ids
                .into_iter()
                .filter_map(|id| store.instances.remove(&id))
                .collect::<Vec<_>>();
            let keys = store
                .changes
                .keys()
                .filter(|key| key.owner_user_id != owner_user_id)
                .cloned()
                .collect::<Vec<_>>();
            let signals = keys
                .into_iter()
                .filter_map(|key| store.changes.remove(&key))
                .collect::<Vec<_>>();
            (processes, signals)
        };
        for signal in retired_signals {
            signal.bump();
        }
        terminate_all(processes, "登录账号已切换").await;
    }

    pub(crate) async fn release_project(&self, owner_user_id: &str, project_id: &str) {
        let (processes, signal) = {
            let mut store = self.store.lock().await;
            let ids = store
                .instances
                .iter()
                .filter(|(_, process)| {
                    process.owner_user_id == owner_user_id && process.project_id == project_id
                })
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            let processes = ids
                .into_iter()
                .filter_map(|id| store.instances.remove(&id))
                .collect::<Vec<_>>();
            let signal = store.changes.remove(&ProjectKey {
                owner_user_id: owner_user_id.to_owned(),
                project_id: project_id.to_owned(),
            });
            (processes, signal)
        };
        if let Some(signal) = signal {
            signal.bump();
        }
        terminate_all(processes, "项目已移除").await;
    }

    pub(crate) async fn reset(&self) {
        let (processes, signals) = {
            let mut store = self.store.lock().await;
            let processes = store
                .instances
                .drain()
                .map(|(_, process)| process)
                .collect::<Vec<_>>();
            let signals = store
                .changes
                .drain()
                .map(|(_, signal)| signal)
                .collect::<Vec<_>>();
            (processes, signals)
        };
        for signal in signals {
            signal.bump();
        }
        terminate_all(processes, "应用正在退出").await;
    }

    async fn snapshot_current(&self, scope: &ProjectRunScope) -> ProjectRunState {
        let (processes, changed) = {
            let mut store = self.store.lock().await;
            let changed = store.changes.entry(scope.key()).or_default().clone();
            let processes = store
                .instances
                .values()
                .filter(|process| process.belongs_to(scope))
                .cloned()
                .collect::<Vec<_>>();
            (processes, changed)
        };
        let revision = changed.current();
        let mut instances = Vec::with_capacity(processes.len());
        for process in processes {
            instances.push(process.snapshot().await);
        }
        instances.sort_by(|left, right| {
            right
                .started_at_unix_ms
                .cmp(&left.started_at_unix_ms)
                .then_with(|| right.id.cmp(&left.id))
        });
        let is_running = instances.iter().any(|instance| instance.is_running);
        let is_busy = instances.iter().any(|instance| instance.is_busy);
        ProjectRunState {
            project_id: scope.project_id.clone(),
            status: if is_running { "running" } else { "idle" },
            is_busy,
            is_running,
            revision,
            instances,
        }
    }

    async fn instance(
        &self,
        scope: &ProjectRunScope,
        instance_id: &str,
    ) -> Result<Arc<ManagedProjectRun>, String> {
        if instance_id.is_empty()
            || instance_id.trim() != instance_id
            || instance_id.len() > 160
            || instance_id.chars().any(char::is_control)
        {
            return Err("项目运行实例字段无效。".into());
        }
        self.store
            .lock()
            .await
            .instances
            .get(instance_id)
            .filter(|process| process.belongs_to(scope))
            .cloned()
            .ok_or_else(|| "项目运行实例不存在。".to_owned())
    }

    async fn change_signal(&self, scope: &ProjectRunScope) -> Arc<ProjectChangeSignal> {
        self.store
            .lock()
            .await
            .changes
            .entry(scope.key())
            .or_default()
            .clone()
    }

    async fn reconcile_scope(&self, scope: &ProjectRunScope) {
        let stale = {
            let mut store = self.store.lock().await;
            let ids = store
                .instances
                .iter()
                .filter(|(_, process)| {
                    process.owner_user_id == scope.owner_user_id
                        && process.project_id == scope.project_id
                        && process.project_root != scope.project_root
                })
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            ids.into_iter()
                .filter_map(|id| store.instances.remove(&id))
                .collect::<Vec<_>>()
        };
        terminate_all(stale, "项目目录授权已变化").await;
    }

    async fn prune_exited(&self) {
        let processes = {
            self.store
                .lock()
                .await
                .instances
                .values()
                .cloned()
                .collect::<Vec<_>>()
        };
        let mut exited = Vec::new();
        for process in processes {
            if process.is_finished().await {
                exited.push((process.sequence, process.id.clone()));
            }
        }
        exited.sort_by_key(|(sequence, _)| std::cmp::Reverse(*sequence));
        if exited.len() <= MAXIMUM_RETAINED_EXITED_INSTANCES {
            return;
        }
        let mut store = self.store.lock().await;
        for (_, id) in exited.into_iter().skip(MAXIMUM_RETAINED_EXITED_INSTANCES) {
            store.instances.remove(&id);
        }
    }
}

impl ManagedProjectRun {
    fn belongs_to(&self, scope: &ProjectRunScope) -> bool {
        self.owner_user_id == scope.owner_user_id
            && self.project_id == scope.project_id
            && self.project_root == scope.project_root
    }

    async fn snapshot(&self) -> ProjectRunInstance {
        let state = self.state.lock().await;
        let mut log = String::from_utf8_lossy(&state.log).into_owned();
        for value in &self.redactions {
            log = log.replace(value, "[已隐藏]");
        }
        ProjectRunInstance {
            id: self.id.clone(),
            target_id: self.target_id.clone(),
            name: self.name.clone(),
            cwd: self.cwd.clone(),
            status: match state.status {
                ManagedStatus::Running => "running",
                ManagedStatus::Stopping => "stopping",
                ManagedStatus::Exited => "exited",
                ManagedStatus::Failed => "failed",
            },
            is_busy: state.status == ManagedStatus::Stopping,
            is_running: matches!(
                state.status,
                ManagedStatus::Running | ManagedStatus::Stopping
            ),
            log,
            log_truncated: state.log_truncated,
            started_at_unix_ms: state.started_at_unix_ms,
            exit_code: state.exit_code,
        }
    }

    async fn append(&self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let mut state = self.state.lock().await;
        if bytes.len() >= MAXIMUM_LOG_BYTES {
            state.log.clear();
            state
                .log
                .extend_from_slice(&bytes[bytes.len() - MAXIMUM_LOG_BYTES..]);
            state.log_truncated = true;
        } else {
            state.log.extend_from_slice(bytes);
            if state.log.len() > MAXIMUM_LOG_BYTES {
                let overflow = state.log.len() - MAXIMUM_LOG_BYTES;
                state.log.drain(..overflow);
                state.log_truncated = true;
            }
        }
        drop(state);
        self.changed.bump();
    }

    async fn finish(&self, exit_code: Option<i32>) {
        let mut state = self.state.lock().await;
        state.exit_code = exit_code;
        // A user-requested stop is a normal terminal outcome even when the
        // wrapped launcher reports a non-zero termination code (for example
        // npm/cmd commonly returns 1 after its process tree is terminated).
        // Preserve the exit code for diagnostics, but do not surface this as
        // a failed run in the UI.
        state.status = if state.stop_requested || exit_code == Some(0) {
            ManagedStatus::Exited
        } else {
            ManagedStatus::Failed
        };
        drop(state);
        self.resource_permit.lock().await.take();
        self.changed.bump();
        self.finished.notify_waiters();
    }

    async fn is_finished(&self) -> bool {
        matches!(
            self.state.lock().await.status,
            ManagedStatus::Exited | ManagedStatus::Failed
        )
    }

    async fn wait_finished(&self, duration: Duration) -> bool {
        let wait = async {
            loop {
                let notified = self.finished.notified();
                if self.is_finished().await {
                    return;
                }
                notified.await;
            }
        };
        timeout(duration, wait).await.is_ok()
    }

    async fn stop(&self, reason: &str) -> Result<(), String> {
        {
            let mut state = self.state.lock().await;
            if matches!(state.status, ManagedStatus::Exited | ManagedStatus::Failed) {
                return Ok(());
            }
            state.stop_requested = true;
            state.status = ManagedStatus::Stopping;
        }
        self.append(format!("\n[{reason}]\n").as_bytes()).await;
        let _ = self.termination.terminate(false);
        if self
            .wait_finished(Duration::from_millis(TERMINATION_GRACE_MS))
            .await
        {
            return Ok(());
        }
        let _ = self.termination.terminate(true);
        if self
            .wait_finished(Duration::from_millis(FORCE_TERMINATION_WAIT_MS))
            .await
        {
            Ok(())
        } else {
            Err("项目进程无法确认已停止。".into())
        }
    }

    fn terminate_now(&self) {
        let _ = self.termination.terminate(true);
    }
}

#[cfg(test)]
pub(crate) fn test_scope(
    root: &std::path::Path,
    owner_user_id: &str,
    project_id: &str,
) -> ProjectRunScope {
    ProjectRunScope {
        owner_user_id: owner_user_id.into(),
        project_id: project_id.into(),
        project_root: root.to_path_buf(),
    }
}
