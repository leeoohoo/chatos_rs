// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_config_sdk::{ConfigClient, ConfigSnapshot, ServicePressureSignal};
use chatos_queue_observability::{RabbitMqQueueRuntimeStats, RabbitMqQueueSpec};
use tokio::sync::watch;
use tokio::task::JoinHandle;

pub use chatos_config_sdk::PlatformPressureLevel;

use crate::state::AppState;

const PRESSURE_QUEUE_ELEVATED_MESSAGES_CONFIG_KEY: &str =
    "plugin_management.pressure.queue_elevated_messages";
const PRESSURE_QUEUE_CRITICAL_MESSAGES_CONFIG_KEY: &str =
    "plugin_management.pressure.queue_critical_messages";
const PRESSURE_REPORT_INTERVAL_MS_CONFIG_KEY: &str =
    "plugin_management.pressure.report_interval_ms";
const PLATFORM_PRESSURE_LEVEL_CONFIG_KEY: &str = "platform.pressure.level";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginManagementPressurePolicy {
    pub level: PlatformPressureLevel,
    pub queue_elevated_messages: u64,
    pub queue_critical_messages: u64,
    pub report_interval: Duration,
}

impl PluginManagementPressurePolicy {
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
pub struct PluginManagementPressureState {
    sender: watch::Sender<PluginManagementPressurePolicy>,
}

impl PluginManagementPressureState {
    pub fn new(initial: PluginManagementPressurePolicy) -> Self {
        let (sender, _) = watch::channel(initial);
        Self { sender }
    }

    pub fn snapshot(&self) -> PluginManagementPressurePolicy {
        self.sender.borrow().clone()
    }

    fn update(&self, next: PluginManagementPressurePolicy) -> bool {
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
    service_id: String,
    running_version: Option<String>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let policy = state.pressure.snapshot();
            let stats = inspect_main_queue(&state).await;
            let signal = pressure_signal_from_queue_stats(&stats, &policy);
            if let Err(error) = client
                .report_pressure(service_id.as_str(), running_version.as_deref(), &signal)
                .await
            {
                tracing::warn!(
                    error = error.as_str(),
                    "Plugin Management failed to report local pressure signal"
                );
            }

            tokio::time::sleep(policy.report_interval).await;
            let snapshot = match client.refresh().await {
                Ok(Some(snapshot)) => snapshot,
                Ok(None) => continue,
                Err(error) => {
                    tracing::warn!(
                        error = error.as_str(),
                        "Plugin Management pressure config refresh failed; keeping previous valid policy"
                    );
                    continue;
                }
            };
            let next = match PluginManagementPressurePolicy::from_snapshot(&snapshot) {
                Ok(next) => next,
                Err(error) => {
                    tracing::warn!(
                        revision = snapshot.revision,
                        error = error.as_str(),
                        "Plugin Management rejected invalid pressure config refresh"
                    );
                    continue;
                }
            };
            if state.pressure.update(next.clone()) {
                tracing::info!(
                    revision = snapshot.revision,
                    pressure_level = ?next.level,
                    queue_elevated_messages = next.queue_elevated_messages,
                    queue_critical_messages = next.queue_critical_messages,
                    report_interval_ms = next.report_interval.as_millis(),
                    "Plugin Management applied pressure policy from configuration center"
                );
            }
        }
    })
}

async fn inspect_main_queue(state: &AppState) -> RabbitMqQueueRuntimeStats {
    if !state.config.plugin_catalog_sync_enabled {
        return RabbitMqQueueRuntimeStats::disabled();
    }
    state
        .rabbitmq_queue_inspector
        .inspect(&[RabbitMqQueueSpec::new(
            "catalog_sync",
            state.config.plugin_catalog_queue.as_str(),
        )])
        .await
}

fn pressure_signal_from_queue_stats(
    stats: &RabbitMqQueueRuntimeStats,
    policy: &PluginManagementPressurePolicy,
) -> ServicePressureSignal {
    if !stats.enabled {
        return ServicePressureSignal {
            level: PlatformPressureLevel::Normal,
            reason: "Plugin catalog sync is disabled".to_string(),
        };
    }
    if !stats.available {
        return ServicePressureSignal {
            level: PlatformPressureLevel::Critical,
            reason: "Plugin catalog sync queue inspection unavailable".to_string(),
        };
    }
    let Some(queue) = stats
        .queues
        .iter()
        .find(|queue| queue.role == "catalog_sync")
    else {
        return ServicePressureSignal {
            level: PlatformPressureLevel::Critical,
            reason: "Plugin catalog sync queue stats missing".to_string(),
        };
    };
    if queue.messages > 0 && queue.consumers == 0 {
        return ServicePressureSignal {
            level: PlatformPressureLevel::Critical,
            reason: format!(
                "Plugin catalog sync queue has no consumer; ready_messages={}",
                queue.messages
            ),
        };
    }
    let ready_messages = u64::from(queue.messages);
    let level = if ready_messages >= policy.queue_critical_messages {
        PlatformPressureLevel::Critical
    } else if ready_messages >= policy.queue_elevated_messages {
        PlatformPressureLevel::Elevated
    } else {
        PlatformPressureLevel::Normal
    };
    ServicePressureSignal {
        level,
        reason: format!("Plugin catalog sync ready_messages={ready_messages}"),
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
            service_name: "plugin-management-service".to_string(),
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

    fn stats(messages: u32, consumers: u32) -> RabbitMqQueueRuntimeStats {
        RabbitMqQueueRuntimeStats {
            enabled: true,
            available: true,
            queues: vec![RabbitMqQueueDepth {
                role: "catalog_sync".to_string(),
                name: "plugin.catalog.sync".to_string(),
                messages,
                consumers,
            }],
            error: None,
        }
    }

    #[test]
    fn catalog_queue_depth_maps_to_normal_elevated_and_critical() {
        let policy = PluginManagementPressurePolicy::from_snapshot(&snapshot(100, 1_000, 5_000))
            .expect("valid pressure policy");
        assert_eq!(
            pressure_signal_from_queue_stats(&stats(99, 1), &policy).level,
            PlatformPressureLevel::Normal
        );
        assert_eq!(
            pressure_signal_from_queue_stats(&stats(100, 1), &policy).level,
            PlatformPressureLevel::Elevated
        );
        assert_eq!(
            pressure_signal_from_queue_stats(&stats(1_000, 1), &policy).level,
            PlatformPressureLevel::Critical
        );
    }

    #[test]
    fn disabled_sync_is_normal_but_unavailable_inspection_is_critical() {
        let policy = PluginManagementPressurePolicy::from_snapshot(&snapshot(100, 1_000, 5_000))
            .expect("valid pressure policy");
        assert_eq!(
            pressure_signal_from_queue_stats(&RabbitMqQueueRuntimeStats::disabled(), &policy).level,
            PlatformPressureLevel::Normal
        );
        assert_eq!(
            pressure_signal_from_queue_stats(
                &RabbitMqQueueRuntimeStats {
                    enabled: true,
                    available: false,
                    queues: Vec::new(),
                    error: Some("unavailable".to_string()),
                },
                &policy,
            )
            .level,
            PlatformPressureLevel::Critical
        );
    }

    #[test]
    fn backlog_without_a_consumer_is_critical() {
        let policy = PluginManagementPressurePolicy::from_snapshot(&snapshot(100, 1_000, 5_000))
            .expect("valid pressure policy");
        let signal = pressure_signal_from_queue_stats(&stats(1, 0), &policy);
        assert_eq!(signal.level, PlatformPressureLevel::Critical);
        assert!(signal.reason.contains("no consumer"));
    }

    #[test]
    fn invalid_configuration_center_values_are_rejected() {
        assert!(
            PluginManagementPressurePolicy::from_snapshot(&snapshot(1_000, 100, 5_000)).is_err()
        );
        assert!(PluginManagementPressurePolicy::from_snapshot(&snapshot(0, 1_000, 5_000)).is_err());
        assert!(PluginManagementPressurePolicy::from_snapshot(&snapshot(100, 1_000, 999)).is_err());
    }

    #[test]
    fn platform_pressure_level_is_required_and_strict() {
        let mut missing = snapshot(100, 1_000, 5_000);
        missing.values.remove(PLATFORM_PRESSURE_LEVEL_CONFIG_KEY);
        assert!(PluginManagementPressurePolicy::from_snapshot(&missing).is_err());

        let mut invalid = snapshot(100, 1_000, 5_000);
        invalid.values.insert(
            PLATFORM_PRESSURE_LEVEL_CONFIG_KEY.to_string(),
            json!("overloaded"),
        );
        assert!(PluginManagementPressurePolicy::from_snapshot(&invalid).is_err());
    }
}
