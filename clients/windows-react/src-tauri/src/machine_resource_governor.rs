use chatos_local_agent_protocol::LocalAgentResourcePressureLevel;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, MutexGuard,
    },
};
use tokio::sync::oneshot;

const MINIMUM_FILE_WORKERS: usize = 3;
const MAXIMUM_FILE_WORKERS: usize = 10;
const MAXIMUM_QUEUED_WORK: usize = 256;
const INTERACTIVE_RESERVE_UNITS: usize = 2;
const MAXIMUM_INTERACTIVE_BURST: usize = 4;
const GIBIBYTE: u64 = 1_024 * 1_024 * 1_024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum MachineResourceKind {
    MediaWorker,
    CodeReadWorker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MachineResourcePriority {
    Background,
    Interactive,
}

#[derive(Debug, Clone, Copy)]
struct MachineResourceBudget {
    available_units: usize,
    maximum_media_workers: usize,
    maximum_code_read_workers: usize,
}

impl MachineResourceBudget {
    fn adaptive() -> Self {
        let logical_processors = std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(2);
        let physical_memory_bytes = physical_memory_bytes().unwrap_or(8 * GIBIBYTE);
        let logical_jobs = recommended_logical_jobs(logical_processors, physical_memory_bytes);
        let configured_host = std::env::var("CHATOS_LOCAL_AGENT_MAX_CONCURRENCY")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| (1..=16).contains(value));
        let host_workers = configured_host
            .unwrap_or_else(|| (logical_jobs / 4).max(1))
            .min(logical_jobs - 1);
        let file_workers = recommended_file_workers(logical_processors, physical_memory_bytes);
        Self {
            // Match macOS: the Host owns a hard reservation and the App pool receives the
            // remaining logical jobs at two units per Agent-sized job.
            available_units: (logical_jobs - host_workers) * 2,
            maximum_media_workers: 4.min(file_workers - 1),
            maximum_code_read_workers: 8.min(file_workers - 1),
        }
    }

    fn maximum_active(self, kind: MachineResourceKind) -> usize {
        match kind {
            MachineResourceKind::MediaWorker => self.maximum_media_workers,
            MachineResourceKind::CodeReadWorker => self.maximum_code_read_workers,
        }
    }
}

fn recommended_logical_jobs(logical_processors: usize, physical_memory_bytes: u64) -> usize {
    if let Some(configured) = std::env::var("CHATOS_BACKGROUND_JOB_BUDGET")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| (2..=16).contains(value))
    {
        return configured;
    }
    let processor_capacity = logical_processors.saturating_sub(2).max(2);
    let memory_capacity = physical_memory_bytes
        .saturating_sub(2 * GIBIBYTE)
        .checked_div(2 * GIBIBYTE)
        .unwrap_or_default()
        .clamp(2, 16) as usize;
    processor_capacity.min(memory_capacity).clamp(2, 16)
}

fn recommended_file_workers(logical_processors: usize, physical_memory_bytes: u64) -> usize {
    let processor_capacity = logical_processors
        .saturating_sub(2)
        .max(MINIMUM_FILE_WORKERS);
    let memory_capacity = physical_memory_bytes
        .saturating_sub(2 * GIBIBYTE)
        .checked_div(GIBIBYTE)
        .unwrap_or_default()
        .clamp(MINIMUM_FILE_WORKERS as u64, MAXIMUM_FILE_WORKERS as u64)
        as usize;
    processor_capacity
        .min(memory_capacity)
        .clamp(MINIMUM_FILE_WORKERS, MAXIMUM_FILE_WORKERS)
}

#[cfg(target_os = "windows")]
fn physical_memory_bytes() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    let mut memory = unsafe { std::mem::zeroed::<MEMORYSTATUSEX>() };
    memory.dwLength = u32::try_from(std::mem::size_of::<MEMORYSTATUSEX>()).ok()?;
    (unsafe { GlobalMemoryStatusEx(&mut memory) } != 0).then_some(memory.ullTotalPhys)
}

#[cfg(not(target_os = "windows"))]
fn physical_memory_bytes() -> Option<u64> {
    None
}

struct Waiter {
    id: u64,
    kind: MachineResourceKind,
    priority: MachineResourcePriority,
    sender: oneshot::Sender<()>,
}

struct GovernorState {
    active: HashMap<u64, (MachineResourceKind, MachineResourcePriority)>,
    waiters: Vec<Waiter>,
    pressure_level: LocalAgentResourcePressureLevel,
    effective_units: usize,
    maximum_background_units: usize,
    consecutive_interactive_admissions: usize,
}

struct GovernorInner {
    budget: MachineResourceBudget,
    maximum_queued: usize,
    next_id: AtomicU64,
    state: Mutex<GovernorState>,
}

#[derive(Clone)]
pub(crate) struct MachineResourceGovernor {
    inner: Arc<GovernorInner>,
}

impl Default for MachineResourceGovernor {
    fn default() -> Self {
        Self::new(MachineResourceBudget::adaptive(), MAXIMUM_QUEUED_WORK)
    }
}

impl MachineResourceGovernor {
    fn new(budget: MachineResourceBudget, maximum_queued: usize) -> Self {
        let (effective_units, maximum_background_units) = capacity(
            budget.available_units,
            LocalAgentResourcePressureLevel::Normal,
        );
        Self {
            inner: Arc::new(GovernorInner {
                budget,
                maximum_queued,
                next_id: AtomicU64::new(1),
                state: Mutex::new(GovernorState {
                    active: HashMap::new(),
                    waiters: Vec::new(),
                    pressure_level: LocalAgentResourcePressureLevel::Normal,
                    effective_units,
                    maximum_background_units,
                    consecutive_interactive_admissions: 0,
                }),
            }),
        }
    }

    pub(crate) async fn acquire(
        &self,
        kind: MachineResourceKind,
        priority: MachineResourcePriority,
    ) -> Result<MachineResourcePermit, String> {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let receiver = {
            let mut state = self.lock_state();
            if state.waiters.is_empty() && self.can_admit(&state, kind, priority) {
                state.active.insert(id, (kind, priority));
                return Ok(MachineResourcePermit {
                    governor: Some(self.clone()),
                    id,
                });
            }
            if state.waiters.len() >= self.inner.maximum_queued {
                return Err("本机后台任务队列已满，请稍后重试。".into());
            }
            let (sender, receiver) = oneshot::channel();
            state.waiters.push(Waiter {
                id,
                kind,
                priority,
                sender,
            });
            // A new waiter may be admissible even when an older waiter is blocked by a
            // per-kind reservation or background-pressure limit. Re-run the fair selector now
            // instead of waiting for an unrelated active permit to be released.
            self.drain_waiters(&mut state);
            receiver
        };

        let mut registration = QueuedRegistration {
            governor: Some(self.clone()),
            id,
        };
        receiver
            .await
            .map_err(|_| "本机后台任务准入已取消。".to_string())?;
        registration.governor = None;
        Ok(MachineResourcePermit {
            governor: Some(self.clone()),
            id,
        })
    }

    pub(crate) fn update_pressure(&self, level: LocalAgentResourcePressureLevel) {
        let mut state = self.lock_state();
        state.pressure_level = level;
        (state.effective_units, state.maximum_background_units) =
            capacity(self.inner.budget.available_units, level);
        self.drain_waiters(&mut state);
    }

    fn release_or_cancel(&self, id: u64) {
        let mut state = self.lock_state();
        if state.active.remove(&id).is_none() {
            if let Some(index) = state.waiters.iter().position(|waiter| waiter.id == id) {
                state.waiters.remove(index);
            }
        }
        self.drain_waiters(&mut state);
    }

    fn drain_waiters(&self, state: &mut GovernorState) {
        while let Some(index) = self.next_admissible_waiter(state) {
            let waiter = state.waiters.remove(index);
            state
                .active
                .insert(waiter.id, (waiter.kind, waiter.priority));
            if waiter.sender.send(()).is_err() {
                state.active.remove(&waiter.id);
            }
        }
    }

    fn next_admissible_waiter(&self, state: &mut GovernorState) -> Option<usize> {
        let mut interactive = None;
        let mut background = None;
        for (index, waiter) in state.waiters.iter().enumerate() {
            if !self.can_admit(state, waiter.kind, waiter.priority) {
                continue;
            }
            match waiter.priority {
                MachineResourcePriority::Interactive if interactive.is_none() => {
                    interactive = Some(index)
                }
                MachineResourcePriority::Background if background.is_none() => {
                    background = Some(index)
                }
                _ => {}
            }
        }
        if let Some(background) = background {
            if interactive.is_none()
                || state.consecutive_interactive_admissions >= MAXIMUM_INTERACTIVE_BURST
            {
                state.consecutive_interactive_admissions = 0;
                return Some(background);
            }
        }
        if let Some(interactive) = interactive {
            state.consecutive_interactive_admissions = if background.is_some() {
                state.consecutive_interactive_admissions.saturating_add(1)
            } else {
                0
            };
            return Some(interactive);
        }
        state.consecutive_interactive_admissions = 0;
        background
    }

    fn can_admit(
        &self,
        state: &GovernorState,
        kind: MachineResourceKind,
        priority: MachineResourcePriority,
    ) -> bool {
        if self.active_count(state, kind) >= self.inner.budget.maximum_active(kind)
            || state.active.len().saturating_add(1) > state.effective_units
        {
            return false;
        }
        if priority == MachineResourcePriority::Background {
            let background = state
                .active
                .values()
                .filter(|(_, active_priority)| {
                    *active_priority == MachineResourcePriority::Background
                })
                .count();
            if background.saturating_add(1) > state.maximum_background_units {
                return false;
            }
        }
        let other = match kind {
            MachineResourceKind::MediaWorker => MachineResourceKind::CodeReadWorker,
            MachineResourceKind::CodeReadWorker => MachineResourceKind::MediaWorker,
        };
        let reserve =
            usize::from(self.active_count(state, kind) > 0 && self.active_count(state, other) == 0);
        state.active.len().saturating_add(1) <= state.effective_units.saturating_sub(reserve)
    }

    fn active_count(&self, state: &GovernorState, kind: MachineResourceKind) -> usize {
        state
            .active
            .values()
            .filter(|(active_kind, _)| *active_kind == kind)
            .count()
    }

    fn lock_state(&self) -> MutexGuard<'_, GovernorState> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[cfg(test)]
    fn snapshot(&self) -> GovernorSnapshot {
        let state = self.lock_state();
        GovernorSnapshot {
            active: state.active.values().copied().collect(),
            queued: state
                .waiters
                .iter()
                .map(|waiter| (waiter.kind, waiter.priority))
                .collect(),
            pressure_level: state.pressure_level,
            effective_units: state.effective_units,
            maximum_background_units: state.maximum_background_units,
        }
    }
}

pub(crate) struct MachineResourcePermit {
    governor: Option<MachineResourceGovernor>,
    id: u64,
}

impl Drop for MachineResourcePermit {
    fn drop(&mut self) {
        if let Some(governor) = self.governor.take() {
            governor.release_or_cancel(self.id);
        }
    }
}

struct QueuedRegistration {
    governor: Option<MachineResourceGovernor>,
    id: u64,
}

impl Drop for QueuedRegistration {
    fn drop(&mut self) {
        if let Some(governor) = self.governor.take() {
            governor.release_or_cancel(self.id);
        }
    }
}

fn capacity(baseline: usize, pressure: LocalAgentResourcePressureLevel) -> (usize, usize) {
    let minimum = baseline.min(INTERACTIVE_RESERVE_UNITS);
    let effective = match pressure {
        LocalAgentResourcePressureLevel::Normal => baseline,
        LocalAgentResourcePressureLevel::Elevated => (baseline * 3).div_ceil(4).max(minimum),
        LocalAgentResourcePressureLevel::Serious => baseline.div_ceil(2).max(minimum),
        LocalAgentResourcePressureLevel::Critical => minimum,
    };
    let background = match pressure {
        LocalAgentResourcePressureLevel::Normal => baseline,
        _ => effective.saturating_sub(minimum),
    };
    (effective, background)
}

#[cfg(test)]
#[derive(Debug)]
struct GovernorSnapshot {
    active: Vec<(MachineResourceKind, MachineResourcePriority)>,
    queued: Vec<(MachineResourceKind, MachineResourcePriority)>,
    pressure_level: LocalAgentResourcePressureLevel,
    effective_units: usize,
    maximum_background_units: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn governor(units: usize, maximum_queued: usize) -> MachineResourceGovernor {
        MachineResourceGovernor::new(
            MachineResourceBudget {
                available_units: units,
                maximum_media_workers: units,
                maximum_code_read_workers: units,
            },
            maximum_queued,
        )
    }

    #[test]
    fn adaptive_budget_preserves_host_and_file_worker_headroom() {
        assert_eq!(recommended_logical_jobs(2, 2 * GIBIBYTE), 2);
        assert_eq!(recommended_logical_jobs(16, 32 * GIBIBYTE), 14);
        assert_eq!(recommended_file_workers(2, 2 * GIBIBYTE), 3);
        assert_eq!(recommended_file_workers(16, 32 * GIBIBYTE), 10);
        let budget = MachineResourceBudget::adaptive();
        assert!(budget.available_units >= 2);
        assert!((2..=4).contains(&budget.maximum_media_workers));
        assert!((2..=8).contains(&budget.maximum_code_read_workers));
    }

    #[tokio::test]
    async fn file_kinds_keep_one_cross_kind_reservation() {
        let governor = governor(3, 8);
        let first = governor
            .acquire(
                MachineResourceKind::MediaWorker,
                MachineResourcePriority::Interactive,
            )
            .await
            .unwrap();
        let second = governor
            .acquire(
                MachineResourceKind::MediaWorker,
                MachineResourcePriority::Interactive,
            )
            .await
            .unwrap();
        let waiting_governor = governor.clone();
        let third = tokio::spawn(async move {
            waiting_governor
                .acquire(
                    MachineResourceKind::MediaWorker,
                    MachineResourcePriority::Interactive,
                )
                .await
                .unwrap()
        });
        tokio::task::yield_now().await;
        assert_eq!(governor.snapshot().queued.len(), 1);
        let code = governor
            .acquire(
                MachineResourceKind::CodeReadWorker,
                MachineResourcePriority::Interactive,
            )
            .await
            .unwrap();
        assert_eq!(governor.snapshot().active.len(), 3);
        drop(first);
        drop(second);
        drop(code);
        drop(third.await.unwrap());
    }

    #[tokio::test]
    async fn critical_pressure_keeps_interactive_capacity_and_stops_background_work() {
        let governor = governor(8, 8);
        governor.update_pressure(LocalAgentResourcePressureLevel::Critical);
        let snapshot = governor.snapshot();
        assert_eq!(
            snapshot.pressure_level,
            LocalAgentResourcePressureLevel::Critical
        );
        assert_eq!(snapshot.effective_units, 2);
        assert_eq!(snapshot.maximum_background_units, 0);

        let waiting_governor = governor.clone();
        let background = tokio::spawn(async move {
            waiting_governor
                .acquire(
                    MachineResourceKind::CodeReadWorker,
                    MachineResourcePriority::Background,
                )
                .await
        });
        tokio::task::yield_now().await;
        assert_eq!(governor.snapshot().queued.len(), 1);
        let interactive = governor
            .acquire(
                MachineResourceKind::MediaWorker,
                MachineResourcePriority::Interactive,
            )
            .await
            .unwrap();
        drop(interactive);
        background.abort();
        let _ = background.await;
        tokio::task::yield_now().await;
        assert!(governor.snapshot().queued.is_empty());
    }

    #[tokio::test]
    async fn queue_limit_and_cancelled_waiters_fail_closed() {
        let governor = governor(2, 1);
        let first = governor
            .acquire(
                MachineResourceKind::MediaWorker,
                MachineResourcePriority::Interactive,
            )
            .await
            .unwrap();
        let waiting_governor = governor.clone();
        let waiting = tokio::spawn(async move {
            waiting_governor
                .acquire(
                    MachineResourceKind::MediaWorker,
                    MachineResourcePriority::Interactive,
                )
                .await
        });
        tokio::task::yield_now().await;
        let result = governor
            .acquire(
                MachineResourceKind::CodeReadWorker,
                MachineResourcePriority::Interactive,
            )
            .await;
        let Err(error) = result else {
            panic!("a full admission queue must reject new work")
        };
        assert!(error.contains("队列已满"));
        waiting.abort();
        let _ = waiting.await;
        tokio::task::yield_now().await;
        assert!(governor.snapshot().queued.is_empty());
        drop(first);
    }
}
