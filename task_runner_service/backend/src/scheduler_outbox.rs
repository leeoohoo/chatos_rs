// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use tokio::task::JoinHandle;

use crate::{
    services::spawn_chatos_callback_reconciler, spawn_ask_user_resolution_outbox_reconciler,
    spawn_cloud_agent_outbox_reconciler, spawn_run_cancel_outbox_reconciler,
    spawn_run_event_outbox_reconciler, spawn_run_post_process_outbox_reconciler,
    spawn_run_terminal_outbox_reconciler, AppState,
};

const SCHEDULER_OUTBOX_LEASE_NAME: &str = "scheduler-outbox-reconcilers";
const SCHEDULER_OUTBOX_LEASE_TTL: Duration = Duration::from_secs(30);
const SCHEDULER_OUTBOX_LEASE_RENEW_INTERVAL: Duration = Duration::from_secs(5);
const SCHEDULER_OUTBOX_LEASE_RENEW_TIMEOUT: Duration = Duration::from_secs(5);

pub fn spawn_scheduler_outbox_supervisor(app_state: AppState) -> JoinHandle<()> {
    let owner_id = format!(
        "{}:{}:{}",
        app_state.config.worker_id,
        std::process::id(),
        uuid::Uuid::new_v4()
    );
    tokio::spawn(async move {
        let mut outbox_handles = AbortOnDrop::default();
        loop {
            let renewal = tokio::time::timeout(
                SCHEDULER_OUTBOX_LEASE_RENEW_TIMEOUT,
                app_state.run_service.try_acquire_maintenance_lease(
                    SCHEDULER_OUTBOX_LEASE_NAME,
                    owner_id.as_str(),
                    SCHEDULER_OUTBOX_LEASE_TTL,
                ),
            )
            .await;
            match renewal {
                Ok(Ok(true)) => {
                    if outbox_handles.is_empty() || outbox_handles.has_finished() {
                        if outbox_handles.has_finished() {
                            tracing::warn!(
                                owner_id = owner_id.as_str(),
                                "task runner scheduler restarting stopped outbox reconcilers"
                            );
                            outbox_handles.abort_all();
                        }
                        outbox_handles = start_scheduler_outbox_reconcilers(&app_state);
                        tracing::info!(
                            owner_id = owner_id.as_str(),
                            "task runner scheduler acquired outbox reconciler lease"
                        );
                    }
                }
                Ok(Ok(false)) => {
                    if !outbox_handles.is_empty() {
                        tracing::warn!(
                            owner_id = owner_id.as_str(),
                            "task runner scheduler lost outbox reconciler lease"
                        );
                        outbox_handles.abort_all();
                    }
                }
                Ok(Err(error)) => {
                    outbox_handles.abort_all();
                    tracing::warn!(
                        owner_id = owner_id.as_str(),
                        error = error.as_str(),
                        "task runner scheduler failed to renew outbox reconciler lease"
                    );
                }
                Err(_) => {
                    outbox_handles.abort_all();
                    tracing::warn!(
                        owner_id = owner_id.as_str(),
                        timeout_seconds = SCHEDULER_OUTBOX_LEASE_RENEW_TIMEOUT.as_secs(),
                        "task runner scheduler timed out renewing outbox reconciler lease"
                    );
                }
            }
            tokio::time::sleep(SCHEDULER_OUTBOX_LEASE_RENEW_INTERVAL).await;
        }
    })
}

fn start_scheduler_outbox_reconcilers(app_state: &AppState) -> AbortOnDrop {
    AbortOnDrop(vec![
        spawn_cloud_agent_outbox_reconciler(
            app_state.task_queue_topology.clone(),
            app_state.run_service.clone(),
        ),
        spawn_run_cancel_outbox_reconciler(
            app_state.task_queue_topology.clone(),
            app_state.run_service.clone(),
        ),
        spawn_run_terminal_outbox_reconciler(
            app_state.task_queue_topology.clone(),
            app_state.run_service.clone(),
        ),
        spawn_ask_user_resolution_outbox_reconciler(
            app_state.task_queue_topology.clone(),
            app_state.ask_user_prompt_service.clone(),
        ),
        spawn_run_post_process_outbox_reconciler(
            app_state.task_queue_topology.clone(),
            app_state.run_service.clone(),
        ),
        spawn_run_event_outbox_reconciler(
            app_state.task_queue_topology.clone(),
            app_state.run_service.clone(),
        ),
        spawn_chatos_callback_reconciler(app_state.run_service.clone()),
    ])
}

#[derive(Default)]
struct AbortOnDrop(Vec<JoinHandle<()>>);

impl AbortOnDrop {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn has_finished(&self) -> bool {
        self.0.iter().any(JoinHandle::is_finished)
    }

    fn abort_all(&mut self) {
        for handle in self.0.drain(..) {
            handle.abort();
        }
    }
}

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.abort_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn abort_on_drop_tracks_finished_tasks_and_clears_all_handles() {
        let pending = tokio::spawn(std::future::pending());
        let finished = tokio::spawn(async {});
        tokio::task::yield_now().await;
        let mut handles = AbortOnDrop(vec![pending, finished]);

        assert!(handles.has_finished());
        handles.abort_all();
        assert!(handles.is_empty());
    }

    #[test]
    fn renewal_timeout_is_shorter_than_the_lease_ttl() {
        assert!(SCHEDULER_OUTBOX_LEASE_RENEW_TIMEOUT < SCHEDULER_OUTBOX_LEASE_TTL);
        assert!(SCHEDULER_OUTBOX_LEASE_RENEW_INTERVAL < SCHEDULER_OUTBOX_LEASE_TTL);
    }
}
