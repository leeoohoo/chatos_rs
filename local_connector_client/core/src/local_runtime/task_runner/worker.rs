// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::terminal::controller::kill_local_terminal_sessions_for_task_run;
use crate::{tracing_stdout, LocalRuntime};

use super::execution::{
    execute_local_task_run, persist_task_run_receipt, set_requirement_status, set_work_item_status,
    user_visible_task_run_failure_receipt,
};

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
                        let _ = persist_task_run_receipt(
                            database,
                            &run,
                            "failed",
                            user_visible_task_run_failure_receipt(error.as_str()),
                        )
                        .await;
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
                        let _ = set_work_item_status(&runtime, &run, "blocked").await;
                        let _ = set_requirement_status(&runtime, &run, "failed").await;
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
    let mut ticks = 0u64;
    loop {
        tokio::select! {
            _ = stop.cancelled() => return,
            _ = tokio::time::sleep(Duration::from_secs(1)) => {}
        }
        ticks = ticks.saturating_add(1);
        let Ok(database) = runtime.local_database() else {
            let _ = kill_local_terminal_sessions_for_task_run(run_id.as_str()).await;
            abort.cancel();
            return;
        };
        if database
            .local_task_run_cancel_requested(run_id.as_str())
            .await
            .unwrap_or(true)
        {
            let _ = kill_local_terminal_sessions_for_task_run(run_id.as_str()).await;
            abort.cancel();
            return;
        }
        if ticks.is_multiple_of(5)
            && !database
                .heartbeat_local_task_run(run_id.as_str(), worker_id.as_str())
                .await
                .unwrap_or(false)
        {
            let _ = kill_local_terminal_sessions_for_task_run(run_id.as_str()).await;
            abort.cancel();
            return;
        }
    }
}
