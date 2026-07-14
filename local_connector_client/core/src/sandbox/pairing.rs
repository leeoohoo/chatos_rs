// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use anyhow::{Context, Result};
use chatos_sandbox_contract::SandboxBackendKind;
use serde_json::{json, Value};
use tokio::sync::RwLock;

use crate::config::{api_url, ClientConfig};
use crate::registration::ensure_success;
use crate::sandbox::docker::docker_status_struct;
use crate::LocalState;

pub(crate) async fn reconcile_sandbox_pairings(
    client: &reqwest::Client,
    config: &ClientConfig,
    state: &Arc<RwLock<LocalState>>,
    device_id: &str,
) -> Result<usize> {
    let (enabled, workspaces, policy) = {
        let state = state.read().await;
        (
            state.sandbox.enabled,
            state.workspaces.clone(),
            state.sandbox.effective_policy_defaults(),
        )
    };
    let docker_status =
        serde_json::to_value(docker_status_struct().await).unwrap_or_else(|_| json!({}));
    let readiness = sandbox_pairing_readiness(policy.sandbox_mode, &docker_status);
    let mut synced = 0;
    for workspace in workspaces {
        let response = client
            .post(api_url(
                config.cloud_base_url.as_str(),
                "/api/local-connectors/sandbox-pairings",
            ))
            .bearer_auth(config.access_token.as_str())
            .json(&json!({
                "device_id": device_id,
                "workspace_id": workspace.id,
                "enabled": enabled,
                "sandbox_mode": policy.sandbox_mode,
                "sandbox_readiness": readiness,
                "permission_profile_id": policy.permission_profile_id,
                "approval_policy": policy.approval_policy,
                "approval_reviewer": policy.approval_reviewer,
                "policy_revision": policy.policy_revision.clone(),
            }))
            .send()
            .await
            .context("reconcile Local Connector sandbox pairing")?;
        ensure_success(
            response.status(),
            "reconcile Local Connector sandbox pairing",
        )?;
        synced += 1;
    }
    Ok(synced)
}

fn sandbox_pairing_readiness(backend: SandboxBackendKind, docker_status: &Value) -> &'static str {
    match backend {
        SandboxBackendKind::Docker => {
            if docker_status
                .get("installed")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                && docker_status
                    .get("running")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            {
                "ready"
            } else {
                "setup_required"
            }
        }
        SandboxBackendKind::LocalProcess => "under_development",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_pairing_readiness_tracks_docker_status() {
        assert_eq!(
            sandbox_pairing_readiness(
                SandboxBackendKind::Docker,
                &json!({ "installed": true, "running": true }),
            ),
            "ready"
        );
        assert_eq!(
            sandbox_pairing_readiness(
                SandboxBackendKind::Docker,
                &json!({ "installed": true, "running": false }),
            ),
            "setup_required"
        );
        assert_eq!(
            sandbox_pairing_readiness(
                SandboxBackendKind::Docker,
                &json!({ "installed": false, "running": false }),
            ),
            "setup_required"
        );
    }

    #[test]
    fn local_process_pairing_readiness_is_not_ready_until_native_isolation_exists() {
        assert_eq!(
            sandbox_pairing_readiness(SandboxBackendKind::LocalProcess, &json!({})),
            "under_development"
        );
    }
}
