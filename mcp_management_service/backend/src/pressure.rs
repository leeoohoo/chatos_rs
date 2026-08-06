// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_config_sdk::{
    ConfigClient, ConfigSnapshot, PlatformPressureLevel, ServicePressureSignal,
};
use chatos_queue_observability::RabbitMqQueueRuntimeStats;
use tokio::task::JoinHandle;

use crate::state::AppState;

const PRESSURE_QUEUE_ELEVATED_PERCENT_CONFIG_KEY: &str =
    "mcp_management.pressure.queue_elevated_percent";
const PRESSURE_QUEUE_CRITICAL_PERCENT_CONFIG_KEY: &str =
    "mcp_management.pressure.queue_critical_percent";
const PRESSURE_REPORT_INTERVAL_MS_CONFIG_KEY: &str = "mcp_management.pressure.report_interval_ms";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpManagementPressurePolicy {
    pub queue_elevated_percent: u64,
    pub queue_critical_percent: u64,
    pub report_interval: Duration,
}

impl McpManagementPressurePolicy {
    pub fn from_snapshot(snapshot: &ConfigSnapshot) -> Result<Self, String> {
        let queue_elevated_percent =
            required_u64(snapshot, PRESSURE_QUEUE_ELEVATED_PERCENT_CONFIG_KEY, 1, 99)?;
        let queue_critical_percent =
            required_u64(snapshot, PRESSURE_QUEUE_CRITICAL_PERCENT_CONFIG_KEY, 1, 99)?;
        if queue_elevated_percent >= queue_critical_percent {
            return Err(format!(
                "{PRESSURE_QUEUE_ELEVATED_PERCENT_CONFIG_KEY} must be less than {PRESSURE_QUEUE_CRITICAL_PERCENT_CONFIG_KEY}"
            ));
        }
        let report_interval_ms = required_u64(
            snapshot,
            PRESSURE_REPORT_INTERVAL_MS_CONFIG_KEY,
            1_000,
            300_000,
        )?;
        Ok(Self {
            queue_elevated_percent,
            queue_critical_percent,
            report_interval: Duration::from_millis(report_interval_ms),
        })
    }
}

pub fn start_pressure_reporter(
    state: AppState,
    client: ConfigClient,
    initial_policy: McpManagementPressurePolicy,
    service_id: String,
    running_version: Option<String>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut policy = initial_policy;
        loop {
            let stats = state.async_tool_dispatch.rabbitmq_queue_stats().await;
            let signal = pressure_signal_from_queue_stats(
                &stats,
                state.config.async_tool_dispatch_topology.queue_max_length,
                &policy,
            );
            if let Err(error) = client
                .report_pressure(service_id.as_str(), running_version.as_deref(), &signal)
                .await
            {
                tracing::warn!(
                    error = error.as_str(),
                    "MCP Management failed to report local pressure signal"
                );
            }

            tokio::time::sleep(policy.report_interval).await;
            let snapshot = match client.refresh().await {
                Ok(Some(snapshot)) => snapshot,
                Ok(None) => continue,
                Err(error) => {
                    tracing::warn!(
                        error = error.as_str(),
                        "MCP Management pressure config refresh failed; keeping previous valid policy"
                    );
                    continue;
                }
            };
            let next = match McpManagementPressurePolicy::from_snapshot(&snapshot) {
                Ok(next) => next,
                Err(error) => {
                    tracing::warn!(
                        revision = snapshot.revision,
                        error = error.as_str(),
                        "MCP Management rejected invalid pressure config refresh"
                    );
                    continue;
                }
            };
            if next != policy {
                tracing::info!(
                    revision = snapshot.revision,
                    queue_elevated_percent = next.queue_elevated_percent,
                    queue_critical_percent = next.queue_critical_percent,
                    report_interval_ms = next.report_interval.as_millis(),
                    "MCP Management applied pressure policy from configuration center"
                );
                policy = next;
            }
        }
    })
}

fn pressure_signal_from_queue_stats(
    stats: &RabbitMqQueueRuntimeStats,
    queue_max_length: u32,
    policy: &McpManagementPressurePolicy,
) -> ServicePressureSignal {
    if !stats.available {
        return ServicePressureSignal {
            level: PlatformPressureLevel::Critical,
            reason: "MCP async dispatch queue inspection unavailable".to_string(),
        };
    }
    let Some(dispatch) = stats.queues.iter().find(|queue| queue.role == "dispatch") else {
        return ServicePressureSignal {
            level: PlatformPressureLevel::Critical,
            reason: "MCP async dispatch queue stats missing".to_string(),
        };
    };
    if dispatch.messages > 0 && dispatch.consumers == 0 {
        return ServicePressureSignal {
            level: PlatformPressureLevel::Critical,
            reason: format!(
                "MCP async dispatch queue has no consumer; ready_messages={}",
                dispatch.messages
            ),
        };
    }
    if queue_max_length == 0 {
        return ServicePressureSignal {
            level: PlatformPressureLevel::Critical,
            reason: "MCP async dispatch queue maximum length is invalid".to_string(),
        };
    }
    let utilization_percent = u64::from(dispatch.messages) * 100 / u64::from(queue_max_length);
    let level = if utilization_percent >= policy.queue_critical_percent {
        PlatformPressureLevel::Critical
    } else if utilization_percent >= policy.queue_elevated_percent {
        PlatformPressureLevel::Elevated
    } else {
        PlatformPressureLevel::Normal
    };
    ServicePressureSignal {
        level,
        reason: format!(
            "MCP async dispatch utilization_percent={utilization_percent}; ready_messages={}; queue_max_length={queue_max_length}",
            dispatch.messages
        ),
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
            service_name: "mcp-management-service".to_string(),
            revision: 1,
            checksum: "checksum".to_string(),
            values: BTreeMap::from([
                (
                    PRESSURE_QUEUE_ELEVATED_PERCENT_CONFIG_KEY.to_string(),
                    json!(elevated),
                ),
                (
                    PRESSURE_QUEUE_CRITICAL_PERCENT_CONFIG_KEY.to_string(),
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
                role: "dispatch".to_string(),
                name: "mcp_management.async.dispatch".to_string(),
                messages,
                consumers,
            }],
            error: None,
        }
    }

    #[test]
    fn queue_capacity_maps_to_normal_elevated_and_critical() {
        let policy = McpManagementPressurePolicy::from_snapshot(&snapshot(70, 90, 5_000))
            .expect("valid pressure policy");
        assert_eq!(
            pressure_signal_from_queue_stats(&stats(699, 1), 1_000, &policy).level,
            PlatformPressureLevel::Normal
        );
        assert_eq!(
            pressure_signal_from_queue_stats(&stats(700, 1), 1_000, &policy).level,
            PlatformPressureLevel::Elevated
        );
        assert_eq!(
            pressure_signal_from_queue_stats(&stats(900, 1), 1_000, &policy).level,
            PlatformPressureLevel::Critical
        );
    }

    #[test]
    fn ready_messages_without_a_consumer_are_critical() {
        let policy = McpManagementPressurePolicy::from_snapshot(&snapshot(70, 90, 5_000))
            .expect("valid pressure policy");
        let signal = pressure_signal_from_queue_stats(&stats(1, 0), 1_000, &policy);
        assert_eq!(signal.level, PlatformPressureLevel::Critical);
        assert!(signal.reason.contains("no consumer"));
    }

    #[test]
    fn unavailable_or_missing_dispatch_stats_are_critical() {
        let policy = McpManagementPressurePolicy::from_snapshot(&snapshot(70, 90, 5_000))
            .expect("valid pressure policy");
        let unavailable = RabbitMqQueueRuntimeStats {
            enabled: true,
            available: false,
            queues: Vec::new(),
            error: Some("inspection unavailable".to_string()),
        };
        assert_eq!(
            pressure_signal_from_queue_stats(&unavailable, 1_000, &policy).level,
            PlatformPressureLevel::Critical
        );
        let missing = RabbitMqQueueRuntimeStats {
            enabled: true,
            available: true,
            queues: Vec::new(),
            error: None,
        };
        assert_eq!(
            pressure_signal_from_queue_stats(&missing, 1_000, &policy).level,
            PlatformPressureLevel::Critical
        );
    }

    #[test]
    fn invalid_configuration_center_values_are_rejected() {
        assert!(McpManagementPressurePolicy::from_snapshot(&snapshot(90, 70, 5_000)).is_err());
        assert!(McpManagementPressurePolicy::from_snapshot(&snapshot(0, 90, 5_000)).is_err());
        assert!(McpManagementPressurePolicy::from_snapshot(&snapshot(70, 100, 5_000)).is_err());
        assert!(McpManagementPressurePolicy::from_snapshot(&snapshot(70, 90, 999)).is_err());
    }

    #[test]
    fn invalid_queue_capacity_is_critical_instead_of_being_normalized() {
        let policy = McpManagementPressurePolicy::from_snapshot(&snapshot(70, 90, 5_000))
            .expect("valid pressure policy");
        let signal = pressure_signal_from_queue_stats(&stats(0, 0), 0, &policy);
        assert_eq!(signal.level, PlatformPressureLevel::Critical);
        assert!(signal.reason.contains("maximum length is invalid"));
    }
}
