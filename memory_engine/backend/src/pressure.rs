// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Duration;

use chatos_config_sdk::{ConfigClient, ConfigSnapshot, ServicePressureSignal};
use chatos_queue_observability::{RabbitMqQueueRuntimeStats, RabbitMqQueueSpec};
use tokio::sync::watch;

use crate::state::AppState;

pub use chatos_config_sdk::PlatformPressureLevel;

const PLATFORM_PRESSURE_LEVEL_CONFIG_KEY: &str = "platform.pressure.level";
const PRESSURE_SUMMARY_CONCURRENCY_CONFIG_KEY: &str =
    "memory_engine.worker.pressure_summary_concurrency";
const PRESSURE_REFRESH_INTERVAL_MS_CONFIG_KEY: &str =
    "memory_engine.worker.pressure_refresh_interval_ms";
const PRESSURE_QUEUE_ELEVATED_MESSAGES_CONFIG_KEY: &str =
    "memory_engine.pressure.queue_elevated_messages";
const PRESSURE_QUEUE_CRITICAL_MESSAGES_CONFIG_KEY: &str =
    "memory_engine.pressure.queue_critical_messages";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryEnginePressurePolicy {
    pub level: PlatformPressureLevel,
    pub active_summary_concurrency: usize,
    pub reconcile_paused: bool,
    pub refresh_interval: Duration,
    pub queue_elevated_messages: u64,
    pub queue_critical_messages: u64,
}

impl MemoryEnginePressurePolicy {
    pub fn from_snapshot(
        snapshot: &ConfigSnapshot,
        normal_summary_concurrency: usize,
    ) -> Result<Self, String> {
        if normal_summary_concurrency == 0 {
            return Err("Memory Engine normal summary concurrency must be positive".to_string());
        }
        let level = snapshot
            .value(PLATFORM_PRESSURE_LEVEL_CONFIG_KEY)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                format!(
                    "{PLATFORM_PRESSURE_LEVEL_CONFIG_KEY} is required from configuration center"
                )
            })
            .and_then(PlatformPressureLevel::parse)?;
        let pressure_summary_concurrency = required_usize(
            snapshot,
            PRESSURE_SUMMARY_CONCURRENCY_CONFIG_KEY,
            1,
            normal_summary_concurrency,
        )?;
        let refresh_interval_ms = required_u64(
            snapshot,
            PRESSURE_REFRESH_INTERVAL_MS_CONFIG_KEY,
            1_000,
            300_000,
        )?;
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
        if queue_critical_messages <= queue_elevated_messages {
            return Err(format!(
                "{PRESSURE_QUEUE_CRITICAL_MESSAGES_CONFIG_KEY} must be greater than {PRESSURE_QUEUE_ELEVATED_MESSAGES_CONFIG_KEY}"
            ));
        }
        let active_summary_concurrency = match level {
            PlatformPressureLevel::Normal => normal_summary_concurrency,
            PlatformPressureLevel::Elevated | PlatformPressureLevel::Critical => {
                pressure_summary_concurrency
            }
        };
        Ok(Self {
            level,
            active_summary_concurrency,
            reconcile_paused: level == PlatformPressureLevel::Critical,
            refresh_interval: Duration::from_millis(refresh_interval_ms),
            queue_elevated_messages,
            queue_critical_messages,
        })
    }
}

#[derive(Clone)]
pub struct MemoryEnginePressureState {
    sender: watch::Sender<MemoryEnginePressurePolicy>,
}

impl MemoryEnginePressureState {
    pub fn new(initial: MemoryEnginePressurePolicy) -> Self {
        let (sender, _) = watch::channel(initial);
        Self { sender }
    }

    pub fn snapshot(&self) -> MemoryEnginePressurePolicy {
        self.sender.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<MemoryEnginePressurePolicy> {
        self.sender.subscribe()
    }

    fn update(&self, next: MemoryEnginePressurePolicy) -> bool {
        if *self.sender.borrow() == next {
            return false;
        }
        self.sender.send_replace(next);
        true
    }
}

pub fn start_config_watcher(state: Arc<AppState>, client: ConfigClient) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(state.pressure.snapshot().refresh_interval).await;
            let snapshot = match client.refresh().await {
                Ok(Some(snapshot)) => snapshot,
                Ok(None) => continue,
                Err(error) => {
                    tracing::warn!(
                        error = error.as_str(),
                        "Memory Engine pressure config refresh failed; keeping previous valid policy"
                    );
                    continue;
                }
            };
            let next = match MemoryEnginePressurePolicy::from_snapshot(
                &snapshot,
                state.config.worker_summary_concurrency,
            ) {
                Ok(next) => next,
                Err(error) => {
                    tracing::warn!(
                        revision = snapshot.revision,
                        error = error.as_str(),
                        "Memory Engine rejected invalid pressure config refresh"
                    );
                    continue;
                }
            };
            if state.pressure.update(next.clone()) {
                tracing::info!(
                    revision = snapshot.revision,
                    pressure_level = ?next.level,
                    active_summary_concurrency = next.active_summary_concurrency,
                    reconcile_paused = next.reconcile_paused,
                    "Memory Engine applied pressure policy from configuration center"
                );
            }
        }
    });
}

pub fn start_pressure_reporter(
    state: Arc<AppState>,
    client: ConfigClient,
    service_id: String,
    running_version: Option<String>,
) {
    tokio::spawn(async move {
        loop {
            let policy = state.pressure.snapshot();
            let stats = state
                .rabbitmq_queue_inspector
                .inspect(&[
                    RabbitMqQueueSpec::new("rollup", state.config.rollup_queue.as_str()),
                    RabbitMqQueueSpec::new(
                        "subject_memory",
                        state.config.subject_memory_queue.as_str(),
                    ),
                ])
                .await;
            let signal = pressure_signal_from_queue_stats(&stats, &policy);
            if let Err(error) = client
                .report_pressure(service_id.as_str(), running_version.as_deref(), &signal)
                .await
            {
                tracing::warn!(
                    error = error.as_str(),
                    "Memory Engine failed to report local pressure signal"
                );
            }
            tokio::time::sleep(policy.refresh_interval).await;
        }
    });
}

fn pressure_signal_from_queue_stats(
    stats: &RabbitMqQueueRuntimeStats,
    policy: &MemoryEnginePressurePolicy,
) -> ServicePressureSignal {
    if !stats.available {
        return ServicePressureSignal {
            level: PlatformPressureLevel::Critical,
            reason: "memory RabbitMQ queue inspection unavailable".to_string(),
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
            reason: format!("memory queue has no consumer; ready_messages={ready_messages}"),
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
        reason: format!("memory ready_messages={ready_messages}"),
    }
}

fn required_usize(
    snapshot: &ConfigSnapshot,
    key: &str,
    min: usize,
    max: usize,
) -> Result<usize, String> {
    let value = required_u64(snapshot, key, min as u64, max as u64)?;
    usize::try_from(value).map_err(|_| format!("{key} exceeds this platform's usize range"))
}

fn required_u64(snapshot: &ConfigSnapshot, key: &str, min: u64, max: u64) -> Result<u64, String> {
    let value = snapshot
        .value(key)
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| format!("{key} is required as an integer from configuration center"))?;
    if !(min..=max).contains(&value) {
        return Err(format!("{key} must be between {min} and {max}"));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::*;

    fn snapshot(
        level: &str,
        pressure_concurrency: u64,
        refresh_interval_ms: u64,
    ) -> ConfigSnapshot {
        ConfigSnapshot {
            environment: "test".to_string(),
            service_name: "memory-engine".to_string(),
            revision: 1,
            checksum: "checksum".to_string(),
            values: BTreeMap::from([
                (PLATFORM_PRESSURE_LEVEL_CONFIG_KEY.to_string(), json!(level)),
                (
                    PRESSURE_SUMMARY_CONCURRENCY_CONFIG_KEY.to_string(),
                    json!(pressure_concurrency),
                ),
                (
                    PRESSURE_REFRESH_INTERVAL_MS_CONFIG_KEY.to_string(),
                    json!(refresh_interval_ms),
                ),
                (
                    PRESSURE_QUEUE_ELEVATED_MESSAGES_CONFIG_KEY.to_string(),
                    json!(100),
                ),
                (
                    PRESSURE_QUEUE_CRITICAL_MESSAGES_CONFIG_KEY.to_string(),
                    json!(1_000),
                ),
            ]),
            env: BTreeMap::new(),
            generated_at: "now".to_string(),
            stale: false,
            source: Some("configuration_center".to_string()),
        }
    }

    #[test]
    fn normal_pressure_keeps_full_summary_concurrency() {
        let policy = MemoryEnginePressurePolicy::from_snapshot(&snapshot("normal", 1, 5_000), 4)
            .expect("normal pressure policy");
        assert_eq!(policy.level, PlatformPressureLevel::Normal);
        assert_eq!(policy.active_summary_concurrency, 4);
        assert!(!policy.reconcile_paused);
    }

    #[test]
    fn elevated_and_critical_pressure_reduce_summary_concurrency() {
        let elevated =
            MemoryEnginePressurePolicy::from_snapshot(&snapshot("elevated", 2, 5_000), 4)
                .expect("elevated pressure policy");
        let critical =
            MemoryEnginePressurePolicy::from_snapshot(&snapshot("critical", 1, 5_000), 4)
                .expect("critical pressure policy");
        assert_eq!(elevated.active_summary_concurrency, 2);
        assert!(!elevated.reconcile_paused);
        assert_eq!(critical.active_summary_concurrency, 1);
        assert!(critical.reconcile_paused);
    }

    #[test]
    fn pressure_policy_rejects_missing_invalid_or_excessive_values() {
        assert!(
            MemoryEnginePressurePolicy::from_snapshot(&snapshot("unknown", 1, 5_000), 4).is_err()
        );
        assert!(
            MemoryEnginePressurePolicy::from_snapshot(&snapshot("elevated", 5, 5_000), 4).is_err()
        );
        let mut missing = snapshot("normal", 1, 5_000);
        missing.values.remove(PLATFORM_PRESSURE_LEVEL_CONFIG_KEY);
        assert!(MemoryEnginePressurePolicy::from_snapshot(&missing, 4).is_err());
    }

    #[test]
    fn queue_signal_uses_managed_thresholds_and_detects_missing_consumers() {
        let policy = MemoryEnginePressurePolicy::from_snapshot(&snapshot("normal", 1, 5_000), 4)
            .expect("pressure policy");
        let stats = RabbitMqQueueRuntimeStats {
            enabled: true,
            available: true,
            queues: vec![chatos_queue_observability::RabbitMqQueueDepth {
                role: "rollup".to_string(),
                name: "rollup".to_string(),
                messages: 100,
                consumers: 1,
            }],
            error: None,
        };
        assert_eq!(
            pressure_signal_from_queue_stats(&stats, &policy).level,
            PlatformPressureLevel::Elevated
        );

        let stalled = RabbitMqQueueRuntimeStats {
            queues: vec![chatos_queue_observability::RabbitMqQueueDepth {
                consumers: 0,
                ..stats.queues[0].clone()
            }],
            ..stats
        };
        assert_eq!(
            pressure_signal_from_queue_stats(&stalled, &policy).level,
            PlatformPressureLevel::Critical
        );
    }
}
