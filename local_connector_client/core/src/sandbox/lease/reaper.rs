// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

use crate::approval::clear_session_approvals;
use crate::sandbox::types::{LocalSandboxLease, LocalSandboxRuntime};
use crate::{local_now_rfc3339, LOCAL_SANDBOX_STATUS_DESTROYED};

const LOCAL_SANDBOX_LEASE_REAPER_INTERVAL: Duration = Duration::from_secs(15);

pub(crate) fn spawn_local_sandbox_lease_reaper(
    sandbox_runtime: LocalSandboxRuntime,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(LOCAL_SANDBOX_LEASE_REAPER_INTERVAL);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            reap_expired_local_sandboxes(&sandbox_runtime).await;
        }
    })
}

pub(crate) fn local_sandbox_lease_expired(lease: &LocalSandboxLease) -> bool {
    local_sandbox_lease_expired_at(lease.expires_at.as_str(), Utc::now())
}

async fn reap_expired_local_sandboxes(sandbox_runtime: &LocalSandboxRuntime) {
    let now = Utc::now();
    let expired = sandbox_runtime
        .leases
        .read()
        .await
        .values()
        .filter(|lease| {
            lease.status != LOCAL_SANDBOX_STATUS_DESTROYED
                && local_sandbox_lease_expired_at(lease.expires_at.as_str(), now)
        })
        .cloned()
        .collect::<Vec<_>>();

    for lease in expired {
        let sandbox_id = lease.sandbox_id.clone();
        clear_session_approvals(sandbox_id.as_str()).await;

        let mut leases = sandbox_runtime.leases.write().await;
        let Some(stored) = leases.get_mut(sandbox_id.as_str()) else {
            continue;
        };
        stored.updated_at = local_now_rfc3339();
        stored.status = LOCAL_SANDBOX_STATUS_DESTROYED.to_string();
        stored.destroyed_at = Some(stored.updated_at.clone());
        stored.last_error = None;
    }
}

fn local_sandbox_lease_expired_at(expires_at: &str, now: DateTime<Utc>) -> bool {
    DateTime::parse_from_rfc3339(expires_at)
        .map(|expires_at| expires_at.with_timezone(&Utc) <= now)
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    #[test]
    fn lease_expiration_is_fail_closed() {
        let now = Utc::now();
        assert!(local_sandbox_lease_expired_at(
            (now - Duration::seconds(1)).to_rfc3339().as_str(),
            now
        ));
        assert!(!local_sandbox_lease_expired_at(
            (now + Duration::seconds(1)).to_rfc3339().as_str(),
            now
        ));
        assert!(local_sandbox_lease_expired_at("not-a-timestamp", now));
    }
}
