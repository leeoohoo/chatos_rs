// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{bail, Context, Result};
use chatos_plugin_management_sdk::{
    PluginCloudComponentBundle, PluginExecutionHost, PluginManifest,
    PLUGIN_MANIFEST_SCHEMA_VERSION_V2,
};
use chatos_plugin_package::{
    build_component_text_bundle, load_verified_plugin_package_directory, PluginPackageLimits,
};

use crate::plugins::ActivePluginInstallation;

pub(super) fn load_local_portable_bundle(
    installation: &ActivePluginInstallation,
    manifest: &PluginManifest,
    component_key: &str,
) -> Result<Option<PluginCloudComponentBundle>> {
    let component = installation
        .version
        .inventory
        .components
        .iter()
        .find(|component| component.component_key == component_key)
        .context("Plugin component is missing from the signed installation inventory")?;
    match component.execution_host {
        PluginExecutionHost::Local => return Ok(None),
        PluginExecutionHost::Cloud => {
            bail!("cloud-only Plugin components cannot execute through Local Connector")
        }
        PluginExecutionHost::Portable => {}
    }
    if manifest.schema_version < PLUGIN_MANIFEST_SCHEMA_VERSION_V2 {
        bail!("portable Plugin execution requires Manifest schema v2");
    }
    let package = load_verified_plugin_package_directory(
        installation.installation_path.as_path(),
        installation.version.artifact_sha256.as_str(),
        &installation.version.package_file_sha256,
        PluginPackageLimits::default(),
    )
    .context("verify installed portable Plugin package")?;
    if package.manifest != *manifest
        || package.artifact_sha256 != installation.version.artifact_sha256
    {
        bail!("installed portable Plugin package does not match the signed Release");
    }
    build_component_text_bundle(
        installation.plugin_id.as_str(),
        installation.version.release_id.as_str(),
        installation.version.version.as_str(),
        installation.version.artifact_sha256.as_str(),
        &package,
        component,
        installation.version.installed_at.as_str(),
    )
    .map(Some)
    .context("build installed portable Plugin Bundle")
}

pub(super) fn validate_local_portable_bundle(
    installation: &ActivePluginInstallation,
    manifest: &PluginManifest,
    component_key: &str,
    expected_content_sha256: &str,
) -> Result<Option<PluginCloudComponentBundle>> {
    let bundle = load_local_portable_bundle(installation, manifest, component_key)?;
    if bundle
        .as_ref()
        .is_some_and(|bundle| bundle.bundle_sha256 != expected_content_sha256)
    {
        bail!("portable Plugin Bundle does not match the immutable component snapshot");
    }
    Ok(bundle)
}
