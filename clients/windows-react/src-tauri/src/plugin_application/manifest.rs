use super::{
    metadata_response_error, plugin_catalog_error, PluginManifest, MAXIMUM_MANIFEST_BYTES,
    MAXIMUM_MCP_SERVERS, MAXIMUM_UI_CONTRIBUTIONS,
};
use crate::{
    plugin_installation_metadata::{canonical_plugin_root, record_matches_summary},
    plugin_integrity::{requires_installation_integrity, verify_installation_integrity},
};
use chatos_local_agent_protocol::{LocalPluginInstallationRecord, LocalPluginInstallationSummary};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

pub(super) fn validate_summary(
    summary: &LocalPluginInstallationSummary,
    owner_user_id: &str,
) -> Result<(), String> {
    for value in [
        &summary.installation_id,
        &summary.plugin_id,
        &summary.release_id,
        &summary.component_id,
        &summary.component_revision,
        &summary.server_id,
    ] {
        validate_text(value, 1_024)?;
    }
    if summary.owner_user_id != owner_user_id
        || summary.version == 0
        || summary.created_at_unix_ms < 0
        || summary.updated_at_unix_ms < summary.created_at_unix_ms
    {
        return Err(metadata_response_error());
    }
    Ok(())
}

pub(super) fn validate_record(
    record: &LocalPluginInstallationRecord,
    summary: &LocalPluginInstallationSummary,
    owner_user_id: &str,
) -> Result<(), String> {
    if !record_matches_summary(record, summary, owner_user_id)
        || !record.spec.enabled
        || !valid_release_digest(&record.spec.release_digest)
    {
        return Err(metadata_response_error());
    }
    Ok(())
}

pub(super) fn confined_plugin_root(requested: &Path) -> Result<PathBuf, String> {
    canonical_plugin_root(requested).map_err(|_| plugin_catalog_error())
}

pub(super) fn load_manifest(
    plugin_root: &Path,
    record: &LocalPluginInstallationRecord,
) -> Result<(PathBuf, PluginManifest), String> {
    let executable = PathBuf::from(&record.spec.executable_path)
        .canonicalize()
        .map_err(|_| plugin_catalog_error())?;
    if !executable.is_file() || !executable.starts_with(plugin_root) {
        return Err(plugin_catalog_error());
    }
    let mut candidate = executable.parent().ok_or_else(plugin_catalog_error)?;
    let installation_root = loop {
        if !candidate.starts_with(plugin_root) {
            return Err(plugin_catalog_error());
        }
        let manifest = candidate.join("chatos.plugin.json");
        if manifest.is_file() {
            break candidate.to_path_buf();
        }
        if candidate == plugin_root {
            return Err(plugin_catalog_error());
        }
        candidate = candidate.parent().ok_or_else(plugin_catalog_error)?;
    };
    let manifest_path = installation_root.join("chatos.plugin.json");
    if requires_installation_integrity(&record.spec.component_revision) {
        verify_installation_integrity(plugin_root, &installation_root, &record.spec.release_digest)
            .map_err(|_| plugin_catalog_error())?;
    }
    let canonical_manifest = manifest_path
        .canonicalize()
        .map_err(|_| plugin_catalog_error())?;
    if !canonical_manifest.starts_with(&installation_root) {
        return Err(plugin_catalog_error());
    }
    let metadata = canonical_manifest
        .metadata()
        .map_err(|_| plugin_catalog_error())?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAXIMUM_MANIFEST_BYTES {
        return Err(plugin_catalog_error());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(&canonical_manifest)
        .and_then(|file| {
            file.take(MAXIMUM_MANIFEST_BYTES + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|_| plugin_catalog_error())?;
    if bytes.len() as u64 > MAXIMUM_MANIFEST_BYTES {
        return Err(plugin_catalog_error());
    }
    let manifest =
        serde_json::from_slice::<PluginManifest>(&bytes).map_err(|_| plugin_catalog_error())?;
    validate_manifest(&manifest)?;
    Ok((installation_root, manifest))
}

fn validate_manifest(manifest: &PluginManifest) -> Result<(), String> {
    if manifest.schema_version != 3
        || manifest.ui.len() > MAXIMUM_UI_CONTRIBUTIONS
        || manifest.mcp_servers.len() > MAXIMUM_MCP_SERVERS
        || !valid_text(&manifest.name, 512)
        || !valid_text(&manifest.version, 128)
        || !valid_optional_text(&manifest.description, 16 * 1_024)
    {
        return Err(plugin_catalog_error());
    }
    if let Some(platforms) = manifest
        .dependencies
        .as_ref()
        .map(|dependencies| &dependencies.supported_platforms)
    {
        if platforms.len() > 32
            || (!platforms.is_empty()
                && !platforms
                    .iter()
                    .any(|platform| platform.eq_ignore_ascii_case("windows")))
        {
            return Err(plugin_catalog_error());
        }
    }
    Ok(())
}

fn valid_release_digest(value: &str) -> bool {
    let digest = value.strip_prefix("sha256:").unwrap_or(value);
    digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(super) fn valid_package_executable(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && value != "."
        && value != ".."
}

pub(super) fn valid_health_path(value: &str) -> bool {
    value.starts_with('/')
        && value.len() <= 2_048
        && !value.starts_with("//")
        && !value.contains("..")
        && !value.contains('?')
        && !value.contains('#')
        && !value.chars().any(char::is_control)
}

pub(super) fn valid_capability(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

pub(super) fn valid_environment_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value.contains('=')
        && !value.chars().any(char::is_control)
}

pub(super) fn valid_brand_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(super) fn validate_text(value: &str, maximum: usize) -> Result<(), String> {
    if valid_text(value, maximum) {
        Ok(())
    } else {
        Err(plugin_catalog_error())
    }
}

fn valid_text(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.trim() == value
        && value.len() <= maximum
        && !value.chars().any(char::is_control)
}

pub(super) fn valid_optional_text(value: &str, maximum: usize) -> bool {
    value.len() <= maximum && !value.chars().any(char::is_control)
}
