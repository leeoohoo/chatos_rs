// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{tracing_stdout, LocalRuntime};

use super::execution::execute_local_task_run;

pub(crate) async fn run_local_task_worker_loop(runtime: LocalRuntime) {
    let worker_id = format!("local-worker-{}", Uuid::new_v4());
    if let Ok(database) = runtime.local_database() {
        match database.recover_local_task_runs().await {
            Ok(count) if count > 0 => tracing_stdout(
                format!("marked {count} interrupted local task runs after restart").as_str(),
            ),
            Ok(_) => {}
            Err(error) => {
                tracing_stdout(format!("recover local task runs failed: {error}").as_str())
            }
        }
    }
    loop {
        let run = match runtime.local_database() {
            Ok(database) => database.claim_next_local_task_run(worker_id.as_str()).await,
            Err(error) => Err(error),
        };
        match run {
            Ok(Some(run)) => {
                let abort = CancellationToken::new();
                let monitor_stop = CancellationToken::new();
                let monitor = tokio::spawn(monitor_run(
                    runtime.clone(),
                    run.id.clone(),
                    worker_id.clone(),
                    abort.clone(),
                    monitor_stop.clone(),
                ));
                if let Err(error) = execute_local_task_run(&runtime, &run, abort).await {
                    if let Ok(database) = runtime.local_database() {
                        let _ = database
                            .fail_turn(
                                run.owner_user_id.as_str(),
                                run.turn_id.as_str(),
                                "local_task_runner_failed",
                                error.as_str(),
                            )
                            .await;
                        let _ = database
                            .fail_local_task_run(&run, "failed", error.as_str())
                            .await;
                    }
                    tracing_stdout(format!("local task run {} failed: {error}", run.id).as_str());
                }
                monitor_stop.cancel();
                let _ = monitor.await;
            }
            Ok(None) => tokio::time::sleep(Duration::from_millis(500)).await,
            Err(error) => {
                tracing_stdout(format!("claim local task run failed: {error}").as_str());
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

async fn monitor_run(
    runtime: LocalRuntime,
    run_id: String,
    worker_id: String,
    abort: CancellationToken,
    stop: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = stop.cancelled() => return,
            _ = tokio::time::sleep(Duration::from_secs(5)) => {}
        }
        let Ok(database) = runtime.local_database() else {
            abort.cancel();
            return;
        };
        if database
            .local_task_run_cancel_requested(run_id.as_str())
            .await
            .unwrap_or(true)
        {
            abort.cancel();
        }
        if !database
            .heartbeat_local_task_run(run_id.as_str(), worker_id.as_str())
            .await
            .unwrap_or(false)
        {
            abort.cancel();
            return;
        }
    }
}
