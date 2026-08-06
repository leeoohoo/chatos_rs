// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_config_sdk::{ConfigClient, ConfigSnapshot, ServicePressureSignal};
use tokio::sync::watch;
use tokio::task::JoinHandle;

pub use chatos_config_sdk::PlatformPressureLevel;

use crate::models::LocalConnectorRelayStats;
use crate::state::AppState;

const PRESSURE_PENDING_RELAY_ELEVATED_CONFIG_KEY: &str =
    "local_connector.pressure.pending_relay_elevated_requests";
const PRESSURE_PENDING_RELAY_CRITICAL_CONFIG_KEY: &str =
    "local_connector.pressure.pending_relay_critical_requests";
const PRESSURE_REPORT_INTERVAL_MS_CONFIG_KEY: &str = "local_connector.pressure.report_interval_ms";
const PLATFORM_PRESSURE_LEVEL_CONFIG_KEY: &str = "platform.pressure.level";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalConnectorPressurePolicy {
    pub level: PlatformPressureLevel,
    pub pending_relay_elevated: usize,
    pub pending_relay_critical: usize,
    pub report_interval: Duration,
}

impl LocalConnectorPressurePolicy {
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
        let pending_relay_elevated = required_usize(
            snapshot,
            PRESSURE_PENDING_RELAY_ELEVATED_CONFIG_KEY,
            1,
            10_000_000,
        )?;
        let pending_relay_critical = required_usize(
            snapshot,
            PRESSURE_PENDING_RELAY_CRITICAL_CONFIG_KEY,
            2,
            100_000_000,
        )?;
        if pending_relay_elevated >= pending_relay_critical {
            return Err(format!(
                "{PRESSURE_PENDING_RELAY_ELEVATED_CONFIG_KEY} must be less than {PRESSURE_PENDING_RELAY_CRITICAL_CONFIG_KEY}"
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
            pending_relay_elevated,
            pending_relay_critical,
            report_interval: Duration::from_millis(report_interval_ms),
        })
    }
}

#[derive(Clone)]
pub struct LocalConnectorPressureState {
    sender: watch::Sender<LocalConnectorPressurePolicy>,
}

impl LocalConnectorPressureState {
    pub fn new(initial: LocalConnectorPressurePolicy) -> Self {
        let (sender, _) = watch::channel(initial);
        Self { sender }
    }

    pub fn snapshot(&self) -> LocalConnectorPressurePolicy {
        self.sender.borrow().clone()
    }

    fn update(&self, next: LocalConnectorPressurePolicy) -> bool {
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
            let stats = state.relay.stats().await;
            let signal = pressure_signal_from_relay_stats(&stats, &policy);
            if let Err(error) = client
                .report_pressure(service_id.as_str(), running_version.as_deref(), &signal)
                .await
            {
                tracing::warn!(
                    error = error.as_str(),
                    "Local Connector failed to report local pressure signal"
                );
            }

            tokio::time::sleep(policy.report_interval).await;
            let snapshot = match client.refresh().await {
                Ok(Some(snapshot)) => snapshot,
                Ok(None) => continue,
                Err(error) => {
                    tracing::warn!(
                        error = error.as_str(),
                        "Local Connector pressure config refresh failed; keeping previous valid policy"
                    );
                    continue;
                }
            };
            let next = match LocalConnectorPressurePolicy::from_snapshot(&snapshot) {
                Ok(next) => next,
                Err(error) => {
                    tracing::warn!(
                        revision = snapshot.revision,
                        error = error.as_str(),
                        "Local Connector rejected invalid pressure config refresh"
                    );
                    continue;
                }
            };
            if state.pressure.update(next.clone()) {
                state.relay.set_platform_pressure_level(next.level);
                tracing::info!(
                    revision = snapshot.revision,
                    pressure_level = ?next.level,
                    pending_relay_elevated = next.pending_relay_elevated,
                    pending_relay_critical = next.pending_relay_critical,
                    report_interval_ms = next.report_interval.as_millis(),
                    "Local Connector applied pressure policy from configuration center"
                );
            }
        }
    })
}

fn pressure_signal_from_relay_stats(
    stats: &LocalConnectorRelayStats,
    policy: &LocalConnectorPressurePolicy,
) -> ServicePressureSignal {
    let terminal_critical = stats.terminal_sessions >= stats.terminal_max_active_sessions;
    let relay_critical = stats.pending_relay_requests >= policy.pending_relay_critical;
    let terminal_elevated = stats.terminal_sessions >= stats.terminal_new_session_soft_limit;
    let relay_elevated = stats.pending_relay_requests >= policy.pending_relay_elevated;
    let level = if terminal_critical || relay_critical {
        PlatformPressureLevel::Critical
    } else if terminal_elevated || relay_elevated {
        PlatformPressureLevel::Elevated
    } else {
        PlatformPressureLevel::Normal
    };
    ServicePressureSignal {
        level,
        reason: format!(
            "Local Connector terminal_sessions={}/{} soft={}; pending_relay_requests={}",
            stats.terminal_sessions,
            stats.terminal_max_active_sessions,
            stats.terminal_new_session_soft_limit,
            stats.pending_relay_requests
        ),
    }
}

fn required_usize(
    snapshot: &ConfigSnapshot,
    key: &str,
    minimum: usize,
    maximum: usize,
) -> Result<usize, String> {
    let value = snapshot
        .usize(key)
        .ok_or_else(|| format!("{key} is required from configuration center"))?;
    if !(minimum..=maximum).contains(&value) {
        return Err(format!("{key} must be between {minimum} and {maximum}"));
    }
    Ok(value)
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

    use serde_json::json;

    use super::*;

    fn snapshot(elevated: u64, critical: u64, interval_ms: u64) -> ConfigSnapshot {
        ConfigSnapshot {
            environment: "test".to_string(),
            service_name: "local-connector-service".to_string(),
            revision: 1,
            checksum: "checksum".to_string(),
            values: BTreeMap::from([
                (
                    PLATFORM_PRESSURE_LEVEL_CONFIG_KEY.to_string(),
                    json!("normal"),
                ),
                (
                    PRESSURE_PENDING_RELAY_ELEVATED_CONFIG_KEY.to_string(),
                    json!(elevated),
                ),
                (
                    PRESSURE_PENDING_RELAY_CRITICAL_CONFIG_KEY.to_string(),
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

    fn stats(terminal_sessions: usize, pending_relay_requests: usize) -> LocalConnectorRelayStats {
        LocalConnectorRelayStats {
            active_device_sessions: 1,
            pending_relay_requests,
            terminal_sessions,
            terminal_ws_subscribers: 0,
            max_pending_requests_per_device: 256,
            terminal_max_event_bytes: 131_072,
            terminal_event_channel_capacity: 1_024,
            terminal_max_active_sessions: 10_000,
            terminal_new_session_soft_limit: 8_000,
            new_terminal_sessions_paused: terminal_sessions >= 8_000,
            terminal_max_subscribers_per_session: 64,
            relay_signing_enabled: true,
        }
    }

    #[test]
    fn terminal_soft_and_hard_limits_map_to_elevated_and_critical() {
        let policy = LocalConnectorPressurePolicy::from_snapshot(&snapshot(1_000, 5_000, 5_000))
            .expect("valid pressure policy");
        assert_eq!(
            pressure_signal_from_relay_stats(&stats(7_999, 0), &policy).level,
            PlatformPressureLevel::Normal
        );
        assert_eq!(
            pressure_signal_from_relay_stats(&stats(8_000, 0), &policy).level,
            PlatformPressureLevel::Elevated
        );
        assert_eq!(
            pressure_signal_from_relay_stats(&stats(10_000, 0), &policy).level,
            PlatformPressureLevel::Critical
        );
    }

    #[test]
    fn pending_relay_thresholds_map_to_elevated_and_critical() {
        let policy = LocalConnectorPressurePolicy::from_snapshot(&snapshot(1_000, 5_000, 5_000))
            .expect("valid pressure policy");
        assert_eq!(
            pressure_signal_from_relay_stats(&stats(0, 1_000), &policy).level,
            PlatformPressureLevel::Elevated
        );
        assert_eq!(
            pressure_signal_from_relay_stats(&stats(0, 5_000), &policy).level,
            PlatformPressureLevel::Critical
        );
    }

    #[test]
    fn invalid_configuration_center_values_are_rejected() {
        assert!(
            LocalConnectorPressurePolicy::from_snapshot(&snapshot(5_000, 1_000, 5_000)).is_err()
        );
        assert!(LocalConnectorPressurePolicy::from_snapshot(&snapshot(0, 5_000, 5_000)).is_err());
        assert!(LocalConnectorPressurePolicy::from_snapshot(&snapshot(1_000, 5_000, 999)).is_err());
    }

    #[test]
    fn platform_pressure_level_is_required_and_strict() {
        let mut missing = snapshot(1_000, 5_000, 5_000);
        missing.values.remove(PLATFORM_PRESSURE_LEVEL_CONFIG_KEY);
        assert!(LocalConnectorPressurePolicy::from_snapshot(&missing).is_err());

        let mut invalid = snapshot(1_000, 5_000, 5_000);
        invalid.values.insert(
            PLATFORM_PRESSURE_LEVEL_CONFIG_KEY.to_string(),
            json!("overloaded"),
        );
        assert!(LocalConnectorPressurePolicy::from_snapshot(&invalid).is_err());
    }
}
