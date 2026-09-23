// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::config::AppConfig;
use crate::models::StartTaskRunRequest;
use crate::pressure::{PlatformPressureLevel, TaskRunnerPressureState};
use crate::services::{RunService, TaskService};
use crate::state::TaskRunnerRuntimeStats;

const SCHEDULER_CLAIM_BATCH_SIZE: usize = 100;

pub fn spawn_task_scheduler(
    config: AppConfig,
    task_service: TaskService,
    run_service: RunService,
    pressure: TaskRunnerPressureState,
    runtime_stats: TaskRunnerRuntimeStats,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        info!(
            "task scheduler started with poll interval {} ms",
            config.scheduler_poll_interval.as_millis()
        );

        let mut pressure_updates = pressure.subscribe();
        loop {
            while pressure_updates.borrow().level == PlatformPressureLevel::Critical {
                runtime_stats.set_scheduler_pressure_paused(true);
                info!("task scheduler paused while platform pressure is critical");
                if pressure_updates.changed().await.is_err() {
                    return;
                }
            }
            runtime_stats.set_scheduler_pressure_paused(false);
            let now = chrono::Utc::now();
            match task_service
                .claim_due_scheduled_tasks(now, SCHEDULER_CLAIM_BATCH_SIZE)
                .await
            {
                Ok(tasks) => {
                    if !tasks.is_empty() {
                        info!(
                            due_count = tasks.len(),
                            due_tasks = tasks
                                .iter()
                                .map(|task| format!("{}:{}", task.id, task.title))
                                .collect::<Vec<_>>()
                                .join(" | "),
                            "scheduler found due tasks"
                        );
                    }
                    for claimed in tasks {
                        match run_service
                            .start_scheduled_run(&claimed.id, StartTaskRunRequest::default())
                            .await
                        {
                            Ok(run) => {
                                info!("scheduler started run {} for task {}", run.id, claimed.id);
                            }
                            Err(err) => {
                                warn!("scheduler failed to start task {}: {}", claimed.id, err);
                                if is_active_run_conflict_error(&err) {
                                    info!(
                                        "scheduler consumed due slot for task {} because an active run already exists",
                                        claimed.id
                                    );
                                }
                            }
                        }
                    }
                }
                Err(err) => {
                    warn!("scheduler failed to list due tasks: {}", err);
                }
            }
            tokio::select! {
                _ = tokio::time::sleep(config.scheduler_poll_interval) => {}
                changed = pressure_updates.changed() => {
                    if changed.is_err() {
                        return;
                    }
                }
            }
        }
    })
}

fn is_active_run_conflict_error(error: &str) -> bool {
    error.contains("当前任务已有正在执行的运行")
        || error.contains("an active run already exists for this task")
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::pressure::TaskRunnerPressurePolicy;

    #[test]
    fn only_critical_pressure_pauses_scheduled_task_discovery() {
        for (level, expected) in [
            (PlatformPressureLevel::Normal, false),
            (PlatformPressureLevel::Elevated, false),
            (PlatformPressureLevel::Critical, true),
        ] {
            let pressure = TaskRunnerPressureState::new(TaskRunnerPressurePolicy {
                level,
                queue_elevated_messages: 100,
                queue_critical_messages: 1_000,
                report_interval: Duration::from_secs(5),
            });
            assert_eq!(
                pressure.snapshot().level == PlatformPressureLevel::Critical,
                expected
            );
        }
    }
}
