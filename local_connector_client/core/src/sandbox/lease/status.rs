// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use chatos_sandbox_contract::SandboxBackendKind;
use serde_json::{json, Value};

use crate::local_now_rfc3339;
use crate::sandbox::docker::inspect_local_sandbox_container;
use crate::sandbox::process::{native_sandbox_agent_alive, native_sandbox_process_alive};
use crate::sandbox::types::LocalSandboxRuntime;

pub(crate) async fn get_local_sandbox(
    sandbox_runtime: &LocalSandboxRuntime,
    sandbox_id: &str,
) -> Result<(u16, BTreeMap<String, String>, Value)> {
    let Some(lease) = sandbox_runtime.leases.read().await.get(sandbox_id).cloned() else {
        return Ok((
            404,
            BTreeMap::new(),
            json!({ "error": "sandbox not found" }),
        ));
    };
    Ok((200, BTreeMap::new(), json!(lease)))
}

pub(crate) async fn health_local_sandbox(
    http_client: &reqwest::Client,
    sandbox_runtime: &LocalSandboxRuntime,
    sandbox_id: &str,
) -> Result<(u16, BTreeMap<String, String>, Value)> {
    let Some(lease) = sandbox_runtime.leases.read().await.get(sandbox_id).cloned() else {
        return Ok((
            404,
            BTreeMap::new(),
            json!({ "error": "sandbox not found" }),
        ));
    };
    let (backend_alive, backend_check_name) = match lease.effective_policy.sandbox_mode {
        SandboxBackendKind::Docker => (
            inspect_local_sandbox_container(sandbox_id).await?,
            "docker_container",
        ),
        SandboxBackendKind::LocalProcess => (
            native_sandbox_process_alive(sandbox_runtime, sandbox_id).await,
            "native_process",
        ),
    };
    let workspace_alive = Path::new(lease.run_workspace.as_str()).is_dir();
    let agent_alive = match lease.effective_policy.sandbox_mode {
        SandboxBackendKind::Docker => match lease.agent_endpoint.as_deref() {
            Some(endpoint) => http_client
                .get(format!("{}/health", endpoint.trim_end_matches('/')))
                .timeout(Duration::from_secs(5))
                .send()
                .await
                .ok()
                .map(|response| response.status().is_success()),
            None => None,
        },
        SandboxBackendKind::LocalProcess => {
            Some(native_sandbox_agent_alive(sandbox_runtime, sandbox_id).await)
        }
    };
    let ok = backend_alive && workspace_alive && agent_alive.unwrap_or(false);
    Ok((
        200,
        BTreeMap::new(),
        json!({
            "ok": ok,
            "sandbox_id": lease.sandbox_id,
            "lease_id": lease.id,
            "status": lease.status,
            "backend": lease.backend,
            "backend_id": lease.backend_id,
            "backend_alive": backend_alive,
            "agent_endpoint": lease.agent_endpoint,
            "agent_alive": agent_alive,
            "workspace_alive": workspace_alive,
            "checked_at": local_now_rfc3339(),
            "effective_policy": lease.effective_policy,
            "effective_permissions": lease.effective_permissions,
            "message": if ok { "ok" } else { "local sandbox is not healthy" },
            "checks": [
                { "name": backend_check_name, "ok": backend_alive, "message": if backend_alive { "running" } else { "not running" } },
                { "name": "workspace", "ok": workspace_alive, "message": if workspace_alive { "available" } else { "missing" } },
                { "name": "agent", "ok": agent_alive.unwrap_or(false), "message": if agent_alive.unwrap_or(false) { "reachable" } else { "unreachable" } }
            ]
        }),
    ))
}
