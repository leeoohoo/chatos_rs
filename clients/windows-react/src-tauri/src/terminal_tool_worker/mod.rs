mod code_write;
mod executor;
mod schema;

pub(crate) use schema::{
    project_read_tool_schemas, project_write_tool_schemas, terminal_tool_schemas,
};

use chatos_local_agent_protocol::{
    ClaimNextToolCommand, CommitToolCommand, GetEventCursorCommand, HostCommand, HostResult,
    ListEventsCommand, LocalAgentEventPayloadMode, LocalAgentToolClaim, LocalAgentToolOutcome,
    RenewToolClaimCommand,
};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tauri::{AppHandle, Manager};
use tokio::{
    sync::{Mutex, Notify},
    task::{JoinHandle, JoinSet},
    time::{sleep, Duration, Instant},
};

use crate::{host_lifecycle::HostManager, terminal_runtime::TerminalProcessStore};

pub(crate) const PLATFORM_TOOL_NAMES: [&str; 16] = [
    "read_file_raw",
    "read_file_range",
    "list_dir",
    "search_text",
    "read_file",
    "search_files",
    "open_edit_session",
    "stage_edit_batch",
    "commit_edit_session",
    "abort_edit_session",
    "execute_command",
    "process_poll",
    "process_log",
    "process_wait",
    "process_write",
    "process_kill",
];

const WORKER_ID: &str = "windows-terminal-tool-worker";
const LEASE_DURATION_MS: u64 = 30_000;
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
const EVENT_POLL_INTERVAL: Duration = Duration::from_secs(1);
const EVENT_PAGE_SIZE: u32 = 200;
const MAXIMUM_CONCURRENT_CLAIMS: usize = 4;

pub struct TerminalToolWorker {
    generation: Arc<AtomicU64>,
    wake: Arc<Notify>,
    task: Mutex<Option<JoinHandle<()>>>,
    processes: TerminalProcessStore,
    edits: code_write::CodeWriteStore,
}

impl Default for TerminalToolWorker {
    fn default() -> Self {
        Self {
            generation: Arc::new(AtomicU64::new(0)),
            wake: Arc::new(Notify::new()),
            task: Mutex::new(None),
            processes: TerminalProcessStore::default(),
            edits: code_write::CodeWriteStore::default(),
        }
    }
}

impl TerminalToolWorker {
    pub async fn configure(&self, app: AppHandle, owner_user_id: String) {
        self.reset().await;
        let expected_generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let generation = self.generation.clone();
        let wake = self.wake.clone();
        let processes = self.processes.clone();
        let edits = self.edits.clone();
        *self.task.lock().await = Some(tokio::spawn(async move {
            poll(
                app,
                owner_user_id,
                generation,
                expected_generation,
                wake,
                processes,
                edits,
            )
            .await;
        }));
    }

    pub fn wake(&self) {
        self.wake.notify_one();
    }

    pub async fn release_run(&self, run_id: &str) {
        self.processes.release_run(run_id).await;
        self.edits.release_run(run_id);
    }

    pub(crate) fn project_write_completion_verified(
        &self,
        owner_user_id: &str,
        run_id: &str,
    ) -> bool {
        self.edits.completion_verified(owner_user_id, run_id)
    }

    pub async fn reset(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.wake.notify_waiters();
        if let Some(task) = self.task.lock().await.take() {
            task.abort();
            let _ = task.await;
        }
        self.processes.reset().await;
        self.edits.reset();
    }
}

async fn poll(
    app: AppHandle,
    owner_user_id: String,
    generation: Arc<AtomicU64>,
    expected_generation: u64,
    wake: Arc<Notify>,
    processes: TerminalProcessStore,
    edits: code_write::CodeWriteStore,
) {
    let mut active = JoinSet::new();
    let mut idle_delay = Duration::from_millis(250);
    let mut event_cursor = None;
    let mut next_event_poll = Instant::now();
    while generation.load(Ordering::SeqCst) == expected_generation {
        if Instant::now() >= next_event_poll {
            if release_finished_runs(&app, &owner_user_id, &processes, &edits, &mut event_cursor)
                .await
                .is_err()
                && !app
                    .state::<HostManager>()
                    .is_running_for(&owner_user_id)
                    .await
            {
                processes.reset().await;
                edits.reset();
                return;
            }
            next_event_poll = Instant::now() + EVENT_POLL_INTERVAL;
        }
        while active.len() < MAXIMUM_CONCURRENT_CLAIMS {
            let claim = match claim_next(&app, &owner_user_id).await {
                Ok(claim) => claim,
                Err(_)
                    if !app
                        .state::<HostManager>()
                        .is_running_for(&owner_user_id)
                        .await =>
                {
                    processes.reset().await;
                    edits.reset();
                    return;
                }
                Err(_) => None,
            };
            let Some(claim) = claim else { break };
            let app = app.clone();
            let owner = owner_user_id.clone();
            let generation = generation.clone();
            let processes = processes.clone();
            let edits = edits.clone();
            active.spawn(async move {
                execute_and_commit(
                    app,
                    owner,
                    claim,
                    generation,
                    expected_generation,
                    processes,
                    edits,
                )
                .await
            });
        }

        if active.is_empty() {
            tokio::select! {
                _ = wake.notified() => idle_delay = Duration::from_millis(250),
                _ = sleep(idle_delay) => {
                    idle_delay = (idle_delay * 2).min(Duration::from_secs(2));
                }
            }
        } else {
            tokio::select! {
                result = active.join_next() => {
                    if matches!(result, Some(Ok(true))) {
                        idle_delay = Duration::from_millis(250);
                    }
                }
                _ = wake.notified() => idle_delay = Duration::from_millis(250),
                _ = sleep(Duration::from_millis(250)) => {}
            }
        }
    }
    active.abort_all();
    while active.join_next().await.is_some() {}
}

async fn release_finished_runs(
    app: &AppHandle,
    owner_user_id: &str,
    processes: &TerminalProcessStore,
    edits: &code_write::CodeWriteStore,
    cursor: &mut Option<i64>,
) -> Result<(), String> {
    let host = app.state::<HostManager>();
    let Some(mut after_cursor) = *cursor else {
        let result = host
            .execute(HostCommand::GetEventCursor(GetEventCursorCommand {
                owner_user_id: owner_user_id.to_owned(),
            }))
            .await?;
        let HostResult::EventCursor { cursor: current } = result else {
            return Err("Local Agent Host returned an unexpected event cursor.".into());
        };
        *cursor = Some(current);
        return Ok(());
    };
    loop {
        let result = host
            .execute(HostCommand::ListEvents(ListEventsCommand {
                owner_user_id: owner_user_id.to_owned(),
                after_cursor,
                limit: EVENT_PAGE_SIZE,
                run_id: None,
                event_type: None,
                newest_first: false,
                payload_mode: LocalAgentEventPayloadMode::Routing,
            }))
            .await?;
        let HostResult::Events {
            events,
            next_cursor,
        } = result
        else {
            return Err("Local Agent Host returned unexpected lifecycle events.".into());
        };
        let event_count = events.len();
        for event in events {
            if should_release_run(&event.event_type) {
                processes.release_run(&event.run_id).await;
                edits.release_run(&event.run_id);
            }
        }
        if next_cursor <= after_cursor {
            break;
        }
        after_cursor = next_cursor;
        *cursor = Some(next_cursor);
        if event_count < EVENT_PAGE_SIZE as usize {
            break;
        }
    }
    Ok(())
}

fn should_release_run(event_type: &str) -> bool {
    matches!(event_type, "run_succeeded" | "run_failed" | "run_cancelled")
}

async fn claim_next(
    app: &AppHandle,
    owner_user_id: &str,
) -> Result<Option<LocalAgentToolClaim>, String> {
    let result = app
        .state::<HostManager>()
        .execute(HostCommand::ClaimNextTool(ClaimNextToolCommand {
            owner_user_id: owner_user_id.to_owned(),
            worker_id: WORKER_ID.into(),
            lease_duration_ms: LEASE_DURATION_MS,
            include_tool_names: Some(
                PLATFORM_TOOL_NAMES
                    .iter()
                    .map(|name| (*name).into())
                    .collect(),
            ),
            exclude_tool_names: Vec::new(),
            execution_route: None,
        }))
        .await?;
    let HostResult::ToolClaim { claim } = result else {
        return Err("Local Agent Host returned an unexpected terminal claim result.".into());
    };
    Ok(claim)
}

async fn execute_and_commit(
    app: AppHandle,
    owner_user_id: String,
    claim: LocalAgentToolClaim,
    generation: Arc<AtomicU64>,
    expected_generation: u64,
    processes: TerminalProcessStore,
    edits: code_write::CodeWriteStore,
) -> bool {
    let outcome = match claim.preflight_outcome.clone() {
        Some(outcome) => Some(outcome),
        None => execute_while_renewing(&app, &owner_user_id, &claim, &processes, &edits).await,
    };
    let Some(outcome) = outcome else { return false };
    if generation.load(Ordering::SeqCst) != expected_generation {
        return false;
    }
    let result = app
        .state::<HostManager>()
        .execute(HostCommand::CommitTool(CommitToolCommand {
            owner_user_id,
            invocation_id: claim.invocation.invocation_id.clone(),
            claim_token: claim.claim_token,
            expected_version: claim.invocation.version,
            outcome,
        }))
        .await;
    match result {
        Ok(HostResult::ToolCommit { result }) => {
            if result.run.status.is_terminal() {
                processes.release_run(&result.run.run_id).await;
                edits.release_run(&result.run.run_id);
            }
            true
        }
        _ => false,
    }
}

async fn execute_while_renewing(
    app: &AppHandle,
    owner_user_id: &str,
    claim: &LocalAgentToolClaim,
    processes: &TerminalProcessStore,
    edits: &code_write::CodeWriteStore,
) -> Option<LocalAgentToolOutcome> {
    let execution = executor::execute(app, owner_user_id, claim, processes, edits);
    let renewal = renew_until_lost(app, owner_user_id, claim);
    tokio::pin!(execution);
    tokio::pin!(renewal);
    tokio::select! {
        outcome = &mut execution => Some(outcome),
        _ = &mut renewal => None,
    }
}

async fn renew_until_lost(app: &AppHandle, owner_user_id: &str, claim: &LocalAgentToolClaim) {
    let initial_remaining = claim
        .invocation
        .claim_until_unix_ms
        .map(|until| until.saturating_sub(now_unix_ms()).max(0) as u64)
        .unwrap_or(LEASE_DURATION_MS);
    let mut known_valid_until = Instant::now() + Duration::from_millis(initial_remaining);
    let mut next_delay = HEARTBEAT_INTERVAL.min(Duration::from_millis(initial_remaining));
    loop {
        sleep(next_delay).await;
        let renewal_started = Instant::now();
        let result = app
            .state::<HostManager>()
            .execute(HostCommand::RenewToolClaim(RenewToolClaimCommand {
                owner_user_id: owner_user_id.to_owned(),
                invocation_id: claim.invocation.invocation_id.clone(),
                claim_token: claim.claim_token.clone(),
                expected_version: claim.invocation.version,
                lease_duration_ms: LEASE_DURATION_MS,
            }))
            .await;
        match result {
            Ok(HostResult::ToolClaimRenewed { renewed: true }) => {
                known_valid_until = renewal_started + Duration::from_millis(LEASE_DURATION_MS);
                next_delay = HEARTBEAT_INTERVAL;
            }
            Ok(HostResult::ToolClaimRenewed { renewed: false }) | Ok(_) => return,
            Err(_) if Instant::now() < known_valid_until => {
                next_delay = Duration::from_millis(250)
                    .min(known_valid_until.saturating_duration_since(Instant::now()));
            }
            Err(_) => return,
        }
    }
}

fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_resources_follow_the_macos_run_terminal_events() {
        for event_type in ["run_succeeded", "run_failed", "run_cancelled"] {
            assert!(should_release_run(event_type));
        }
        for event_type in ["run_started", "tool_batch_completed", "step_started"] {
            assert!(!should_release_run(event_type));
        }
    }
}
