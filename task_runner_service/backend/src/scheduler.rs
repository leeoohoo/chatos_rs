// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::config::AppConfig;
use crate::models::StartTaskRunRequest;
use crate::pressure::{PlatformPressureLevel, TaskRunnerPressureState};
use crate::services::{RunService, TaskService};
use crate::state::TaskRunnerRuntimeStats;

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
            match task_service.list_due_scheduled_tasks(now).await {
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
                    for task in tasks {
                        match run_service.has_active_run_for_task(&task.id).await {
                            Ok(true) => {
                                match task_service
                                    .mark_scheduled_run_started_if_due(&task, now)
                                    .await
                                {
                                    Ok(Some(_)) => {
                                        info!(
                                            "scheduler consumed due slot for task {} because an active run already exists",
                                            task.id
                                        );
                                    }
                                    Ok(None) => {
                                        info!(
                                            "scheduler skipped due slot for task {} because another scheduler already advanced it",
                                            task.id
                                        );
                                    }
                                    Err(err) => {
                                        warn!(
                                            "scheduler failed to advance next_run_at for already-active task {}: {}",
                                            task.id, err
                                        );
                                    }
                                }
                                continue;
                            }
                            Ok(false) => {}
                            Err(err) => {
                                warn!(
                                    "scheduler failed to inspect active runs for task {}: {}",
                                    task.id, err
                                );
                                continue;
                            }
                        }

                        match run_service
                            .start_scheduled_run(&task.id, StartTaskRunRequest::default())
                            .await
                        {
                            Ok(run) => {
                                match task_service
                                    .mark_scheduled_run_started_if_due(&task, now)
                                    .await
                                {
                                    Ok(Some(_)) => {
                                        info!(
                                            "scheduler started run {} for task {}",
                                            run.id, task.id
                                        );
                                    }
                                    Ok(None) => {
                                        info!(
                                            "scheduler started run {} for task {}, but due slot was already advanced by another scheduler",
                                            run.id, task.id
                                        );
                                    }
                                    Err(err) => {
                                        warn!(
                                            "scheduler failed to advance next_run_at for task {} after run {}: {}",
                                            task.id, run.id, err
                                        );
                                    }
                                }
                            }
                            Err(err) => {
                                warn!("scheduler failed to start task {}: {}", task.id, err);
                                if is_active_run_conflict_error(&err) {
                                    match task_service
                                        .mark_scheduled_run_started_if_due(&task, now)
                                        .await
                                    {
                                        Ok(Some(_)) => {}
                                        Ok(None) => {
                                            info!(
                                                "scheduler skipped active-run conflict slot for task {} because another scheduler already advanced it",
                                                task.id
                                            );
                                        }
                                        Err(mark_err) => {
                                            warn!(
                                                "scheduler failed to advance next_run_at after active-run conflict for task {}: {}",
                                                task.id, mark_err
                                            );
                                        }
                                    }
                                } else if let Err(mark_err) =
                                    task_service.mark_scheduled_run_failed(&task.id, &err).await
                                {
                                    warn!(
                                        "scheduler failed to persist start failure for task {}: {}",
                                        task.id, mark_err
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
