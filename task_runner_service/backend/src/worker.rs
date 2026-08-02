// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::config::AppConfig;
use crate::services::{RejectedRunClaimHeartbeatAction, RunService};

pub fn spawn_task_worker(config: AppConfig, run_service: RunService) -> JoinHandle<()> {
    tokio::spawn(async move {
        let semaphore = Arc::new(Semaphore::new(config.worker_concurrency));
        let mut last_stale_recovery = Instant::now();
        let stale_recovery_interval = heartbeat_interval(config.worker_claim_ttl);

        info!(
            worker_id = config.worker_id.as_str(),
            concurrency = config.worker_concurrency,
            poll_ms = config.worker_poll_interval.as_millis(),
            claim_ttl_ms = config.worker_claim_ttl.as_millis(),
            claim_expiry_grace_ms =
                crate::services::worker_claim_expiry_grace(config.worker_claim_ttl).as_millis(),
            stale_recovery_poll_ms = stale_recovery_interval.as_millis(),
            "task runner worker started"
        );

        loop {
            if last_stale_recovery.elapsed() >= stale_recovery_interval {
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
                last_stale_recovery = Instant::now();
            }

            while let Ok(permit) = semaphore.clone().try_acquire_owned() {
                match run_service
                    .claim_next_queued_run(config.worker_id.as_str(), config.worker_claim_ttl)
                    .await
                {
                    Ok(Some(run)) => {
                        spawn_claimed_run(
                            run_service.clone(),
                            config.worker_id.clone(),
                            config.worker_claim_ttl,
                            run,
                            permit,
                        );
                    }
                    Ok(None) => {
                        drop(permit);
                        break;
                    }
                    Err(err) => {
                        drop(permit);
                        warn!(
                            worker_id = config.worker_id.as_str(),
                            error = err.as_str(),
                            "task runner worker failed to claim queued run"
                        );
                        break;
                    }
                }
            }

            tokio::time::sleep(config.worker_poll_interval).await;
        }
    })
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
