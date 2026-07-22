// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use tokio::task::JoinHandle;

const CALLBACK_RECONCILE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);
const CALLBACK_RECONCILE_BATCH_SIZE: usize = 100;

pub fn spawn_chatos_callback_reconciler(run_service: RunService) -> JoinHandle<()> {
    tokio::spawn(async move {
        info!("task callback delivery reconciler started");
        if let Err(err) = run_service
            .reconcile_pending_chatos_callbacks_with_force(true)
            .await
        {
            warn!(
                error = err.as_str(),
                "failed to reconcile pending task callbacks during startup"
            );
        }
        loop {
            match run_service.reconcile_pending_chatos_callbacks().await {
                Ok(attempted) if attempted > 0 => {
                    info!(attempted, "reconciled pending task callbacks");
                }
                Ok(_) => {}
                Err(err) => {
                    warn!(
                        error = err.as_str(),
                        "failed to reconcile pending task callbacks"
                    );
                }
            }
            tokio::time::sleep(CALLBACK_RECONCILE_INTERVAL).await;
        }
    })
}

impl RunService {
    pub async fn reconcile_pending_chatos_callbacks(&self) -> Result<usize, String> {
        self.reconcile_pending_chatos_callbacks_with_force(false)
            .await
    }

    async fn reconcile_pending_chatos_callbacks_with_force(
        &self,
        force: bool,
    ) -> Result<usize, String> {
        let due_before = if force {
            "9999-12-31T23:59:59+00:00".to_string()
        } else {
            now_rfc3339()
        };
        let runs = self
            .store
            .list_pending_chatos_callback_runs(due_before.as_str(), CALLBACK_RECONCILE_BATCH_SIZE)
            .await?;
        let mut attempted = 0usize;
        for run in runs {
            let Some(event) = super::dispatch::terminal_callback_event_for_status(run.status)
            else {
                continue;
            };
            if self
                .deliver_pending_terminal_callback(run.id.as_str(), event, force)
                .await
            {
                attempted += 1;
            }
        }
        Ok(attempted)
    }
}
