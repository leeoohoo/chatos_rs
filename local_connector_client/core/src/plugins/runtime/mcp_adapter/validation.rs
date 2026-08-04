// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

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
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        bail!("Plugin MCP invocation id is invalid");
    }
    Ok(())
}

pub(in crate::plugins::runtime) fn load_verified_manifest(
    installation: &ActivePluginInstallation,
) -> Result<chatos_plugin_management_sdk::PluginManifest> {
    let relative_path = [".chatos-plugin/plugin.json", ".codex-plugin/plugin.json"]
        .into_iter()
        .find(|path| installation.version.package_file_sha256.contains_key(*path))
        .context("installed Plugin has no checksummed Manifest")?;
    let path = installation.installation_path.join(relative_path);
    let metadata = fs::symlink_metadata(path.as_path())
        .with_context(|| format!("read installed Plugin Manifest metadata: {relative_path}"))?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_MANIFEST_BYTES
    {
        bail!("installed Plugin Manifest is unsafe or exceeds its size limit");
    }
    let raw = fs::read_to_string(path.as_path()).context("read installed Plugin Manifest")?;
    let source = plugin_manifest_source_from_path(Path::new(relative_path))
        .context("derive installed Plugin Manifest source")?;
    let manifest = parse_plugin_manifest(raw.as_str(), source)
        .context("parse installed normalized Plugin Manifest")?;
    if normalized_plugin_manifest_sha256(&manifest)? != installation.version.manifest_sha256 {
        bail!("installed Plugin Manifest does not match the active signed Release");
    }
    Ok(manifest)
}
