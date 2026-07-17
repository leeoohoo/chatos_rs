// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use chatos_sandbox_contract::SandboxBackendKind;
use serde_json::{json, Value};

use crate::approval::clear_session_approvals;
use crate::relay::RelayRequest;
use crate::sandbox::docker::{
    destroy_all_local_sandbox_containers, destroy_local_sandbox_container,
};
use crate::sandbox::manifest::summarize_local_sandbox_manifest_counts;
use crate::sandbox::process::destroy_native_sandbox_process;
use crate::sandbox::types::{LocalSandboxRuntime, ReleaseLocalSandboxRequest};
use crate::sandbox::workspace::export_local_sandbox_output;
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
    let (output_workspace, change_manifest, diff_summary, output_error) = if input.export_result {
        match export_local_sandbox_output(&lease) {
            Ok(manifest) => {
                let output_workspace = manifest
                    .get("output_workspace")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                let summary = manifest
                    .get("counts")
                    .map(summarize_local_sandbox_manifest_counts);
                (output_workspace, Some(manifest), summary, None)
            }
            Err(err) => (None, None, None, Some(err.to_string())),
        }
    } else {
        (None, None, None, None)
    };
    if input.destroy {
        match lease.effective_policy.sandbox_mode {
            SandboxBackendKind::Docker => destroy_local_sandbox_container(sandbox_id).await?,
            SandboxBackendKind::LocalProcess => {
                destroy_native_sandbox_process(sandbox_runtime, sandbox_id).await?
            }
        }
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
            "output_workspace": output_workspace,
            "diff_summary": diff_summary,
            "output_error": output_error,
            "change_manifest": change_manifest,
        }),
    ))
}

pub(crate) async fn shutdown_local_sandboxes(sandbox_runtime: &LocalSandboxRuntime) -> Value {
    let process_ids = sandbox_runtime
        .processes
        .read()
        .await
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    let lease_ids = sandbox_runtime
        .leases
        .read()
        .await
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    let mut errors = Vec::new();
    for sandbox_id in &process_ids {
        if let Err(err) = destroy_native_sandbox_process(sandbox_runtime, sandbox_id).await {
            errors.push(format!("destroy native sandbox {sandbox_id} failed: {err}"));
        }
    }
    if let Err(err) = destroy_all_local_sandbox_containers().await {
        errors.push(format!("destroy Docker sandboxes failed: {err}"));
    }
    for sandbox_id in &lease_ids {
        clear_session_approvals(sandbox_id).await;
    }
    sandbox_runtime.leases.write().await.clear();
    json!({
        "ok": errors.is_empty(),
        "released_leases": lease_ids.len(),
        "released_native_processes": process_ids.len(),
        "errors": errors,
    })
}
