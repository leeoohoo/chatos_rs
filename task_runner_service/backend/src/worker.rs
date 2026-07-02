// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::config::AppConfig;
use crate::services::RunService;

pub fn spawn_task_worker(config: AppConfig, run_service: RunService) -> JoinHandle<()> {
    tokio::spawn(async move {
        let semaphore = Arc::new(Semaphore::new(config.worker_concurrency));
        let mut last_stale_recovery = Instant::now();

        info!(
            worker_id = config.worker_id.as_str(),
            concurrency = config.worker_concurrency,
            poll_ms = config.worker_poll_interval.as_millis(),
            claim_ttl_ms = config.worker_claim_ttl.as_millis(),
            "task runner worker started"
        );

        loop {
            if last_stale_recovery.elapsed() >= config.worker_claim_ttl {
                match run_service.fail_expired_run_claims().await {
                    Ok(count) if count > 0 => {
                        warn!(
                            worker_id = config.worker_id.as_str(),
                            recovered_count = count,
                            "task runner worker marked expired run claims as failed"
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
                    warn!(
                        worker_id = worker_id.as_str(),
                        run_id = run.id.as_str(),
                        "task runner worker lost run claim heartbeat"
                    );
                    break;
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
