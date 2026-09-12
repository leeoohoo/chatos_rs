// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Duration;

use chatos_client_storage::{ClientStorage, RecordScope, StorageError};
use chatos_local_agent_runtime::MemorySynchronizer;
use chrono::Utc;
use tokio_util::sync::CancellationToken;

const DEFAULT_IDLE_SCAN_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LocalAgentMemorySyncWorkerExit {
    pub claimed_record_count: u64,
    pub synchronized_record_count: u64,
    pub permanently_failed_record_count: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum LocalAgentMemorySyncWorkerError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("local Agent Memory Sync counter overflowed")]
    CounterOverflow,
}

/// The Host-owned background consumer for the Memory Engine outbox. It sends
/// one bounded batch at a time and never participates in the Agent event
/// reducer, so remote Memory availability cannot own or advance a Run.
pub struct LocalAgentMemorySyncWorker {
    storage: Arc<dyn ClientStorage>,
    scope: RecordScope,
    synchronizer: MemorySynchronizer,
    idle_scan_interval: Duration,
}

impl LocalAgentMemorySyncWorker {
    pub fn new(
        storage: Arc<dyn ClientStorage>,
        scope: RecordScope,
        synchronizer: MemorySynchronizer,
    ) -> Self {
        Self {
            storage,
            scope,
            synchronizer,
            idle_scan_interval: DEFAULT_IDLE_SCAN_INTERVAL,
        }
    }

    pub async fn run(
        &self,
        shutdown: CancellationToken,
    ) -> Result<LocalAgentMemorySyncWorkerExit, LocalAgentMemorySyncWorkerError> {
        let mut exit = LocalAgentMemorySyncWorkerExit::default();
        loop {
            if shutdown.is_cancelled() {
                return Ok(exit);
            }
            let report = self
                .synchronizer
                .sync_once(
                    self.storage.as_ref(),
                    self.scope.clone(),
                    Utc::now(),
                    shutdown.child_token(),
                )
                .await?;
            exit.claimed_record_count = checked_add(exit.claimed_record_count, report.claimed)?;
            exit.synchronized_record_count =
                checked_add(exit.synchronized_record_count, report.synced)?;
            let permanently_failed = report
                .permanently_failed
                .checked_add(report.exhausted_before_send)
                .ok_or(LocalAgentMemorySyncWorkerError::CounterOverflow)?;
            exit.permanently_failed_record_count =
                checked_add(exit.permanently_failed_record_count, permanently_failed)?;
            if shutdown.is_cancelled() {
                return Ok(exit);
            }
            if report.claimed > 0 {
                continue;
            }
            tokio::select! {
                _ = shutdown.cancelled() => return Ok(exit),
                _ = tokio::time::sleep(self.idle_scan_interval) => {}
            }
        }
    }
}

fn checked_add(current: u64, increment: usize) -> Result<u64, LocalAgentMemorySyncWorkerError> {
    current
        .checked_add(
            u64::try_from(increment)
                .map_err(|_| LocalAgentMemorySyncWorkerError::CounterOverflow)?,
        )
        .ok_or(LocalAgentMemorySyncWorkerError::CounterOverflow)
}
