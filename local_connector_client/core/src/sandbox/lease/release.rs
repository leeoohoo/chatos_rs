// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::approval::clear_session_approvals;
use crate::relay::RelayRequest;
use crate::sandbox::types::{LocalSandboxRuntime, ReleaseLocalSandboxRequest};
use crate::{local_now_rfc3339, LOCAL_SANDBOX_STATUS_DESTROYED};

pub(crate) async fn release_local_sandbox(
    request: &RelayRequest,
    sandbox_runtime: &LocalSandboxRuntime,
    sandbox_id: &str,
) -> Result<(u16, BTreeMap<String, String>, Value)> {
    let input = serde_json::from_value::<ReleaseLocalSandboxRequest>(request.body.clone())
        .context("parse local sandbox release request")?;
    let mut lease = {
        let leases = sandbox_runtime.leases.read().await;
        let Some(lease) = leases.get(sandbox_id).cloned() else {
            return Ok((
                404,
                BTreeMap::new(),
                json!({ "error": "sandbox not found" }),
            ));
        };
        lease
    };
    if lease.id != input.lease_id {
        return Ok((
            400,
            BTreeMap::new(),
            json!({ "error": "lease_id does not match sandbox" }),
        ));
    }
    let change_manifest = input.export_result.then(|| {
        json!({
            "schema_version": 1,
            "run_id": lease.run_id,
            "sandbox_id": lease.sandbox_id,
            "lease_id": lease.id,
            "generated_at": local_now_rfc3339(),
            "output_workspace": null,
            "manifest_path": null,
            "counts": {
                "added": 0,
                "modified": 0,
                "deleted": 0,
                "binary": 0,
                "diff_available": 0,
                "total": 0
            },
            "files": [],
            "message": "Local Connector tools execute directly in the local project"
        })
    });
    if input.destroy {
        lease.status = LOCAL_SANDBOX_STATUS_DESTROYED.to_string();
        lease.destroyed_at = Some(local_now_rfc3339());
        clear_session_approvals(sandbox_id).await;
    }
    lease.updated_at = local_now_rfc3339();
    sandbox_runtime
        .leases
        .write()
        .await
        .insert(sandbox_id.to_string(), lease.clone());
    Ok((
        200,
        BTreeMap::new(),
        json!({
            "ok": true,
            "status": lease.status,
            "output_workspace": null,
            "diff_summary": input.export_result.then_some("added=0, modified=0, deleted=0, total=0"),
            "output_error": null,
            "change_manifest": change_manifest,
        }),
    ))
}

pub(crate) async fn shutdown_local_sandboxes(sandbox_runtime: &LocalSandboxRuntime) -> Value {
    let lease_ids = sandbox_runtime
        .leases
        .read()
        .await
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    for sandbox_id in &lease_ids {
        clear_session_approvals(sandbox_id).await;
    }
    sandbox_runtime.leases.write().await.clear();
    json!({
        "ok": true,
        "released_leases": lease_ids.len(),
        "released_native_processes": 0,
        "errors": [],
    })
}
