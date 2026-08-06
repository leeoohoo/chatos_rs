// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures_util::StreamExt;
use lapin::{
    options::{
        BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicPublishOptions,
        BasicQosOptions, ConfirmSelectOptions, QueueDeclareOptions,
    },
    publisher_confirm::Confirmation,
    types::FieldTable,
    BasicProperties, Channel, Connection, ConnectionProperties,
};
use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::ask_user_prompt_service::AskUserPromptService;
use crate::config::AppConfig;
use crate::models::{now_rfc3339, TaskRunRecord};
use crate::platform_queue::TaskQueueTopology;
use crate::services::RunService;
use crate::store::RunTerminalSubscriptionRecord;

const WORKER_CONTROL_CONSUMER_TAG: &str = "task-runner-worker-control";
const RUN_CANCEL_REQUESTED_EVENT: &str = "run.cancel.requested";
const RUN_TERMINAL_EVENT: &str = "run.terminal";
const ASK_USER_RESOLVED_EVENT: &str = "ask_user.resolved";
const TERMINAL_CLEANUP_REQUESTED_EVENT: &str = "terminal.cleanup.requested";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkerControlEvent {
    event_id: String,
    event_type: String,
    run_id: String,
    worker_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parent_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    prompt_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    subject_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    workspace_dir: Option<String>,
    emitted_at: String,
}

pub fn spawn_worker_control_consumer(
    config: AppConfig,
    task_queue_topology: TaskQueueTopology,
    run_service: RunService,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let queue_name = match task_queue_topology.worker_control_queue_name(&config.worker_id) {
            Ok(queue_name) => queue_name,
            Err(err) => {
                warn!(
                    error = err.as_str(),
                    "invalid task runner worker control queue name"
                );
                return;
            }
        };
        loop {
            match open_worker_control_consumer(
                &task_queue_topology,
                queue_name.as_str(),
                config.worker_concurrency,
            )
            .await
            {
                Ok((connection, mut consumer)) => {
                    let _connection = connection;
                    run_service
                        .runtime_stats()
                        .set_worker_control_consumer_connected(true);
                    info!(
                        worker_id = config.worker_id.as_str(),
                        queue = queue_name.as_str(),
                        "task runner worker control consumer connected to rabbitmq"
                    );
                    while let Some(delivery) = consumer.next().await {
                        match delivery {
                            Ok(delivery) => {
                                if let Ok(event) =
                                    serde_json::from_slice::<WorkerControlEvent>(&delivery.data)
                                {
                                    if event.worker_id == config.worker_id
                                        && event.event_type == TERMINAL_CLEANUP_REQUESTED_EVENT
                                        && event.task_id.is_some()
                                        && event.subject_id.is_some()
                                        && event.workspace_dir.is_some()
                                    {
                                        let run_service = run_service.clone();
                                        let worker_id = config.worker_id.clone();
                                        tokio::spawn(async move {
                                            let result = run_service
                                                .process_terminal_cleanup_event(
                                                    event.run_id.as_str(),
                                                    event.task_id.as_deref().unwrap_or_default(),
                                                    event.subject_id.as_deref().unwrap_or_default(),
                                                    event
                                                        .workspace_dir
                                                        .as_deref()
                                                        .unwrap_or_default(),
                                                )
                                                .await;
                                            let should_ack = match result {
                                                Ok(()) => {
                                                    info!(
                                                        worker_id = worker_id.as_str(),
                                                        run_id = event.run_id.as_str(),
                                                        event_id = event.event_id.as_str(),
                                                        "task runner worker consumed terminal cleanup event"
                                                    );
                                                    true
                                                }
                                                Err(err) => {
                                                    warn!(
                                                        worker_id = worker_id.as_str(),
                                                        run_id = event.run_id.as_str(),
                                                        error = err.as_str(),
                                                        "task runner terminal cleanup failed; returning it to the Outbox"
                                                    );
                                                    run_service
                                                        .retry_terminal_cleanup(
                                                            event.run_id.as_str(),
                                                            err.as_str(),
                                                        )
                                                        .await
                                                        .is_ok()
                                                }
                                            };
                                            if should_ack {
                                                if let Err(err) =
                                                    delivery.ack(BasicAckOptions::default()).await
                                                {
                                                    warn!(
                                                        worker_id = worker_id.as_str(),
                                                        run_id = event.run_id.as_str(),
                                                        error = err.to_string().as_str(),
                                                        "failed to acknowledge terminal cleanup event"
                                                    );
                                                }
                                            } else if let Err(err) = delivery
                                                .nack(BasicNackOptions {
                                                    requeue: true,
                                                    ..BasicNackOptions::default()
                                                })
                                                .await
                                            {
                                                warn!(
                                                    worker_id = worker_id.as_str(),
                                                    run_id = event.run_id.as_str(),
                                                    error = err.to_string().as_str(),
                                                    "failed to requeue terminal cleanup event"
                                                );
                                            }
                                        });
                                        continue;
                                    }
                                }
                                let processing_succeeded = match serde_json::from_slice::<
                                    WorkerControlEvent,
                                >(
                                    &delivery.data
                                ) {
                                    Ok(event) if event.worker_id == config.worker_id => match event
                                        .event_type
                                        .as_str()
                                    {
                                        RUN_CANCEL_REQUESTED_EVENT => {
                                            run_service
                                                .signal_runtime_cancel(event.run_id.as_str());
                                            info!(
                                                    worker_id = config.worker_id.as_str(),
                                                    run_id = event.run_id.as_str(),
                                                    event_id = event.event_id.as_str(),
                                                    "task runner worker consumed run cancellation event"
                                                );
                                            true
                                        }
                                        RUN_TERMINAL_EVENT if event.parent_run_id.is_some() => {
                                            run_service.signal_run_terminal(event.run_id.as_str());
                                            info!(
                                                    worker_id = config.worker_id.as_str(),
                                                    run_id = event.run_id.as_str(),
                                                    parent_run_id = event.parent_run_id.as_deref().unwrap_or_default(),
                                                    event_id = event.event_id.as_str(),
                                                    "task runner worker consumed dependency run terminal event"
                                                );
                                            true
                                        }
                                        ASK_USER_RESOLVED_EVENT if event.prompt_id.is_some() => {
                                            let prompt_id =
                                                event.prompt_id.as_deref().unwrap_or_default();
                                            run_service.signal_ask_user_resolved(prompt_id);
                                            info!(
                                                    worker_id = config.worker_id.as_str(),
                                                    run_id = event.run_id.as_str(),
                                                    prompt_id,
                                                    event_id = event.event_id.as_str(),
                                                    "task runner worker consumed ask_user resolved event"
                                                );
                                            true
                                        }
                                        _ => {
                                            warn!(
                                                worker_id = config.worker_id.as_str(),
                                                event_type = event.event_type.as_str(),
                                                "task runner ignored unsupported worker control event"
                                            );
                                            true
                                        }
                                    },
                                    Ok(event) => {
                                        warn!(
                                            worker_id = config.worker_id.as_str(),
                                            event_type = event.event_type.as_str(),
                                            event_worker_id = event.worker_id.as_str(),
                                            "task runner ignored mismatched worker control event"
                                        );
                                        true
                                    }
                                    Err(err) => {
                                        warn!(
                                            worker_id = config.worker_id.as_str(),
                                            error = err.to_string().as_str(),
                                            "task runner ignored invalid worker control event"
                                        );
                                        true
                                    }
                                };
                                if !processing_succeeded {
                                    break;
                                }
                                if let Err(err) = delivery.ack(BasicAckOptions::default()).await {
                                    warn!(
                                        worker_id = config.worker_id.as_str(),
                                        error = err.to_string().as_str(),
                                        "task runner failed to acknowledge worker control event"
                                    );
                                    break;
                                }
                            }
                            Err(err) => {
                                warn!(
                                    worker_id = config.worker_id.as_str(),
                                    error = err.to_string().as_str(),
                                    "task runner worker control consumer delivery failed"
                                );
                                break;
                            }
                        }
                    }
                }
                Err(err) => warn!(
                    worker_id = config.worker_id.as_str(),
                    error = err.as_str(),
                    "task runner worker control consumer failed to connect to rabbitmq"
                ),
            }
            run_service
                .runtime_stats()
                .set_worker_control_consumer_connected(false);
            run_service
                .runtime_stats()
                .record_rabbitmq_consumer_reconnect();
            tokio::time::sleep(task_queue_topology.rabbitmq_reconnect_delay).await;
        }
    })
}

pub fn spawn_run_cancel_outbox_reconciler(
    task_queue_topology: TaskQueueTopology,
    run_service: RunService,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(task_queue_topology.run_dispatch_outbox_reconcile_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            match run_service
                .publish_pending_run_cancel_events(
                    task_queue_topology.run_dispatch_outbox_batch_size,
                )
                .await
            {
                Ok(count) if count > 0 => info!(
                    published_count = count,
                    "task runner reconciled pending run cancellation events"
                ),
                Ok(_) => {}
                Err(err) => warn!(
                    error = err.as_str(),
                    "task runner failed to reconcile pending run cancellation events"
                ),
            }
        }
    })
}

pub fn spawn_run_terminal_outbox_reconciler(
    task_queue_topology: TaskQueueTopology,
    run_service: RunService,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(task_queue_topology.run_dispatch_outbox_reconcile_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            match run_service
                .publish_pending_run_terminal_events(
                    task_queue_topology.run_dispatch_outbox_batch_size,
                )
                .await
            {
                Ok(count) if count > 0 => info!(
                    published_count = count,
                    "task runner reconciled pending dependency run terminal events"
                ),
                Ok(_) => {}
                Err(err) => warn!(
                    error = err.as_str(),
                    "task runner failed to reconcile dependency run terminal events"
                ),
            }
        }
    })
}

pub fn spawn_ask_user_resolution_outbox_reconciler(
    task_queue_topology: TaskQueueTopology,
    ask_user_prompt_service: AskUserPromptService,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(task_queue_topology.run_dispatch_outbox_reconcile_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            match ask_user_prompt_service
                .publish_pending_resolution_events(
                    task_queue_topology.run_dispatch_outbox_batch_size,
                )
                .await
            {
                Ok(count) if count > 0 => info!(
                    published_count = count,
                    "task runner reconciled pending ask_user resolved events"
                ),
                Ok(_) => {}
                Err(err) => warn!(
                    error = err.as_str(),
                    "task runner failed to reconcile ask_user resolved events"
                ),
            }
        }
    })
}

pub(crate) async fn publish_run_cancel_event(
    task_queue_topology: &TaskQueueTopology,
    run: &TaskRunRecord,
) -> Result<(), String> {
    let worker_id = run
        .worker_id
        .as_deref()
        .ok_or_else(|| format!("running Run {} has no worker id", run.id))?;
    let queue_name = task_queue_topology.worker_control_queue_name(worker_id)?;
    let (_connection, channel) =
        open_worker_control_publisher(task_queue_topology, queue_name.as_str()).await?;
    let event = WorkerControlEvent {
        event_id: format!("run-cancel:{}", run.id),
        event_type: RUN_CANCEL_REQUESTED_EVENT.to_string(),
        run_id: run.id.clone(),
        worker_id: worker_id.to_string(),
        parent_run_id: None,
        prompt_id: None,
        task_id: None,
        subject_id: None,
        workspace_dir: None,
        emitted_at: now_rfc3339(),
    };
    publish_worker_control_event(&channel, queue_name.as_str(), &event).await
}

pub(crate) async fn publish_run_terminal_event(
    task_queue_topology: &TaskQueueTopology,
    run: &TaskRunRecord,
    subscription: &RunTerminalSubscriptionRecord,
) -> Result<(), String> {
    let queue_name = task_queue_topology.worker_control_queue_name(&subscription.worker_id)?;
    let (_connection, channel) =
        open_worker_control_publisher(task_queue_topology, queue_name.as_str()).await?;
    let event = WorkerControlEvent {
        event_id: format!("run-terminal:{}:{}", run.id, subscription.id),
        event_type: RUN_TERMINAL_EVENT.to_string(),
        run_id: run.id.clone(),
        worker_id: subscription.worker_id.clone(),
        parent_run_id: Some(subscription.parent_run_id.clone()),
        prompt_id: None,
        task_id: None,
        subject_id: None,
        workspace_dir: None,
        emitted_at: now_rfc3339(),
    };
    publish_worker_control_event(&channel, queue_name.as_str(), &event).await
}

pub(crate) async fn publish_ask_user_resolved_event(
    task_queue_topology: &TaskQueueTopology,
    prompt_id: &str,
    run: &TaskRunRecord,
) -> Result<(), String> {
    let worker_id = run
        .worker_id
        .as_deref()
        .ok_or_else(|| format!("Run {} has no Worker id for ask_user routing", run.id))?;
    let queue_name = task_queue_topology.worker_control_queue_name(worker_id)?;
    let (_connection, channel) =
        open_worker_control_publisher(task_queue_topology, queue_name.as_str()).await?;
    let event = WorkerControlEvent {
        event_id: format!("ask-user-resolved:{prompt_id}"),
        event_type: ASK_USER_RESOLVED_EVENT.to_string(),
        run_id: run.id.clone(),
        worker_id: worker_id.to_string(),
        parent_run_id: None,
        prompt_id: Some(prompt_id.to_string()),
        task_id: None,
        subject_id: None,
        workspace_dir: None,
        emitted_at: now_rfc3339(),
    };
    publish_worker_control_event(&channel, queue_name.as_str(), &event).await
}

pub(crate) async fn publish_terminal_cleanup_event(
    task_queue_topology: &TaskQueueTopology,
    run: &TaskRunRecord,
    task_id: &str,
    subject_id: &str,
    workspace_dir: &str,
) -> Result<(), String> {
    let worker_id = run
        .worker_id
        .as_deref()
        .ok_or_else(|| format!("Run {} has no Worker id for terminal cleanup", run.id))?;
    let queue_name = task_queue_topology.worker_control_queue_name(worker_id)?;
    let (_connection, channel) =
        open_worker_control_publisher(task_queue_topology, queue_name.as_str()).await?;
    let event = WorkerControlEvent {
        event_id: format!("terminal-cleanup:{}", run.id),
        event_type: TERMINAL_CLEANUP_REQUESTED_EVENT.to_string(),
        run_id: run.id.clone(),
        worker_id: worker_id.to_string(),
        parent_run_id: None,
        prompt_id: None,
        task_id: Some(task_id.to_string()),
        subject_id: Some(subject_id.to_string()),
        workspace_dir: Some(workspace_dir.to_string()),
        emitted_at: now_rfc3339(),
    };
    publish_worker_control_event(&channel, queue_name.as_str(), &event).await
}

async fn open_worker_control_publisher(
    task_queue_topology: &TaskQueueTopology,
    queue_name: &str,
) -> Result<(Connection, Channel), String> {
    let rabbitmq_url = task_queue_topology.rabbitmq_url.as_deref().ok_or_else(|| {
        "TASK_RUNNER_RABBITMQ_URL is required for worker control events".to_string()
    })?;
    let connection = Connection::connect(rabbitmq_url, ConnectionProperties::default())
        .await
        .map_err(|err| err.to_string())?;
    let channel = connection
        .create_channel()
        .await
        .map_err(|err| err.to_string())?;
    channel
        .confirm_select(ConfirmSelectOptions::default())
        .await
        .map_err(|err| err.to_string())?;
    ensure_worker_control_queue(&channel, queue_name).await?;
    Ok((connection, channel))
}

async fn publish_worker_control_event(
    channel: &Channel,
    queue_name: &str,
    event: &WorkerControlEvent,
) -> Result<(), String> {
    let payload = serde_json::to_vec(event).map_err(|err| err.to_string())?;
    let confirmation = channel
        .basic_publish(
            "",
            queue_name,
            BasicPublishOptions {
                mandatory: true,
                ..BasicPublishOptions::default()
            },
            payload.as_slice(),
            BasicProperties::default()
                .with_content_type("application/json".into())
                .with_delivery_mode(2)
                .with_message_id(event.event_id.clone().into()),
        )
        .await
        .map_err(|err| err.to_string())?
        .await
        .map_err(|err| err.to_string())?;
    ensure_worker_control_publish_confirmed(queue_name, confirmation)
}

fn ensure_worker_control_publish_confirmed(
    queue_name: &str,
    confirmation: Confirmation,
) -> Result<(), String> {
    match confirmation {
        Confirmation::Ack(None) => Ok(()),
        Confirmation::Ack(Some(_)) => Err(format!(
            "RabbitMQ returned unroutable Task Runner worker control event for {queue_name}"
        )),
        Confirmation::Nack(_) => Err(format!(
            "RabbitMQ rejected Task Runner worker control event for {queue_name}"
        )),
        Confirmation::NotRequested => Err(
            "RabbitMQ publisher confirm was not enabled for Task Runner worker control event"
                .to_string(),
        ),
    }
}

async fn open_worker_control_consumer(
    task_queue_topology: &TaskQueueTopology,
    queue_name: &str,
    worker_concurrency: usize,
) -> Result<(Connection, lapin::Consumer), String> {
    let rabbitmq_url = task_queue_topology.rabbitmq_url.as_deref().ok_or_else(|| {
        "TASK_RUNNER_RABBITMQ_URL is required for worker control events".to_string()
    })?;
    let connection = Connection::connect(rabbitmq_url, ConnectionProperties::default())
        .await
        .map_err(|err| err.to_string())?;
    let channel = connection
        .create_channel()
        .await
        .map_err(|err| err.to_string())?;
    ensure_worker_control_queue(&channel, queue_name).await?;
    channel
        .basic_qos(
            worker_control_prefetch(worker_concurrency)?,
            BasicQosOptions::default(),
        )
        .await
        .map_err(|err| err.to_string())?;
    let consumer = channel
        .basic_consume(
            queue_name,
            WORKER_CONTROL_CONSUMER_TAG,
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok((connection, consumer))
}

fn worker_control_prefetch(worker_concurrency: usize) -> Result<u16, String> {
    let prefetch = u16::try_from(worker_concurrency)
        .map_err(|_| "Task Runner worker concurrency exceeds RabbitMQ prefetch limit")?;
    if prefetch == 0 {
        return Err("Task Runner worker concurrency must be greater than zero".to_string());
    }
    Ok(prefetch)
}

async fn ensure_worker_control_queue(channel: &Channel, queue_name: &str) -> Result<(), String> {
    channel
        .queue_declare(
            queue_name,
            QueueDeclareOptions {
                durable: true,
                ..QueueDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_control_prefetch_requires_valid_concurrency() {
        assert_eq!(worker_control_prefetch(1).expect("minimum prefetch"), 1);
        assert_eq!(
            worker_control_prefetch(u16::MAX as usize).expect("maximum prefetch"),
            u16::MAX
        );
        assert!(worker_control_prefetch(0).is_err());
        assert!(worker_control_prefetch(u16::MAX as usize + 1).is_err());
    }

    #[test]
    fn worker_control_outbox_requires_confirmed_routing() {
        assert!(ensure_worker_control_publish_confirmed(
            "task_runner.worker.control.worker-1",
            Confirmation::Ack(None),
        )
        .is_ok());
        assert!(ensure_worker_control_publish_confirmed(
            "task_runner.worker.control.worker-1",
            Confirmation::Nack(None),
        )
        .is_err());
        assert!(ensure_worker_control_publish_confirmed(
            "task_runner.worker.control.worker-1",
            Confirmation::NotRequested,
        )
        .is_err());
    }
}
