// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{bail, Result};
use chatos_plugin_management_sdk::normalized_plugin_manifest_sha256;
use tokio_util::sync::CancellationToken;

use super::MAX_INVOCATION_ID_BYTES;
use crate::plugins::ActivePluginInstallation;

pub(super) async fn wait_for_invocation_cancellation(cancellation: Option<CancellationToken>) {
    match cancellation {
        Some(cancellation) => cancellation.cancelled().await,
        None => std::future::pending::<()>().await,
    }
}

pub(super) fn validate_invocation_id(invocation_id: &str) -> Result<()> {
    let invocation_id = invocation_id.trim();
    if invocation_id.is_empty()
        || invocation_id.len() > MAX_INVOCATION_ID_BYTES
        || !invocation_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b':'))
    {
        bail!("Plugin MCP invocation id is invalid");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_invocation_id;

    #[test]
    fn accepts_mcp_management_batch_invocation_ids() {
        assert!(validate_invocation_id("mcp_batch_task_runner_agent_run_1_1:0").is_ok());
    }

    #[test]
    fn rejects_path_and_control_characters() {
        assert!(validate_invocation_id("batch/0").is_err());
        assert!(validate_invocation_id("batch\n0").is_err());
    }
}

pub(in crate::plugins::runtime) fn load_verified_manifest(
    installation: &ActivePluginInstallation,
) -> Result<chatos_plugin_management_sdk::PluginManifest> {
    let manifest = installation.version.manifest.clone();
    if normalized_plugin_manifest_sha256(&manifest)? != installation.version.manifest_sha256 {
        bail!("installed Plugin Manifest snapshot does not match the active signed Release");
    }
    Ok(manifest)
}
