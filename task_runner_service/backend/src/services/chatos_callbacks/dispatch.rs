// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

use super::payload::{build_chatos_task_callback_payload, load_task_snapshot_for_callback};
use super::publisher::{publish_chatos_task_callback, CallbackPublishOutcome};
use crate::models::{ChatosCallbackDeliveryState, ChatosCallbackDeliveryStatus};
use chrono::Utc;
use std::time::Duration;

const CALLBACK_RETRY_DELAYS: [Duration; 6] = [
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(15),
    Duration::from_secs(30),
    Duration::from_secs(60),
    Duration::from_secs(60),
];
pub(super) const TASK_RUN_STARTED_EVENT: &str = "task.run.started";

impl RunService {
    pub(in crate::services) async fn try_send_task_callback(
        &self,
        event: &str,
        task_id: &str,
        run: Option<&TaskRunRecord>,
    ) {
        send_task_callback_with_store(
            &self.config,
            Some(&self.task_queue_topology),
            &self.store,
            event,
            task_id,
            run,
        )
        .await;
    }

    pub(in crate::services) async fn try_send_terminal_callback(
        &self,
        task_id: &str,
        run: &TaskRunRecord,
    ) {
        let Some(event) = terminal_callback_event_for_status(run.status) else {
            return;
        };
        if let Err(err) = self
            .publish_pending_run_terminal_events(self.task_queue_topology.event_outbox_batch_size)
            .await
        {
            warn!(
                run_id = run.id.as_str(),
                error = err.as_str(),
                "failed to publish pending dependency run terminal events"
            );
        }
        if run.task_id != task_id {
            warn!(
                run_id = run.id.as_str(),
                expected_task_id = run.task_id.as_str(),
                callback_task_id = task_id,
                "terminal callback task id does not match run"
            );
        }
        self.deliver_pending_callback(run.id.as_str(), event, true)
            .await;
    }

    pub(in crate::services) async fn try_send_started_callback(&self, run: &TaskRunRecord) {
        self.deliver_pending_callback(run.id.as_str(), TASK_RUN_STARTED_EVENT, true)
            .await;
    }

    pub(super) async fn deliver_pending_callback(
        &self,
        run_id: &str,
        expected_event: &str,
        force: bool,
    ) -> bool {
        let _guard = self
            .callback_delivery_lock_for_run(run_id)
            .lock_owned()
            .await;
        let Some(mut run) = self.store.get_run(run_id).await.ok().flatten() else {
            return false;
        };
        let actual_event = if expected_event == TASK_RUN_STARTED_EVENT {
            if run.status != TaskRunStatus::Running {
                self.record_callback_skipped(
                    run_id,
                    expected_event,
                    "start callback was superseded before delivery",
                )
                .await;
                return true;
            }
            TASK_RUN_STARTED_EVENT
        } else {
            let Some(event) = terminal_callback_event_for_status(run.status) else {
                return false;
            };
            event
        };
        if actual_event != expected_event {
            warn!(
                run_id,
                expected_event, actual_event, "task callback event changed before delivery"
            );
        }
        if callback_delivery_for_event(&run, actual_event).is_none() {
            match self.store.save_run(run.clone()).await {
                Ok(saved) => run = saved,
                Err(err) => {
                    warn!(
                        run_id,
                        error = err.as_str(),
                        "failed to initialize callback outbox state"
                    );
                    return false;
                }
            }
        }
        let Some(delivery) = callback_delivery_for_event(&run, actual_event) else {
            return false;
        };
        if matches!(
            delivery.status,
            ChatosCallbackDeliveryStatus::Enqueued
                | ChatosCallbackDeliveryStatus::Delivered
                | ChatosCallbackDeliveryStatus::Skipped
        ) {
            return false;
        }
        let now = now_rfc3339();
        if !force
            && delivery
                .next_attempt_at
                .as_deref()
                .is_some_and(|next_attempt_at| next_attempt_at > now.as_str())
        {
            return false;
        }

        let attempt = delivery.attempt_count.saturating_add(1);
        let next_attempt_at = callback_next_attempt_at(attempt);
        if let Err(err) = self
            .update_callback_delivery(run_id, actual_event, |state| {
                state.status = ChatosCallbackDeliveryStatus::Pending;
                state.attempt_count = attempt;
                state.next_attempt_at = Some(next_attempt_at.clone());
                state.last_error = None;
                state.updated_at = now_rfc3339();
            })
            .await
        {
            warn!(
                run_id,
                error = err.as_str(),
                "failed to persist callback attempt state"
            );
            return false;
        }

        let task = match load_task_snapshot_for_callback(&self.store, run.task_id.as_str()).await {
            Ok(Some(task)) => task,
            Ok(None) => {
                self.record_callback_failure(
                    run_id,
                    actual_event,
                    "callback task snapshot is missing",
                )
                .await;
                return true;
            }
            Err(err) => {
                self.record_callback_failure(run_id, actual_event, err.as_str())
                    .await;
                return true;
            }
        };
        let Some(payload) =
            build_chatos_task_callback_payload(actual_event, &task, Some(&run), None)
        else {
            if task.source_session_id.is_some()
                || task.source_turn_id.is_some()
                || task.source_run_id.is_some()
            {
                self.record_callback_failure(
                    run_id,
                    actual_event,
                    "task callback source metadata is incomplete",
                )
                .await;
                return true;
            }
            let reason = "task is not linked to a ChatOS source message";
            if let Err(err) = self
                .update_callback_delivery(run_id, actual_event, |state| {
                    state.status = ChatosCallbackDeliveryStatus::Skipped;
                    state.next_attempt_at = None;
                    state.last_error = Some(reason.to_string());
                    state.updated_at = now_rfc3339();
                })
                .await
            {
                warn!(
                    run_id,
                    error = err.as_str(),
                    "failed to persist skipped callback state"
                );
            }
            return true;
        };
        let payload_task_id = payload.task_id.clone();
        let payload_run_id = payload.run_id.clone().unwrap_or_default();
        let payload_user_message_id = payload.source_user_message_id.clone().unwrap_or_default();
        let payload_event = payload.event.clone();
        match publish_chatos_task_callback(self.config.clone(), &self.task_queue_topology, payload)
            .await
        {
            Ok(outcome) => {
                if let Err(err) = self
                    .record_callback_publish_outcome(run_id, actual_event, outcome)
                    .await
                {
                    warn!(
                        run_id,
                        error = err.as_str(),
                        "callback was sent but delivery state was not persisted"
                    );
                }
                info!(
                    task_id = payload_task_id.as_str(),
                    run_id = payload_run_id.as_str(),
                    event = payload_event.as_str(),
                    source_user_message_id = payload_user_message_id.as_str(),
                    attempt,
                    "sent task callback to chatos"
                );
                if outcome == CallbackPublishOutcome::RabbitMqEnqueued {
                    info!(
                        task_id = payload_task_id.as_str(),
                        run_id = payload_run_id.as_str(),
                        event = payload_event.as_str(),
                        "task callback enqueued to rabbitmq for asynchronous delivery"
                    );
                }
            }
            Err(err) => {
                let error_message = err.to_string();
                if err.is_retryable() {
                    self.record_callback_failure(run_id, actual_event, error_message.as_str())
                        .await;
                    warn!(
                        task_id = payload_task_id.as_str(),
                        run_id = payload_run_id.as_str(),
                        event = payload_event.as_str(),
                        attempt,
                        next_attempt_at = next_attempt_at.as_str(),
                        error = %err,
                        "failed to send task callback; persisted for retry"
                    );
                } else {
                    self.record_callback_skipped(run_id, actual_event, error_message.as_str())
                        .await;
                    warn!(
                        task_id = payload_task_id.as_str(),
                        run_id = payload_run_id.as_str(),
                        event = payload_event.as_str(),
                        attempt,
                        error = %err,
                        "task callback permanently rejected; stopped retrying"
                    );
                }
            }
        }
        true
    }

    fn callback_delivery_lock_for_run(&self, run_id: &str) -> KeyedAsyncLockHandle {
        self.callback_delivery_locks.handle(run_id)
    }

    async fn update_callback_delivery(
        &self,
        run_id: &str,
        event: &str,
        update: impl FnOnce(&mut ChatosCallbackDeliveryState),
    ) -> Result<(), String> {
        let mut run =
            self.store.get_run(run_id).await?.ok_or_else(|| {
                format!("run not found while updating callback delivery: {run_id}")
            })?;
        let state = callback_delivery_for_event_mut(&mut run, event).ok_or_else(|| {
            format!("callback delivery state missing for run {run_id} and event {event}")
        })?;
        update(state);
        self.store.save_run(run).await?;
        Ok(())
    }

    async fn record_callback_failure(&self, run_id: &str, event: &str, error: &str) {
        if let Err(persist_error) = self
            .update_callback_delivery(run_id, event, |state| {
                state.status = ChatosCallbackDeliveryStatus::Pending;
                state.next_attempt_at = Some(callback_next_attempt_at(state.attempt_count.max(1)));
                state.last_error = Some(error.to_string());
                state.updated_at = now_rfc3339();
            })
            .await
        {
            warn!(
                run_id,
                error = persist_error.as_str(),
                "failed to persist callback delivery failure"
            );
        }
    }

    async fn record_callback_skipped(&self, run_id: &str, event: &str, error: &str) {
        if let Err(persist_error) = self
            .update_callback_delivery(run_id, event, |state| {
                state.status = ChatosCallbackDeliveryStatus::Skipped;
                state.next_attempt_at = None;
                state.last_error = Some(error.to_string());
                state.updated_at = now_rfc3339();
            })
            .await
        {
            warn!(
                run_id,
                error = persist_error.as_str(),
                "failed to persist permanently rejected callback state"
            );
        }
    }

    async fn record_callback_publish_outcome(
        &self,
        run_id: &str,
        event: &str,
        outcome: CallbackPublishOutcome,
    ) -> Result<(), String> {
        self.update_callback_delivery(run_id, event, |state| {
            state.status = match outcome {
                CallbackPublishOutcome::InlineDelivered => ChatosCallbackDeliveryStatus::Delivered,
                CallbackPublishOutcome::RabbitMqEnqueued => ChatosCallbackDeliveryStatus::Enqueued,
            };
            state.next_attempt_at = None;
            state.last_error = None;
            state.updated_at = now_rfc3339();
        })
        .await
    }

    pub(in crate::services) async fn record_callback_delivery_result(
        &self,
        run_id: &str,
        event: &str,
        delivered: bool,
        failure: Option<(&str, bool)>,
    ) -> Result<(), String> {
        if delivered {
            return self
                .update_callback_delivery(run_id, event, |state| {
                    state.status = ChatosCallbackDeliveryStatus::Delivered;
                    state.next_attempt_at = None;
                    state.last_error = None;
                    state.updated_at = now_rfc3339();
                })
                .await;
        }
        let Some((error, retryable)) = failure else {
            return Ok(());
        };
        self.update_callback_delivery(run_id, event, |state| {
            state.status = if retryable {
                ChatosCallbackDeliveryStatus::Pending
            } else {
                ChatosCallbackDeliveryStatus::Skipped
            };
            state.next_attempt_at =
                retryable.then(|| callback_next_attempt_at(state.attempt_count.max(1)));
            state.last_error = Some(error.to_string());
            state.updated_at = now_rfc3339();
        })
        .await
    }
}

fn callback_delivery_for_event<'a>(
    run: &'a TaskRunRecord,
    event: &str,
) -> Option<&'a ChatosCallbackDeliveryState> {
    if event == TASK_RUN_STARTED_EVENT {
        run.chatos_started_callback_delivery.as_ref()
    } else {
        run.chatos_callback_delivery.as_ref()
    }
}

fn callback_delivery_for_event_mut<'a>(
    run: &'a mut TaskRunRecord,
    event: &str,
) -> Option<&'a mut ChatosCallbackDeliveryState> {
    if event == TASK_RUN_STARTED_EVENT {
        run.chatos_started_callback_delivery.as_mut()
    } else {
        run.chatos_callback_delivery.as_mut()
    }
}

impl TaskService {
    pub(in crate::services) async fn try_send_task_callback(
        &self,
        event: &str,
        task_id: &str,
        run: Option<&TaskRunRecord>,
    ) {
        send_task_callback_with_store(&self.config, None, &self.store, event, task_id, run).await;
    }
}

async fn send_task_callback_with_store(
    config: &AppConfig,
    task_queue_topology: Option<&crate::platform_queue::TaskQueueTopology>,
    store: &AppStore,
    event: &str,
    task_id: &str,
    run: Option<&TaskRunRecord>,
) {
    let task = match load_task_snapshot_for_callback(store, task_id).await {
        Ok(Some(task)) => task,
        Ok(None) => return,
        Err(err) => {
            warn!(
                "failed to load callback task snapshot for task {} and event {}: {}",
                task_id, event, err
            );
            return;
        }
    };
    let Some(payload) = build_chatos_task_callback_payload(event, &task, run, None) else {
        if task.source_session_id.is_some()
            || task.source_turn_id.is_some()
            || task.source_run_id.is_some()
        {
            warn!(
                task_id = task.id.as_str(),
                task_title = task.title.as_str(),
                event,
                source_session_id = task.source_session_id.as_deref().unwrap_or_default(),
                source_turn_id = task.source_turn_id.as_deref().unwrap_or_default(),
                source_user_message_id = task.source_user_message_id.as_deref().unwrap_or_default(),
                "skip task callback because source_user_message_id is missing"
            );
        }
        return;
    };
    let payload_task_id = payload.task_id.clone();
    let payload_run_id = payload.run_id.clone().unwrap_or_default();
    let payload_user_message_id = payload.source_user_message_id.clone().unwrap_or_default();
    let payload_event = payload.event.clone();
    let owned_task_queue_topology;
    let task_queue_topology = if let Some(task_queue_topology) = task_queue_topology {
        task_queue_topology
    } else {
        owned_task_queue_topology =
            match crate::platform_queue::TaskQueueTopology::from_managed_env() {
                Ok(task_queue_topology) => task_queue_topology,
                Err(err) => {
                    warn!(
                        error = err.as_str(),
                        "failed to load managed task queue topology for callback delivery"
                    );
                    return;
                }
            };
        &owned_task_queue_topology
    };
    if let Err(err) =
        publish_chatos_task_callback(config.clone(), task_queue_topology, payload).await
    {
        warn!(
            "failed to send task callback for task {} and event {}: {}",
            task_id, event, err
        );
    } else {
        match task_queue_topology.callback_delivery_mode {
            crate::platform_queue::TaskQueueMode::Inline => info!(
                task_id = payload_task_id.as_str(),
                run_id = payload_run_id.as_str(),
                event = payload_event.as_str(),
                source_user_message_id = payload_user_message_id.as_str(),
                "sent task callback to chatos"
            ),
            crate::platform_queue::TaskQueueMode::RabbitMq => info!(
                task_id = payload_task_id.as_str(),
                run_id = payload_run_id.as_str(),
                event = payload_event.as_str(),
                source_user_message_id = payload_user_message_id.as_str(),
                queue = task_queue_topology.callback_delivery_queue.as_str(),
                "enqueued task callback for asynchronous delivery"
            ),
        }
    }
}

pub(super) fn terminal_callback_event_for_status(status: TaskRunStatus) -> Option<&'static str> {
    match status {
        TaskRunStatus::Succeeded => Some("task.completed"),
        TaskRunStatus::Failed => Some("task.failed"),
        TaskRunStatus::Cancelled => Some("task.cancelled"),
        TaskRunStatus::Blocked => Some("task.blocked"),
        TaskRunStatus::Queued | TaskRunStatus::Running => None,
    }
}

fn callback_next_attempt_at(attempt: u32) -> String {
    let index = attempt.saturating_sub(1) as usize;
    let delay = CALLBACK_RETRY_DELAYS[index.min(CALLBACK_RETRY_DELAYS.len() - 1)];
    let chrono_delay =
        chrono::Duration::from_std(delay).unwrap_or_else(|_| chrono::Duration::minutes(1));
    (Utc::now() + chrono_delay).to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::{callback_next_attempt_at, terminal_callback_event_for_status, TaskRunStatus};
    use chrono::{DateTime, Utc};

    #[test]
    fn terminal_callback_event_for_status_covers_all_terminal_states() {
        assert_eq!(
            terminal_callback_event_for_status(TaskRunStatus::Succeeded),
            Some("task.completed")
        );
        assert_eq!(
            terminal_callback_event_for_status(TaskRunStatus::Failed),
            Some("task.failed")
        );
        assert_eq!(
            terminal_callback_event_for_status(TaskRunStatus::Cancelled),
            Some("task.cancelled")
        );
        assert_eq!(
            terminal_callback_event_for_status(TaskRunStatus::Blocked),
            Some("task.blocked")
        );
        assert_eq!(
            terminal_callback_event_for_status(TaskRunStatus::Queued),
            None
        );
        assert_eq!(
            terminal_callback_event_for_status(TaskRunStatus::Running),
            None
        );
    }

    #[test]
    fn callback_retry_backoff_is_capped() {
        let early = DateTime::parse_from_rfc3339(callback_next_attempt_at(1).as_str())
            .expect("first retry timestamp")
            .with_timezone(&Utc);
        let capped = DateTime::parse_from_rfc3339(callback_next_attempt_at(99).as_str())
            .expect("capped retry timestamp")
            .with_timezone(&Utc);
        let now = Utc::now();

        assert!(early > now);
        assert!(early <= now + chrono::Duration::seconds(3));
        assert!(capped >= now + chrono::Duration::seconds(59));
        assert!(capped <= now + chrono::Duration::seconds(61));
    }
}
