// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path};

use anyhow::{bail, Context, Result};
use chatos_plugin_management_sdk::{
    plugin_component_descriptors, plugin_ui_snapshot_sha256, PluginComponentKind,
    PluginUiAssetKind, PluginUiAssetSnapshot, PluginUiContribution, PluginUiSnapshot,
    PLUGIN_UI_ASSET_MAX_BYTES, PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1,
    PLUGIN_UI_ENTRYPOINT_MAX_BYTES, PLUGIN_UI_HOST_CSP_V1, PLUGIN_UI_IFRAME_SANDBOX_V1,
    PLUGIN_UI_SURFACE_DETAIL_PANEL, PLUGIN_UI_TOTAL_ASSET_MAX_BYTES,
};
use sha2::{Digest, Sha256};

use super::mcp_runtime::load_verified_manifest;
use crate::plugins::PluginInstaller;

#[derive(Debug, Clone)]
pub struct PluginUiLoader {
    installer: PluginInstaller,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedPluginUiAsset {
    pub kind: PluginUiAssetKind,
    pub relative_path: String,
    pub media_type: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub bytes: Vec<u8>,
}

impl PluginUiLoader {
    pub fn new(installer: PluginInstaller) -> Self {
        Self { installer }
    }

    pub fn load(
        &self,
        plugin_id: &str,
        component_key: &str,
        expected_content_sha256: &str,
        permission_snapshot: &BTreeSet<String>,
    ) -> Result<PluginUiSnapshot> {
        let installation = self
            .installer
            .active_installation(plugin_id)?
            .context("Plugin is not installed and active")?;
        let manifest = load_verified_manifest(&installation)?;
        let ui = manifest
            .ui
            .iter()
            .find(|ui| ui.component_key == component_key)
            .context("Plugin UI is not present in the active Manifest")?;
        validate_ui_inventory(&installation, &manifest, ui)?;
        validate_required_permissions(&installation, component_key, permission_snapshot)?;

        let (entrypoint_bytes, content_sha256) = read_verified_asset(
            installation.installation_path.as_path(),
            &installation.version.package_file_sha256,
            ui.source.path.as_str(),
            expected_content_sha256,
            PLUGIN_UI_ENTRYPOINT_MAX_BYTES,
            "Plugin UI entrypoint",
        )?;
        validate_html_entrypoint(entrypoint_bytes.as_slice())?;

        let mut total_asset_bytes = 0_u64;
        let mut assets = Vec::with_capacity(ui.assets.len());
        for asset in &ui.assets {
            let expected_asset_sha256 = installation
                .version
                .package_file_sha256
                .get(asset.path.trim_start_matches("./"))
                .with_context(|| {
                    format!(
                        "Plugin UI asset is not covered by package checksums: {}",
                        asset.path
                    )
                })?;
            let (bytes, sha256) = read_verified_asset(
                installation.installation_path.as_path(),
                &installation.version.package_file_sha256,
                asset.path.as_str(),
                expected_asset_sha256.as_str(),
                PLUGIN_UI_ASSET_MAX_BYTES,
                "Plugin UI asset",
            )?;
            let size_bytes = u64::try_from(bytes.len()).context("Plugin UI asset size overflow")?;
            total_asset_bytes = total_asset_bytes
                .checked_add(size_bytes)
                .context("Plugin UI total asset size overflow")?;
            if total_asset_bytes > PLUGIN_UI_TOTAL_ASSET_MAX_BYTES {
                bail!("Plugin UI assets exceed the total size limit");
            }
            assets.push(PluginUiAssetSnapshot {
                relative_path: asset.path.clone(),
                media_type: media_type_for_path(asset.path.as_str())?.to_string(),
                size_bytes,
                sha256,
            });
        }
        assets.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

        let title = ui
            .title
            .clone()
            .unwrap_or_else(|| component_key.to_string());
        let surface = ui
            .surface
            .clone()
            .unwrap_or_else(|| PLUGIN_UI_SURFACE_DETAIL_PANEL.to_string());
        let snapshot_sha256 = plugin_ui_snapshot_sha256(
            plugin_id,
            installation.version.release_id.as_str(),
            component_key,
            title.as_str(),
            surface.as_str(),
            ui.source.path.as_str(),
            content_sha256.as_str(),
            assets.as_slice(),
            PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1,
            ui.bridge_capabilities.as_slice(),
            ui.artifact_mime_types.as_slice(),
            PLUGIN_UI_HOST_CSP_V1,
            PLUGIN_UI_IFRAME_SANDBOX_V1,
        )
        .context("hash Plugin UI snapshot")?;
        Ok(PluginUiSnapshot {
            plugin_id: plugin_id.to_string(),
            release_id: installation.version.release_id,
            version: installation.version.version,
            artifact_sha256: installation.version.artifact_sha256,
            component_key: component_key.to_string(),
            title,
            surface,
            relative_source_path: ui.source.path.clone(),
            content_sha256,
            assets,
            bridge_protocol_version: PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1,
            bridge_capabilities: ui.bridge_capabilities.clone(),
            artifact_mime_types: ui.artifact_mime_types.clone(),
            content_security_policy: PLUGIN_UI_HOST_CSP_V1.to_string(),
            iframe_sandbox: PLUGIN_UI_IFRAME_SANDBOX_V1.to_string(),
            snapshot_sha256,
        })
    }

    pub fn read_asset(
        &self,
        snapshot: &PluginUiSnapshot,
        permission_snapshot: &BTreeSet<String>,
        relative_path: &str,
    ) -> Result<LoadedPluginUiAsset> {
        let current = self.load(
            snapshot.plugin_id.as_str(),
            snapshot.component_key.as_str(),
            snapshot.content_sha256.as_str(),
            permission_snapshot,
        )?;
        if current != *snapshot {
            bail!("Plugin UI snapshot no longer matches the active immutable Release");
        }

        let installation = self
            .installer
            .active_installation(snapshot.plugin_id.as_str())?
            .context("Plugin is not installed and active")?;
        if installation.version.release_id != snapshot.release_id
            || installation.version.artifact_sha256 != snapshot.artifact_sha256
        {
            bail!("Plugin UI snapshot no longer matches the active immutable Release");
        }
        let manifest = load_verified_manifest(&installation)?;
        let ui = manifest
            .ui
            .iter()
            .find(|ui| ui.component_key == snapshot.component_key)
            .context("Plugin UI is not present in the active Manifest")?;
        validate_ui_inventory(&installation, &manifest, ui)?;
        validate_required_permissions(
            &installation,
            snapshot.component_key.as_str(),
            permission_snapshot,
        )?;

        let (kind, media_type, expected_size_bytes, expected_sha256, max_bytes) =
            if relative_path == snapshot.relative_source_path {
                (
                    PluginUiAssetKind::Entrypoint,
                    "text/html; charset=utf-8".to_string(),
                    None,
                    snapshot.content_sha256.as_str(),
                    PLUGIN_UI_ENTRYPOINT_MAX_BYTES,
                )
            } else {
                let asset = snapshot
                    .assets
                    .iter()
                    .find(|asset| asset.relative_path == relative_path)
                    .context("Plugin UI asset was not published during prepare")?;
                (
                    PluginUiAssetKind::StaticAsset,
                    asset.media_type.clone(),
                    Some(asset.size_bytes),
                    asset.sha256.as_str(),
                    PLUGIN_UI_ASSET_MAX_BYTES,
                )
            };
        let (bytes, sha256) = read_verified_asset(
            installation.installation_path.as_path(),
            &installation.version.package_file_sha256,
            relative_path,
            expected_sha256,
            max_bytes,
            "Plugin UI requested asset",
        )?;
        if kind == PluginUiAssetKind::Entrypoint {
            validate_html_entrypoint(bytes.as_slice())?;
        }
        let size_bytes = u64::try_from(bytes.len()).context("Plugin UI asset size overflow")?;
        if expected_size_bytes.is_some_and(|expected| expected != size_bytes) {
            bail!("Plugin UI asset size does not match the immutable component snapshot");
        }
        Ok(LoadedPluginUiAsset {
            kind,
            relative_path: relative_path.to_string(),
            media_type,
            size_bytes,
            sha256,
            bytes,
        })
    }
}

fn validate_ui_inventory(
    installation: &crate::plugins::ActivePluginInstallation,
    manifest: &chatos_plugin_management_sdk::PluginManifest,
    ui: &PluginUiContribution,
) -> Result<()> {
    let descriptor = plugin_component_descriptors(manifest)
        .into_iter()
        .find(|component| component.component_key == ui.component_key)
        .context("Plugin UI component descriptor is unavailable")?;
    if descriptor.kind != PluginComponentKind::UiContribution
        || descriptor.runtime_kind != "sandboxed_ui"
        || descriptor.entrypoint.as_ref() != Some(&ui.source)
    {
        bail!("Plugin UI descriptor does not match its signed Manifest");
    }
    let installed = installation
        .version
        .inventory
        .components
        .iter()
        .find(|component| component.component_key == ui.component_key)
        .context("Plugin UI is missing from the signed installation inventory")?;
    if installed != &descriptor {
        bail!("Plugin UI inventory does not match the active signed Manifest");
    }
    Ok(())
}

fn validate_required_permissions(
    installation: &crate::plugins::ActivePluginInstallation,
    component_key: &str,
    permission_snapshot: &BTreeSet<String>,
) -> Result<()> {
    for requirement in installation
        .version
        .inventory
        .permissions
        .iter()
        .filter(|requirement| {
            requirement.required
                && (requirement.components.is_empty()
                    || requirement
                        .components
                        .iter()
                        .any(|key| key == component_key))
        })
    {
        if !permission_snapshot.contains(requirement.permission.as_str()) {
            bail!(
                "Plugin UI required permission is missing from the prepared snapshot: {}",
                requirement.permission
            );
        }
    }
    Ok(())
}

fn read_verified_asset(
    root: &Path,
    package_file_sha256: &std::collections::BTreeMap<String, String>,
    relative_path: &str,
    expected_content_sha256: &str,
    max_bytes: u64,
    label: &str,
) -> Result<(Vec<u8>, String)> {
    let package_path = relative_path.trim_start_matches("./");
    let expected_package_sha256 = package_file_sha256
        .get(package_path)
        .with_context(|| format!("{label} is not covered by package checksums"))?;
    let relative_path = Path::new(package_path);
    let components = relative_path.components().collect::<Vec<_>>();
    if components.is_empty()
        || components
            .iter()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("{label} path is not a normalized package-relative path");
    }
    let mut path = root.to_path_buf();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(component) = component else {
            unreachable!("components were validated above");
        };
        path.push(component);
        let metadata = fs::symlink_metadata(path.as_path())
            .with_context(|| format!("read {label} metadata"))?;
        if metadata.file_type().is_symlink() {
            bail!("{label} path contains a symbolic link");
        }
        if index + 1 < components.len() && !metadata.is_dir() {
            bail!("{label} parent path is not a directory");
        }
    }
    let metadata =
        fs::symlink_metadata(path.as_path()).with_context(|| format!("read {label} metadata"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > max_bytes {
        bail!("{label} is missing, unsafe, or exceeds its size limit");
    }
    let bytes = fs::read(path.as_path()).with_context(|| format!("read {label}"))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max_bytes {
        bail!("{label} exceeds its size limit");
    }
    let sha256 = hex::encode(Sha256::digest(bytes.as_slice()));
    if sha256 != *expected_package_sha256 || sha256 != expected_content_sha256 {
        bail!("{label} does not match the immutable component snapshot");
    }
    Ok((bytes, sha256))
}

fn validate_html_entrypoint(bytes: &[u8]) -> Result<()> {
    let html = std::str::from_utf8(bytes).context("Plugin UI entrypoint is not UTF-8")?;
    if html.contains('\0') {
        bail!("Plugin UI entrypoint contains NUL bytes");
    }
    let lower = html.to_ascii_lowercase();
    for forbidden in [
        "<base",
        "<iframe",
        "<frame",
        "<object",
        "<embed",
        "<portal",
        "<meta http-equiv",
        "javascript:",
    ] {
        if lower.contains(forbidden) {
            bail!("Plugin UI entrypoint contains a forbidden browser primitive");
        }
    }
    Ok(())
}

fn media_type_for_path(path: &str) -> Result<&'static str> {
    let extension = path
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .context("Plugin UI asset has no extension")?;
    match extension.as_str() {
        "js" | "mjs" => Ok("text/javascript"),
        "css" => Ok("text/css"),
        "json" => Ok("application/json"),
        "svg" => Ok("image/svg+xml"),
        "png" => Ok("image/png"),
        "jpg" | "jpeg" => Ok("image/jpeg"),
        "webp" => Ok("image/webp"),
        "gif" => Ok("image/gif"),
        "woff" => Ok("font/woff"),
        "woff2" => Ok("font/woff2"),
        _ => bail!("Plugin UI asset media type is not supported"),
    }
}

#[cfg(test)]
mod tests {
    use super::validate_html_entrypoint;

    #[test]
    fn html_entrypoint_rejects_navigation_and_nested_browsing_primitives() {
        for html in [
            br#"<base href="https://example.com">"#.as_slice(),
            br#"<iframe src="./nested.html"></iframe>"#.as_slice(),
            br#"<a href="javascript:alert(1)">bad</a>"#.as_slice(),
            br#"<meta http-equiv="refresh" content="0; url=https://example.com">"#.as_slice(),
        ] {
            assert!(validate_html_entrypoint(html).is_err());
        }
        validate_html_entrypoint(
            br#"<!doctype html><html><head><link rel="stylesheet" href="./styles.css"></head><body><div id="app"></div><script src="./app.js"></script></body></html>"#,
        )
        .expect("bounded local-only Plugin UI should pass structural validation");
    }
}
