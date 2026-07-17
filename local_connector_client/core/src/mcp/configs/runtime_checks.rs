// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::Result;
use chatos_mcp_runtime::invalidate_stdio_session;
use futures_util::future::join_all;

use crate::local_runtime::LocalDatabase;
use crate::mcp::repository::state_identity;
use crate::{LocalRuntime, LocalState};

use super::record::apply_manifest_test_result;
use super::transport::{stdio_server_for_manifest, test_manifest_record};

pub(crate) async fn refresh_enabled_local_mcp_checks(
    database: &LocalDatabase,
    state: &tokio::sync::RwLock<LocalState>,
) -> Result<()> {
    let state_guard = state.read().await;
    let (owner_user_id, device_id) = state_identity(&state_guard)?;
    drop(state_guard);
    let manifests = database
        .list_mcp_manifests(owner_user_id.as_str(), device_id.as_str())
        .await?
        .into_iter()
        .filter(|record| record.enabled && record.plugin_mcp_id.is_some())
        .collect::<Vec<_>>();
    if manifests.is_empty() {
        return Ok(());
    }
    let tested = join_all(manifests.into_iter().map(|mut record| async move {
        let result = test_manifest_record(&record).await;
        apply_manifest_test_result(&mut record, result);
        record
    }))
    .await;
    for tested_record in tested {
        let Some(mut current) = database
            .get_mcp_manifest(
                owner_user_id.as_str(),
                device_id.as_str(),
                tested_record.manifest_id.as_str(),
            )
            .await?
            .filter(|current| current.manifest_hash == tested_record.manifest_hash)
        else {
            continue;
        };
        current.last_check_status = tested_record.last_check_status;
        current.last_checked_at = tested_record.last_checked_at;
        current.last_error = tested_record.last_error;
        current.tool_snapshot = tested_record.tool_snapshot;
        database.save_mcp_manifest(&current).await?;
    }
    Ok(())
}

pub(crate) async fn invalidate_manifest_session(runtime: &LocalRuntime, manifest_id: &str) {
    let server = crate::mcp::repository::get_runtime_manifest(runtime, manifest_id)
        .await
        .ok()
        .and_then(|record| stdio_server_for_manifest(&record).ok());
    if let Some(server) = server {
        invalidate_stdio_session(&server);
    }
}
