// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use lapin::{
    options::{BasicAckOptions, BasicConsumeOptions, BasicQosOptions, ConfirmSelectOptions},
    types::FieldTable,
    Channel, Connection, ConnectionProperties,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::config::AppConfig;
use crate::run_dispatch_queue::{
    defer_run_dispatch, ensure_run_dispatch_topology, QueuedRunDispatchEnvelope,
};
use crate::services::{RejectedRunClaimHeartbeatAction, RunService};

const RUN_DISPATCH_QUEUE_CONSUMER_TAG: &str = "task-runner-run-dispatch";

pub fn spawn_task_worker(config: AppConfig, run_service: RunService) -> JoinHandle<()> {
    tokio::spawn(async move {
        let semaphore = Arc::new(Semaphore::new(config.worker_concurrency));
        let stale_recovery_interval = heartbeat_interval(config.worker_claim_ttl);

        info!(
            worker_id = config.worker_id.as_str(),
            concurrency = config.worker_concurrency,
            claim_ttl_ms = config.worker_claim_ttl.as_millis(),
            claim_expiry_grace_ms =
                crate::services::worker_claim_expiry_grace(config.worker_claim_ttl).as_millis(),
            stale_recovery_interval_ms = stale_recovery_interval.as_millis(),
            "task runner rabbitmq worker started"
        );

        run_rabbitmq_dispatch_worker_loop(config, run_service, semaphore, stale_recovery_interval)
            .await;
    })
}

async fn run_rabbitmq_dispatch_worker_loop(
    config: AppConfig,
    run_service: RunService,
    semaphore: Arc<Semaphore>,
    stale_recovery_interval: Duration,
) {
    loop {
        reconcile_stale_claims(&config, &run_service).await;
        match open_run_dispatch_consumer(
            run_service.task_queue_topology(),
            config.worker_concurrency,
        )
        .await
        {
            Ok((connection, channel, mut consumer)) => {
                let _connection = connection;
                run_service
                    .runtime_stats()
                    .set_run_dispatch_consumer_connected(true);
                let mut stale_recovery = tokio::time::interval(stale_recovery_interval);
                stale_recovery.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                stale_recovery.tick().await;
                info!(
                    worker_id = config.worker_id.as_str(),
                    queue = run_service
                        .task_queue_topology()
                        .run_dispatch_queue
                        .as_str(),
                    "task runner worker connected to rabbitmq run dispatch queue"
                );
                loop {
                    tokio::select! {
                        delivery = consumer.next() => {
                            match delivery {
                                Some(Ok(delivery)) => {
                                    let permit = match semaphore.clone().acquire_owned().await {
                                        Ok(permit) => permit,
                                        Err(_) => break,
                                    };
                                    if let Err(err) = handle_run_dispatch_delivery(
                                        &config,
                                        &run_service,
                                        &channel,
                                        delivery,
                                        permit,
                                    )
                                    .await
                                    {
                                        warn!(
                                            worker_id = config.worker_id.as_str(),
                                            error = err.as_str(),
                                            "task runner worker failed to process rabbitmq dispatch delivery"
                                        );
                                        break;
                                    }
                                }
                                Some(Err(err)) => {
                                    warn!(
                                        worker_id = config.worker_id.as_str(),
                                        error = err.to_string().as_str(),
                                        "task runner worker rabbitmq consumer delivery failed"
                                    );
                                    break;
                                }
                                None => {
                                    warn!(
                                        worker_id = config.worker_id.as_str(),
                                        "task runner worker rabbitmq consumer stream closed"
                                    );
                                    break;
                                }
                            }
                        }
                        _ = stale_recovery.tick() => {
                            reconcile_stale_claims(&config, &run_service).await;
                        }
                    }
                }
            }
            Err(err) => {
                warn!(
                    worker_id = config.worker_id.as_str(),
                    error = err.as_str(),
                    "task runner worker failed to connect to rabbitmq run dispatch queue"
                );
            }
        }
        run_service
            .runtime_stats()
            .set_run_dispatch_consumer_connected(false);
        run_service
            .runtime_stats()
            .record_rabbitmq_consumer_reconnect();
        tokio::time::sleep(run_service.task_queue_topology().rabbitmq_reconnect_delay).await;
    }
}

async fn open_run_dispatch_consumer(
    task_queue_topology: &crate::platform_queue::TaskQueueTopology,
    worker_concurrency: usize,
) -> Result<(Connection, Channel, lapin::Consumer), String> {
    let rabbitmq_url = task_queue_topology.rabbitmq_url.as_deref().ok_or_else(|| {
        "TASK_RUNNER_RABBITMQ_URL is required when run dispatch uses RabbitMQ".to_string()
    })?;
    let connection = Connection::connect(rabbitmq_url, ConnectionProperties::default())
        .await
        .map_err(|err| err.to_string())?;
    let channel = connection
        .create_channel()
        .await
        .map_err(|err| err.to_string())?;
    ensure_run_dispatch_topology(&channel, task_queue_topology).await?;
    channel
        .confirm_select(ConfirmSelectOptions::default())
        .await
        .map_err(|err| err.to_string())?;
    channel
        .basic_qos(
            u16::try_from(worker_concurrency)
                .map_err(|_| "Task Runner worker concurrency exceeds RabbitMQ prefetch limit")?,
            BasicQosOptions::default(),
        )
        .await
        .map_err(|err| err.to_string())?;
    let consumer = channel
        .basic_consume(
            task_queue_topology.run_dispatch_queue.as_str(),
            RUN_DISPATCH_QUEUE_CONSUMER_TAG,
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok((connection, channel, consumer))
}

async fn handle_run_dispatch_delivery(
    config: &AppConfig,
    run_service: &RunService,
    channel: &Channel,
    delivery: lapin::message::Delivery,
    permit: OwnedSemaphorePermit,
) -> Result<(), String> {
    let envelope = match serde_json::from_slice::<QueuedRunDispatchEnvelope>(&delivery.data) {
        Ok(envelope) => envelope,
        Err(err) => {
            delivery
                .ack(BasicAckOptions::default())
                .await
                .map_err(|ack_err| ack_err.to_string())?;
            drop(permit);
            return Err(format!("invalid run dispatch envelope: {err}"));
        }
    };
    let run = match run_service
        .claim_next_queued_run(config.worker_id.as_str(), config.worker_claim_ttl)
        .await
    {
        Ok(run) => run,
        Err(err) => {
            run_service.runtime_stats().record_worker_claim_failure();
            return Err(err);
        }
    };
    if let Some(run) = run {
        delivery
            .ack(BasicAckOptions::default())
            .await
            .map_err(|err| err.to_string())?;
        spawn_claimed_run(
            run_service.clone(),
            config.worker_id.clone(),
            config.worker_claim_ttl,
            run,
            permit,
        );
    } else {
        let should_defer = run_service.has_queued_run_waiting_for_execution().await?;
        if should_defer {
            run_service
                .runtime_stats()
                .record_run_dispatch_fairness_deferral();
            defer_run_dispatch(
                channel,
                run_service.task_queue_topology(),
                delivery.data.as_slice(),
            )
            .await?;
            info!(
                worker_id = config.worker_id.as_str(),
                trigger_run_id = envelope.run_id.as_str(),
                retry_delay_ms = run_service
                    .task_queue_topology()
                    .run_dispatch_retry_delay
                    .as_millis(),
                "task runner deferred fair scheduling trigger until an execution lane is available"
            );
        }
        delivery
            .ack(BasicAckOptions::default())
            .await
            .map_err(|err| err.to_string())?;
        drop(permit);
    }
    Ok(())
}

async fn reconcile_stale_claims(config: &AppConfig, run_service: &RunService) {
    match run_service
        .reconcile_expired_run_claims(config.worker_claim_ttl)
        .await
    {
        Ok(count) if count > 0 => {
            warn!(
                worker_id = config.worker_id.as_str(),
                recovered_count = count,
                "task runner worker reconciled expired run claims"
            );
        }
        Ok(_) => {}
        Err(err) => {
            warn!(
                worker_id = config.worker_id.as_str(),
                error = err.as_str(),
                "task runner worker failed to recover expired run claims"
            );
        }
    }
}

fn spawn_claimed_run(
    run_service: RunService,
    worker_id: String,
    claim_ttl: Duration,
    run: crate::models::TaskRunRecord,
    permit: OwnedSemaphorePermit,
) {
    tokio::spawn(async move {
        let _permit = permit;
        let run_id = run.id.clone();
        // A recovered run reuses its run id. Clear only this process' stale
        // abort marker before the new claim starts; the persisted claim token
        // still fences an older execution from committing any output.
        run_service.clear_local_run_abort(run_id.as_str());
        let heartbeat = spawn_claim_heartbeat(
            run_service.clone(),
            worker_id.clone(),
            claim_ttl,
            run.clone(),
        );
        info!(
            worker_id = worker_id.as_str(),
            run_id = run.id.as_str(),
            task_id = run.task_id.as_str(),
            attempt = run.attempt,
            "task runner worker executing claimed run"
        );
        run_service.execute_claimed_run(run).await;
        heartbeat.abort();
        run_service.clear_local_run_abort(run_id.as_str());
    });
}

fn spawn_claim_heartbeat(
    run_service: RunService,
    worker_id: String,
    claim_ttl: Duration,
    run: crate::models::TaskRunRecord,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let interval = heartbeat_interval(claim_ttl);
        loop {
            tokio::time::sleep(interval).await;
            match run_service
                .renew_run_claim(&run, worker_id.as_str(), claim_ttl)
                .await
            {
                Ok(true) => {}
                Ok(false) => {
                    match run_service
                        .handle_rejected_run_claim_heartbeat(&run, worker_id.as_str())
                        .await
                    {
                        Ok(RejectedRunClaimHeartbeatAction::Abort) => {
                            warn!(
                                worker_id = worker_id.as_str(),
                                run_id = run.id.as_str(),
                                "task runner worker lost run claim; execution abort requested"
                            );
                            break;
                        }
                        Ok(RejectedRunClaimHeartbeatAction::Stop) => {
                            info!(
                                worker_id = worker_id.as_str(),
                                run_id = run.id.as_str(),
                                "task runner heartbeat stopped after run reached terminal state"
                            );
                            break;
                        }
                        Ok(RejectedRunClaimHeartbeatAction::Continue) => {
                            warn!(
                                worker_id = worker_id.as_str(),
                                run_id = run.id.as_str(),
                                "task runner heartbeat renewal was rejected but claim is still current; retrying"
                            );
                        }
                        Err(err) => {
                            run_service.signal_local_run_abort(run.id.as_str());
                            warn!(
                                worker_id = worker_id.as_str(),
                                run_id = run.id.as_str(),
                                error = err.as_str(),
                                "task runner could not verify rejected heartbeat; execution abort requested defensively"
                            );
                            break;
                        }
                    }
                }
                Err(err) => {
                    warn!(
                        worker_id = worker_id.as_str(),
                        run_id = run.id.as_str(),
                        error = err.as_str(),
                        "task runner worker failed to renew run claim"
                    );
                }
            }
        }
    })
}

fn heartbeat_interval(claim_ttl: Duration) -> Duration {
    let millis = (claim_ttl.as_millis() / 3).clamp(1_000, 30_000) as u64;
    Duration::from_millis(millis)
}
