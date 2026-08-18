// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_config_sdk::{ConfigClient, ConfigSnapshot, ServicePressureSignal};
use chatos_queue_observability::{RabbitMqQueueRuntimeStats, RabbitMqQueueSpec};
use tokio::sync::watch;
use tokio::task::JoinHandle;

pub use chatos_config_sdk::PlatformPressureLevel;

use crate::platform_queue::TaskQueueMode;
use crate::state::AppState;

const PRESSURE_QUEUE_ELEVATED_MESSAGES_CONFIG_KEY: &str =
    "task_runner.pressure.queue_elevated_messages";
const PRESSURE_QUEUE_CRITICAL_MESSAGES_CONFIG_KEY: &str =
    "task_runner.pressure.queue_critical_messages";
const PRESSURE_REPORT_INTERVAL_MS_CONFIG_KEY: &str = "task_runner.pressure.report_interval_ms";
const PLATFORM_PRESSURE_LEVEL_CONFIG_KEY: &str = "platform.pressure.level";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRunnerPressurePolicy {
    pub level: PlatformPressureLevel,
    pub queue_elevated_messages: u64,
    pub queue_critical_messages: u64,
    pub report_interval: Duration,
}

impl TaskRunnerPressurePolicy {
    pub fn from_snapshot(snapshot: &ConfigSnapshot) -> Result<Self, String> {
        let level = snapshot
            .value(PLATFORM_PRESSURE_LEVEL_CONFIG_KEY)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                format!(
                    "{PLATFORM_PRESSURE_LEVEL_CONFIG_KEY} is required from configuration center"
                )
            })
            .and_then(PlatformPressureLevel::parse)?;
        let queue_elevated_messages = required_u64(
            snapshot,
            PRESSURE_QUEUE_ELEVATED_MESSAGES_CONFIG_KEY,
            1,
            10_000_000,
        )?;
        let queue_critical_messages = required_u64(
            snapshot,
            PRESSURE_QUEUE_CRITICAL_MESSAGES_CONFIG_KEY,
            2,
            100_000_000,
        )?;
        if queue_elevated_messages >= queue_critical_messages {
            return Err(format!(
                "{PRESSURE_QUEUE_ELEVATED_MESSAGES_CONFIG_KEY} must be less than {PRESSURE_QUEUE_CRITICAL_MESSAGES_CONFIG_KEY}"
            ));
        }
        let report_interval_ms = required_u64(
            snapshot,
            PRESSURE_REPORT_INTERVAL_MS_CONFIG_KEY,
            1_000,
            300_000,
        )?;
        Ok(Self {
            level,
            queue_elevated_messages,
            queue_critical_messages,
            report_interval: Duration::from_millis(report_interval_ms),
        })
    }
}

#[derive(Clone)]
pub struct TaskRunnerPressureState {
    sender: watch::Sender<TaskRunnerPressurePolicy>,
}

impl TaskRunnerPressureState {
    pub fn new(initial: TaskRunnerPressurePolicy) -> Self {
        let (sender, _) = watch::channel(initial);
        Self { sender }
    }

    pub fn snapshot(&self) -> TaskRunnerPressurePolicy {
        self.sender.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<TaskRunnerPressurePolicy> {
        self.sender.subscribe()
    }

    fn update(&self, next: TaskRunnerPressurePolicy) -> bool {
        if *self.sender.borrow() == next {
            return false;
        }
        self.sender.send_replace(next);
        true
    }
}

pub fn start_pressure_reporter(
    state: AppState,
    client: ConfigClient,
    pressure: TaskRunnerPressureState,
    service_id: String,
    running_version: Option<String>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let policy = pressure.snapshot();
            let stats = inspect_main_queues(&state).await;
            let signal = pressure_signal_from_queue_stats(&stats, &policy);
            if let Err(error) = client
                .report_pressure(service_id.as_str(), running_version.as_deref(), &signal)
                .await
            {
                tracing::warn!(
                    error = error.as_str(),
                    "Task Runner failed to report local pressure signal"
                );
            }

            tokio::time::sleep(policy.report_interval).await;
            let snapshot = match client.refresh().await {
                Ok(Some(snapshot)) => snapshot,
                Ok(None) => continue,
                Err(error) => {
                    tracing::warn!(
                        error = error.as_str(),
                        "Task Runner pressure config refresh failed; keeping previous valid policy"
                    );
                    continue;
                }
            };
            let next = match TaskRunnerPressurePolicy::from_snapshot(&snapshot) {
                Ok(next) => next,
                Err(error) => {
                    tracing::warn!(
                        revision = snapshot.revision,
                        error = error.as_str(),
                        "Task Runner rejected invalid pressure config refresh"
                    );
                    continue;
                }
            };
            if pressure.update(next.clone()) {
                tracing::info!(
                    revision = snapshot.revision,
                    pressure_level = ?next.level,
                    queue_elevated_messages = next.queue_elevated_messages,
                    queue_critical_messages = next.queue_critical_messages,
                    report_interval_ms = next.report_interval.as_millis(),
                    "Task Runner applied pressure policy from configuration center"
                );
            }
        }
    })
}

async fn inspect_main_queues(state: &AppState) -> RabbitMqQueueRuntimeStats {
    let Some(inspector) = state.rabbitmq_queue_inspector.as_ref() else {
        return RabbitMqQueueRuntimeStats::disabled();
    };
    let topology = &state.task_queue_topology;
    let mut specs = vec![
        RabbitMqQueueSpec::new(
            "cloud_agent_runtime",
            crate::cloud_agent_queue::TASK_RUNNER_CLOUD_AGENT_ROUTING_KEY,
        ),
        RabbitMqQueueSpec::new("run_post_process", topology.run_post_process_queue.as_str()),
    ];
    if state.config.callback_delivery_enabled()
        && topology.callback_delivery_mode == TaskQueueMode::RabbitMq
    {
        specs.push(RabbitMqQueueSpec::new(
            "callback_delivery",
            topology.callback_delivery_queue.as_str(),
        ));
    }
    inspector.inspect(specs.as_slice()).await
}

fn pressure_signal_from_queue_stats(
    stats: &RabbitMqQueueRuntimeStats,
    policy: &TaskRunnerPressurePolicy,
) -> ServicePressureSignal {
    if !stats.available {
        return ServicePressureSignal {
            level: PlatformPressureLevel::Critical,
            reason: "Task Runner main queue inspection unavailable".to_string(),
        };
    }
    let ready_messages = stats
        .queues
        .iter()
        .map(|queue| u64::from(queue.messages))
        .sum::<u64>();
    if stats
        .queues
        .iter()
        .any(|queue| queue.messages > 0 && queue.consumers == 0)
    {
        return ServicePressureSignal {
            level: PlatformPressureLevel::Critical,
            reason: format!(
                "Task Runner main queue has no consumer; ready_messages={ready_messages}"
            ),
        };
    }
    let level = if ready_messages >= policy.queue_critical_messages {
        PlatformPressureLevel::Critical
    } else if ready_messages >= policy.queue_elevated_messages {
        PlatformPressureLevel::Elevated
    } else {
        PlatformPressureLevel::Normal
    };
    ServicePressureSignal {
        level,
        reason: format!("Task Runner main queue ready_messages={ready_messages}"),
    }
}

fn required_u64(
    snapshot: &ConfigSnapshot,
    key: &str,
    minimum: u64,
    maximum: u64,
) -> Result<u64, String> {
    let value = snapshot
        .u64(key)
        .ok_or_else(|| format!("{key} is required from configuration center"))?;
    if !(minimum..=maximum).contains(&value) {
        return Err(format!("{key} must be between {minimum} and {maximum}"));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chatos_queue_observability::RabbitMqQueueDepth;
    use serde_json::json;

    use super::*;

    fn snapshot(elevated: u64, critical: u64, interval_ms: u64) -> ConfigSnapshot {
        ConfigSnapshot {
            environment: "test".to_string(),
            service_name: "task-runner".to_string(),
            revision: 1,
            checksum: "checksum".to_string(),
            values: BTreeMap::from([
                (
                    PLATFORM_PRESSURE_LEVEL_CONFIG_KEY.to_string(),
                    json!("normal"),
                ),
                (
                    PRESSURE_QUEUE_ELEVATED_MESSAGES_CONFIG_KEY.to_string(),
                    json!(elevated),
                ),
                (
                    PRESSURE_QUEUE_CRITICAL_MESSAGES_CONFIG_KEY.to_string(),
                    json!(critical),
                ),
                (
                    PRESSURE_REPORT_INTERVAL_MS_CONFIG_KEY.to_string(),
                    json!(interval_ms),
                ),
            ]),
            env: BTreeMap::new(),
            generated_at: "now".to_string(),
            stale: false,
            source: Some("configuration_center".to_string()),
        }
    }

    fn stats(queues: &[(&str, u32, u32)]) -> RabbitMqQueueRuntimeStats {
        RabbitMqQueueRuntimeStats {
            enabled: true,
            available: true,
            queues: queues
                .iter()
                .map(|(role, messages, consumers)| RabbitMqQueueDepth {
                    role: (*role).to_string(),
                    name: format!("task_runner.{role}"),
                    messages: *messages,
                    consumers: *consumers,
                })
                .collect(),
            error: None,
        }
    }

    #[test]
    fn main_queue_totals_map_to_normal_elevated_and_critical() {
        let policy = TaskRunnerPressurePolicy::from_snapshot(&snapshot(100, 1_000, 5_000))
            .expect("valid pressure policy");
        assert_eq!(
            pressure_signal_from_queue_stats(
                &stats(&[("cloud_agent_runtime", 50, 1), ("run_post_process", 49, 1)]),
                &policy,
            )
            .level,
            PlatformPressureLevel::Normal
        );
        assert_eq!(
            pressure_signal_from_queue_stats(
                &stats(&[("cloud_agent_runtime", 60, 1), ("run_post_process", 40, 1)]),
                &policy,
            )
            .level,
            PlatformPressureLevel::Elevated
        );
        assert_eq!(
            pressure_signal_from_queue_stats(
                &stats(&[
                    ("cloud_agent_runtime", 900, 1),
                    ("run_post_process", 100, 1)
                ]),
                &policy,
            )
            .level,
            PlatformPressureLevel::Critical
        );
    }

    #[test]
    fn any_backlogged_main_queue_without_a_consumer_is_critical() {
        let policy = TaskRunnerPressurePolicy::from_snapshot(&snapshot(100, 1_000, 5_000))
            .expect("valid pressure policy");
        let signal = pressure_signal_from_queue_stats(
            &stats(&[("cloud_agent_runtime", 1, 0), ("run_post_process", 0, 0)]),
            &policy,
        );
        assert_eq!(signal.level, PlatformPressureLevel::Critical);
        assert!(signal.reason.contains("no consumer"));
    }

    #[test]
    fn unavailable_queue_inspection_is_critical() {
        let policy = TaskRunnerPressurePolicy::from_snapshot(&snapshot(100, 1_000, 5_000))
            .expect("valid pressure policy");
        let signal = pressure_signal_from_queue_stats(
            &RabbitMqQueueRuntimeStats {
                enabled: true,
                available: false,
                queues: Vec::new(),
                error: Some("unavailable".to_string()),
            },
            &policy,
        );
        assert_eq!(signal.level, PlatformPressureLevel::Critical);
    }

    #[test]
    fn invalid_configuration_center_values_are_rejected() {
        assert!(TaskRunnerPressurePolicy::from_snapshot(&snapshot(1_000, 100, 5_000)).is_err());
        assert!(TaskRunnerPressurePolicy::from_snapshot(&snapshot(0, 1_000, 5_000)).is_err());
        assert!(TaskRunnerPressurePolicy::from_snapshot(&snapshot(100, 1_000, 999)).is_err());
    }
}
