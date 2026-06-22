use std::collections::BTreeMap;

use once_cell::sync::Lazy;
use parking_lot::RwLock;
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeHealthStatus {
    Ok,
    Degraded,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeHealthCheckStatus {
    Ok,
    Warn,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RuntimeHealthCheck {
    pub name: String,
    pub status: RuntimeHealthCheckStatus,
    pub required_for_ready: bool,
    pub message: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RuntimeHealthSnapshot {
    pub status: RuntimeHealthStatus,
    pub ready: bool,
    pub check_count: usize,
    pub degraded_check_count: usize,
    pub checks: Vec<RuntimeHealthCheck>,
}

static RUNTIME_HEALTH_CHECKS: Lazy<RwLock<BTreeMap<String, RuntimeHealthCheck>>> =
    Lazy::new(|| RwLock::new(BTreeMap::new()));

fn upsert_runtime_health_check(
    name: &str,
    required_for_ready: bool,
    status: RuntimeHealthCheckStatus,
    message: impl Into<String>,
) {
    let check = RuntimeHealthCheck {
        name: name.to_string(),
        status,
        required_for_ready,
        message: message.into(),
        updated_at: crate::core::time::now_rfc3339(),
    };
    RUNTIME_HEALTH_CHECKS
        .write()
        .insert(check.name.clone(), check);
}

pub fn mark_runtime_check_ok(name: &str, required_for_ready: bool, message: impl Into<String>) {
    upsert_runtime_health_check(
        name,
        required_for_ready,
        RuntimeHealthCheckStatus::Ok,
        message,
    );
}

pub fn mark_runtime_check_warn(name: &str, required_for_ready: bool, message: impl Into<String>) {
    upsert_runtime_health_check(
        name,
        required_for_ready,
        RuntimeHealthCheckStatus::Warn,
        message,
    );
}

pub fn snapshot_runtime_health() -> RuntimeHealthSnapshot {
    let checks = RUNTIME_HEALTH_CHECKS
        .read()
        .values()
        .cloned()
        .collect::<Vec<_>>();
    let degraded_check_count = checks
        .iter()
        .filter(|check| check.status == RuntimeHealthCheckStatus::Warn)
        .count();
    let ready = checks
        .iter()
        .all(|check| !(check.required_for_ready && check.status == RuntimeHealthCheckStatus::Warn));

    RuntimeHealthSnapshot {
        status: if degraded_check_count > 0 {
            RuntimeHealthStatus::Degraded
        } else {
            RuntimeHealthStatus::Ok
        },
        ready,
        check_count: checks.len(),
        degraded_check_count,
        checks,
    }
}

#[cfg(test)]
pub fn reset_runtime_health_for_tests() {
    RUNTIME_HEALTH_CHECKS.write().clear();
}

#[cfg(test)]
mod tests {
    use once_cell::sync::Lazy;
    use parking_lot::Mutex;

    use super::{
        mark_runtime_check_ok, mark_runtime_check_warn, reset_runtime_health_for_tests,
        snapshot_runtime_health, RuntimeHealthStatus,
    };

    static TEST_GUARD: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    #[test]
    fn optional_warning_degrades_health_without_breaking_readiness() {
        let _guard = TEST_GUARD.lock();
        reset_runtime_health_for_tests();
        mark_runtime_check_ok("database", true, "database initialized");
        mark_runtime_check_warn("terminal_cleanup", false, "cleanup skipped");

        let snapshot = snapshot_runtime_health();

        assert_eq!(snapshot.status, RuntimeHealthStatus::Degraded);
        assert!(snapshot.ready);
        assert_eq!(snapshot.degraded_check_count, 1);
    }

    #[test]
    fn required_warning_breaks_readiness() {
        let _guard = TEST_GUARD.lock();
        reset_runtime_health_for_tests();
        mark_runtime_check_warn("workspace_realtime_watcher", true, "watcher failed");

        let snapshot = snapshot_runtime_health();

        assert_eq!(snapshot.status, RuntimeHealthStatus::Degraded);
        assert!(!snapshot.ready);
        assert_eq!(snapshot.degraded_check_count, 1);
    }
}
