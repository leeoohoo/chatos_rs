// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::json;
use tokio::sync::Mutex;

use super::*;

#[derive(Debug, Clone)]
struct PressureControllerPolicy {
    enabled: bool,
    configured_level: PlatformPressureLevel,
    interval: Duration,
    signal_ttl: Duration,
    escalation_stable: Duration,
    recovery_stable: Duration,
}

impl PressureControllerPolicy {
    fn from_values(values: &BTreeMap<String, Value>) -> Result<Self, String> {
        let enabled = required_bool(values, PLATFORM_PRESSURE_CONTROLLER_ENABLED_CONFIG_KEY)?;
        let configured_level = required_string(values, PLATFORM_PRESSURE_LEVEL_CONFIG_KEY)
            .and_then(PlatformPressureLevel::parse)?;
        let interval_ms = required_u64(
            values,
            PLATFORM_PRESSURE_CONTROLLER_INTERVAL_MS_CONFIG_KEY,
            1_000,
            60_000,
        )?;
        let signal_ttl_seconds = required_u64(
            values,
            PLATFORM_PRESSURE_SIGNAL_TTL_SECONDS_CONFIG_KEY,
            5,
            600,
        )?;
        let escalation_stable_seconds = required_u64(
            values,
            PLATFORM_PRESSURE_ESCALATION_STABLE_SECONDS_CONFIG_KEY,
            0,
            300,
        )?;
        let recovery_stable_seconds = required_u64(
            values,
            PLATFORM_PRESSURE_RECOVERY_STABLE_SECONDS_CONFIG_KEY,
            5,
            3_600,
        )?;
        let interval = Duration::from_millis(interval_ms);
        let signal_ttl = Duration::from_secs(signal_ttl_seconds);
        if signal_ttl <= interval {
            return Err(format!(
                "{PLATFORM_PRESSURE_SIGNAL_TTL_SECONDS_CONFIG_KEY} must exceed {PLATFORM_PRESSURE_CONTROLLER_INTERVAL_MS_CONFIG_KEY}"
            ));
        }
        Ok(Self {
            enabled,
            configured_level,
            interval,
            signal_ttl,
            escalation_stable: Duration::from_secs(escalation_stable_seconds),
            recovery_stable: Duration::from_secs(recovery_stable_seconds),
        })
    }
}

#[derive(Debug, Default)]
struct TransitionTracker {
    candidate: Option<PlatformPressureLevel>,
    since: Option<DateTime<Utc>>,
}

impl TransitionTracker {
    fn reset(&mut self) {
        self.candidate = None;
        self.since = None;
    }

    fn ready(
        &mut self,
        current: PlatformPressureLevel,
        candidate: PlatformPressureLevel,
        now: DateTime<Utc>,
        policy: &PressureControllerPolicy,
    ) -> bool {
        if candidate == current {
            self.reset();
            return false;
        }
        if self.candidate != Some(candidate) {
            self.candidate = Some(candidate);
            self.since = Some(now);
        }
        let required = if candidate > current {
            policy.escalation_stable
        } else {
            policy.recovery_stable
        };
        let elapsed = self
            .since
            .and_then(|since| (now - since).to_std().ok())
            .unwrap_or_default();
        elapsed >= required
    }
}

pub async fn start(state: AppState) -> Result<(), String> {
    let environment = state.config.default_environment.clone();
    let effective = state.effective(environment.as_str()).await?;
    let policy = PressureControllerPolicy::from_values(&effective.values)?;
    if state
        .store
        .get_pressure_state(environment.as_str())
        .await?
        .is_none()
    {
        state
            .store
            .upsert_pressure_state(&PlatformPressureStateRecord {
                id: environment.clone(),
                environment: environment.clone(),
                level: policy.configured_level,
                contributors: Vec::new(),
                reason: "configuration_release".to_string(),
                updated_at: Utc::now().to_rfc3339(),
            })
            .await?;
    }

    tokio::spawn(run(
        state,
        environment,
        policy,
        Arc::new(Mutex::new(TransitionTracker::default())),
    ));
    Ok(())
}

pub async fn status(state: &AppState, environment: &str) -> Result<Value, String> {
    let effective = state.effective(environment).await?;
    let policy = PressureControllerPolicy::from_values(&effective.values)?;
    let current = state
        .store
        .get_pressure_state(environment)
        .await?
        .ok_or_else(|| "authoritative platform pressure state is missing".to_string())?;
    let now = Utc::now();
    let signals = state
        .store
        .list_instances()
        .await?
        .into_iter()
        .filter(|instance| instance.environment == environment)
        .filter_map(|instance| {
            let signal = instance.pressure?;
            let last_seen = DateTime::parse_from_rfc3339(instance.last_seen_at.as_str())
                .ok()?
                .with_timezone(&Utc);
            let age = (now - last_seen).to_std().ok()?;
            Some(json!({
                "service_name": instance.service_name,
                "service_id": instance.service_id,
                "level": signal.level.as_str(),
                "reason": signal.reason,
                "last_seen_at": instance.last_seen_at,
                "fresh": age <= policy.signal_ttl,
            }))
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "environment": environment,
        "level": current.level.as_str(),
        "reason": current.reason,
        "contributors": current.contributors,
        "updated_at": current.updated_at,
        "controller": {
            "enabled": policy.enabled,
            "interval_ms": policy.interval.as_millis(),
            "signal_ttl_seconds": policy.signal_ttl.as_secs(),
            "escalation_stable_seconds": policy.escalation_stable.as_secs(),
            "recovery_stable_seconds": policy.recovery_stable.as_secs(),
        },
        "signals": signals,
    }))
}

async fn run(
    state: AppState,
    environment: String,
    mut policy: PressureControllerPolicy,
    tracker: Arc<Mutex<TransitionTracker>>,
) {
    loop {
        tokio::time::sleep(policy.interval).await;
        match state.effective(environment.as_str()).await {
            Ok(effective) => match PressureControllerPolicy::from_values(&effective.values) {
                Ok(next) => policy = next,
                Err(error) => tracing::warn!(
                    environment = environment.as_str(),
                    error = error.as_str(),
                    "Configuration Center rejected invalid pressure controller policy; keeping previous valid policy"
                ),
            },
            Err(error) => tracing::warn!(
                environment = environment.as_str(),
                error = error.as_str(),
                "Configuration Center could not refresh pressure controller policy"
            ),
        }

        if let Err(error) = reconcile(&state, environment.as_str(), &policy, &tracker).await {
            tracing::warn!(
                environment = environment.as_str(),
                error = error.as_str(),
                "Configuration Center pressure controller reconcile failed"
            );
        }
    }
}

async fn reconcile(
    state: &AppState,
    environment: &str,
    policy: &PressureControllerPolicy,
    tracker: &Arc<Mutex<TransitionTracker>>,
) -> Result<(), String> {
    let current = state
        .store
        .get_pressure_state(environment)
        .await?
        .ok_or_else(|| "authoritative platform pressure state is missing".to_string())?;
    let now = Utc::now();
    let (candidate, contributors, reason) = if policy.enabled {
        aggregate_candidate(
            state.store.list_instances().await?.as_slice(),
            environment,
            now,
            policy.signal_ttl,
        )
    } else {
        (
            policy.configured_level,
            Vec::new(),
            "pressure_controller_disabled".to_string(),
        )
    };

    let should_transition = if policy.enabled {
        tracker
            .lock()
            .await
            .ready(current.level, candidate, now, policy)
    } else {
        tracker.lock().await.reset();
        current.level != candidate
    };
    if !should_transition {
        return Ok(());
    }

    let next = PlatformPressureStateRecord {
        id: environment.to_string(),
        environment: environment.to_string(),
        level: candidate,
        contributors: contributors.clone(),
        reason: reason.clone(),
        updated_at: now.to_rfc3339(),
    };
    if !state
        .store
        .replace_pressure_state_if_level(environment, current.level, &next)
        .await?
    {
        tracker.lock().await.reset();
        return Ok(());
    }
    tracker.lock().await.reset();
    state
        .audit(
            Some(environment),
            "platform.pressure.transition",
            &system_user(),
            None,
            vec![PLATFORM_PRESSURE_LEVEL_CONFIG_KEY.to_string()],
            Some(json!({
                "from": current.level.as_str(),
                "to": candidate.as_str(),
                "contributors": contributors,
                "reason": reason,
            })),
        )
        .await?;
    tracing::warn!(
        environment,
        from = current.level.as_str(),
        to = candidate.as_str(),
        "Configuration Center changed the authoritative platform pressure level"
    );
    Ok(())
}

fn aggregate_candidate(
    instances: &[ServiceInstanceRecord],
    environment: &str,
    now: DateTime<Utc>,
    signal_ttl: Duration,
) -> (PlatformPressureLevel, Vec<String>, String) {
    let mut level = PlatformPressureLevel::Normal;
    let mut contributors = Vec::new();
    let mut reasons = Vec::new();
    for instance in instances
        .iter()
        .filter(|instance| instance.environment == environment)
    {
        let Some(signal) = instance.pressure.as_ref() else {
            continue;
        };
        let Some(last_seen) = DateTime::parse_from_rfc3339(instance.last_seen_at.as_str())
            .ok()
            .map(|value| value.with_timezone(&Utc))
        else {
            continue;
        };
        let Some(age) = (now - last_seen).to_std().ok() else {
            continue;
        };
        if age > signal_ttl {
            continue;
        }
        if signal.level > level {
            level = signal.level;
            contributors.clear();
            reasons.clear();
        }
        if signal.level == level {
            contributors.push(format!("{}:{}", instance.service_name, instance.service_id));
            reasons.push(format!("{}: {}", instance.service_name, signal.reason));
        }
    }
    contributors.sort();
    contributors.dedup();
    reasons.sort();
    reasons.dedup();
    let reason = if reasons.is_empty() {
        "no_active_pressure_signals".to_string()
    } else {
        reasons.join("; ")
    };
    (level, contributors, reason)
}

fn required_bool(values: &BTreeMap<String, Value>, key: &str) -> Result<bool, String> {
    values
        .get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("{key} is required as a boolean from configuration center"))
}

fn required_string<'a>(values: &'a BTreeMap<String, Value>, key: &str) -> Result<&'a str, String> {
    values
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{key} is required as a string from configuration center"))
}

fn required_u64(
    values: &BTreeMap<String, Value>,
    key: &str,
    min: u64,
    max: u64,
) -> Result<u64, String> {
    let value = values
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{key} is required as an integer from configuration center"))?;
    if !(min..=max).contains(&value) {
        return Err(format!("{key} must be between {min} and {max}"));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use chatos_config_sdk::ServicePressureSignal;
    use chrono::TimeDelta;

    use super::*;

    fn policy() -> PressureControllerPolicy {
        PressureControllerPolicy {
            enabled: true,
            configured_level: PlatformPressureLevel::Normal,
            interval: Duration::from_secs(5),
            signal_ttl: Duration::from_secs(30),
            escalation_stable: Duration::from_secs(5),
            recovery_stable: Duration::from_secs(60),
        }
    }

    fn instance(
        service: &str,
        level: PlatformPressureLevel,
        reason: &str,
        seen_at: DateTime<Utc>,
    ) -> ServiceInstanceRecord {
        ServiceInstanceRecord {
            id: format!("local:{service}:one"),
            environment: "local".to_string(),
            service_name: service.to_string(),
            service_id: "one".to_string(),
            running_version: None,
            effective_revision: 1,
            effective_checksum: "checksum".to_string(),
            stale: false,
            pending_restart_keys: Vec::new(),
            emergency_override_keys: Vec::new(),
            last_error: None,
            pressure: Some(ServicePressureSignal {
                level,
                reason: reason.to_string(),
            }),
            last_seen_at: seen_at.to_rfc3339(),
        }
    }

    #[test]
    fn aggregation_uses_the_highest_fresh_signal_and_ignores_stale_instances() {
        let now = Utc::now();
        let instances = vec![
            instance(
                "memory-engine",
                PlatformPressureLevel::Elevated,
                "summary backlog",
                now,
            ),
            instance(
                "mcp-management-service",
                PlatformPressureLevel::Critical,
                "queue unavailable",
                now - TimeDelta::seconds(31),
            ),
        ];

        let (level, contributors, reason) =
            aggregate_candidate(&instances, "local", now, Duration::from_secs(30));

        assert_eq!(level, PlatformPressureLevel::Elevated);
        assert_eq!(contributors, vec!["memory-engine:one"]);
        assert!(reason.contains("summary backlog"));
    }

    #[test]
    fn transition_tracker_applies_fast_escalation_and_slow_recovery_windows() {
        let now = Utc::now();
        let policy = policy();
        let mut tracker = TransitionTracker::default();
        assert!(!tracker.ready(
            PlatformPressureLevel::Normal,
            PlatformPressureLevel::Critical,
            now,
            &policy,
        ));
        assert!(tracker.ready(
            PlatformPressureLevel::Normal,
            PlatformPressureLevel::Critical,
            now + TimeDelta::seconds(5),
            &policy,
        ));
        tracker.reset();
        assert!(!tracker.ready(
            PlatformPressureLevel::Critical,
            PlatformPressureLevel::Normal,
            now,
            &policy,
        ));
        assert!(!tracker.ready(
            PlatformPressureLevel::Critical,
            PlatformPressureLevel::Normal,
            now + TimeDelta::seconds(59),
            &policy,
        ));
        assert!(tracker.ready(
            PlatformPressureLevel::Critical,
            PlatformPressureLevel::Normal,
            now + TimeDelta::seconds(60),
            &policy,
        ));
    }
}
