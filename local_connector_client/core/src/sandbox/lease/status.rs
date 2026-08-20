// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use serde_json::{json, Value};

use crate::local_now_rfc3339;
use crate::sandbox::types::LocalSandboxRuntime;

pub(crate) async fn get_local_sandbox(
    sandbox_runtime: &LocalSandboxRuntime,
    runtime_id: &str,
) -> Result<(u16, BTreeMap<String, String>, Value)> {
    let Some(lease) = sandbox_runtime.leases.read().await.get(runtime_id).cloned() else {
        return Ok((
            404,
            BTreeMap::new(),
            json!({ "error": "sandbox not found" }),
        ));
    };
    Ok((
        200,
        BTreeMap::new(),
        super::cloud_safe_local_sandbox_lease(&lease),
    ))
}

pub(crate) async fn health_local_sandbox(
    _http_client: &reqwest::Client,
    sandbox_runtime: &LocalSandboxRuntime,
    runtime_id: &str,
) -> Result<(u16, BTreeMap<String, String>, Value)> {
    let Some(lease) = sandbox_runtime.leases.read().await.get(runtime_id).cloned() else {
        return Ok((
            404,
            BTreeMap::new(),
            json!({ "error": "sandbox not found" }),
        ));
    };
    let workspace_alive = Path::new(lease.run_workspace.as_str()).is_dir();
    let backend_alive = workspace_alive;
    let ok = workspace_alive;
    Ok((
        200,
        BTreeMap::new(),
        json!({
            "ok": ok,
            "lease_id": lease.id,
            "status": lease.status,
            "backend": lease.backend,
            "backend_id": null,
            "backend_alive": backend_alive,
            "agent_endpoint": null,
            "agent_alive": null,
            "workspace_alive": workspace_alive,
            "checked_at": local_now_rfc3339(),
            "effective_policy": super::cloud_safe_effective_policy(&lease),
            "effective_permissions": super::cloud_safe_effective_permissions(&lease),
            "message": if ok { "ok" } else { "local lease is not healthy" },
            "checks": [
                { "name": "local_connector_client", "ok": true, "message": "connected" },
                { "name": "workspace", "ok": workspace_alive, "message": if workspace_alive { "available" } else { "missing" } }
            ]
        }),
    ))
}
