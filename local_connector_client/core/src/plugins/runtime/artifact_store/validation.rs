// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) fn validate_write_body(bytes: &[u8]) -> Result<()> {
    if bytes.len() as u64 > PLUGIN_ARTIFACT_WRITE_MAX_BYTES {
        bail!("Plugin Artifact write body exceeds the local size limit");
    }
    Ok(())
}

pub(super) fn validate_artifact_display_name(display_name: &str, media_type: &str) -> Result<()> {
    let path = Path::new(display_name);
    if display_name.trim() != display_name
        || display_name.is_empty()
        || display_name.len() > 512
        || display_name.contains('/')
        || display_name.contains('\\')
        || display_name.chars().any(char::is_control)
        || path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
        || artifact_media_type(path) != Some(media_type)
    {
        bail!("Plugin Artifact display name or MIME type is invalid");
    }
    Ok(())
}

pub(super) fn plugin_artifact_workspace_relative_path(
    grant: &PluginUiArtifactGrant,
    artifact_id: &str,
    display_name: &str,
) -> String {
    let mut identity = Sha256::new();
    identity.update(b"chatos.plugin.artifact.workspace.v1\0");
    for value in [
        grant.owner_user_id.as_str(),
        grant.device_id.as_str(),
        grant.workspace_id.as_str(),
        grant.run_id.as_str(),
        grant.plugin_id.as_str(),
        grant.release_id.as_str(),
        grant.component_key.as_str(),
        grant.adapter_session_id.as_str(),
    ] {
        identity.update((value.len() as u64).to_be_bytes());
        identity.update(value.as_bytes());
    }
    let identity = hex::encode(identity.finalize());
    format!(
        "{PLUGIN_ARTIFACT_WORKSPACE_DIRECTORY}/{}/{artifact_id}/{display_name}",
        &identity[..32]
    )
}

pub(super) fn prepare_plugin_artifact_create_path(
    state: &LocalState,
    request: &RelayRequest,
    relative_path: &str,
) -> Result<PathBuf> {
    let workspace = state
        .workspace_by_id(request.workspace_id.trim())
        .ok_or_else(|| anyhow!("Plugin Artifact workspace is not registered"))?;
    let root = workspace
        .absolute_root
        .canonicalize()
        .context("resolve Plugin Artifact workspace root")?;
    let relative = Path::new(relative_path);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("Plugin Artifact create path is not workspace-relative");
    }
    let parent = relative
        .parent()
        .context("Plugin Artifact create path has no parent")?;
    let mut cursor = root.clone();
    for component in parent.components() {
        let Component::Normal(component) = component else {
            bail!("Plugin Artifact create directory is invalid");
        };
        cursor.push(component);
        match fs::symlink_metadata(cursor.as_path()) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    bail!("Plugin Artifact create path contains an unsafe directory");
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(cursor.as_path()).with_context(|| {
                    format!("create Plugin Artifact directory: {}", cursor.display())
                })?;
                let metadata = fs::symlink_metadata(cursor.as_path()).with_context(|| {
                    format!("inspect Plugin Artifact directory: {}", cursor.display())
                })?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    bail!("Plugin Artifact create directory is unsafe");
                }
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("inspect Plugin Artifact directory: {}", cursor.display())
                });
            }
        }
    }
    let target = root.join(relative);
    match fs::symlink_metadata(target.as_path()) {
        Ok(_) => bail!("Plugin Artifact create target already exists"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(target),
        Err(error) => Err(error).context("inspect Plugin Artifact create target"),
    }
}

pub(super) fn atomic_write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .context("Plugin Artifact target has no parent directory")?;
    let mut temporary =
        NamedTempFile::new_in(parent).context("create temporary Plugin Artifact file")?;
    temporary
        .write_all(bytes)
        .context("write temporary Plugin Artifact file")?;
    temporary
        .as_file()
        .sync_all()
        .context("sync temporary Plugin Artifact file")?;
    temporary
        .persist_noclobber(path)
        .map_err(|error| error.error)
        .context("persist new Plugin Artifact file")?;
    sync_registry_directory(parent)?;
    Ok(())
}

pub(super) fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .context("Plugin Artifact target has no parent directory")?;
    let mut temporary =
        NamedTempFile::new_in(parent).context("create replacement Plugin Artifact file")?;
    temporary
        .write_all(bytes)
        .context("write replacement Plugin Artifact file")?;
    temporary
        .as_file()
        .sync_all()
        .context("sync replacement Plugin Artifact file")?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .context("atomically replace Plugin Artifact file")?;
    sync_registry_directory(parent)?;
    Ok(())
}

pub(super) fn validate_persisted_state(state: &mut PluginArtifactStoreState) -> Result<()> {
    if state.ui_grants.len() > MAX_PERSISTED_PLUGIN_UI_GRANTS
        || state.artifacts.len() > MAX_REGISTERED_PLUGIN_ARTIFACTS
    {
        bail!("Plugin Artifact registry exceeds its item limit");
    }
    prune_expired(state);
    for (adapter_session_id, grant) in &state.ui_grants {
        validate_persisted_grant(adapter_session_id.as_str(), grant)?;
    }
    for (artifact_id, artifact) in &state.artifacts {
        validate_persisted_artifact(artifact_id.as_str(), artifact)?;
        let matching_grants = state
            .ui_grants
            .values()
            .filter(|grant| grant_can_retain_artifact(grant, &artifact.descriptor))
            .collect::<Vec<_>>();
        if matching_grants.is_empty() {
            bail!("Plugin Artifact registry contains an Artifact without an active UI grant");
        }
        if artifact.descriptor.mutable
            && !matching_grants.iter().any(|grant| {
                artifact.descriptor.owner.component_key == grant.component_key
                    && artifact.descriptor.owner.adapter_session_id == grant.adapter_session_id
                    && grant.ui.bridge_capabilities.iter().any(|capability| {
                        capability == artifact.descriptor.producer_tool_name.as_str()
                    })
            })
        {
            bail!("Plugin Artifact registry mutable Artifact has no exact UI write grant");
        }
    }
    Ok(())
}

fn validate_persisted_grant(adapter_session_id: &str, grant: &PluginUiArtifactGrant) -> Result<()> {
    validate_bounded_identity("owner user id", grant.owner_user_id.as_str(), 256)?;
    validate_bounded_identity("device id", grant.device_id.as_str(), 256)?;
    validate_bounded_identity("workspace id", grant.workspace_id.as_str(), 256)?;
    validate_bounded_identity("Run id", grant.run_id.as_str(), 256)?;
    validate_bounded_identity("Plugin id", grant.plugin_id.as_str(), 256)?;
    validate_bounded_identity("Plugin Release id", grant.release_id.as_str(), 256)?;
    validate_bounded_identity("Plugin component key", grant.component_key.as_str(), 256)?;
    validate_bounded_identity("adapter session id", grant.adapter_session_id.as_str(), 256)?;
    if adapter_session_id != grant.adapter_session_id {
        bail!("Plugin Artifact registry UI grant key does not match its adapter session");
    }
    if !is_lower_sha256(grant.artifact_sha256.as_str())
        || grant.expires_at <= Utc::now().timestamp()
    {
        bail!("Plugin Artifact registry UI grant identity or expiry is invalid");
    }
    if grant.permission_snapshot.len() > 256 {
        bail!("Plugin Artifact registry UI permission snapshot exceeds its item limit");
    }
    for permission in &grant.permission_snapshot {
        validate_bounded_identity("Plugin permission", permission.as_str(), 256)?;
    }
    validate_persisted_ui_snapshot(grant)
}

fn validate_persisted_ui_snapshot(grant: &PluginUiArtifactGrant) -> Result<()> {
    let ui = &grant.ui;
    if ui.plugin_id != grant.plugin_id
        || ui.release_id != grant.release_id
        || ui.artifact_sha256 != grant.artifact_sha256
        || ui.component_key != grant.component_key
        || ui.bridge_protocol_version != PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1
        || ui.content_security_policy != PLUGIN_UI_HOST_CSP_V1
        || ui.iframe_sandbox != PLUGIN_UI_IFRAME_SANDBOX_V1
        || !is_lower_sha256(ui.content_sha256.as_str())
        || !is_lower_sha256(ui.snapshot_sha256.as_str())
    {
        bail!("Plugin Artifact registry UI snapshot identity is invalid");
    }
    for (label, value, limit) in [
        ("Plugin UI version", ui.version.as_str(), 128_usize),
        ("Plugin UI title", ui.title.as_str(), 512_usize),
        ("Plugin UI surface", ui.surface.as_str(), 64_usize),
        (
            "Plugin UI relative source path",
            ui.relative_source_path.as_str(),
            4_096_usize,
        ),
    ] {
        validate_bounded_identity(label, value, limit)?;
    }
    if ui.assets.len() > PLUGIN_UI_MAX_ASSETS
        || ui.bridge_capabilities.len() > PLUGIN_UI_MAX_BRIDGE_CAPABILITIES
        || ui.artifact_mime_types.len() > PLUGIN_UI_MAX_ARTIFACT_MIME_TYPES
    {
        bail!("Plugin Artifact registry UI snapshot exceeds its item limit");
    }
    let mut total_asset_bytes = 0_u64;
    let mut asset_paths = BTreeSet::new();
    for asset in &ui.assets {
        validate_bounded_identity(
            "Plugin UI asset relative path",
            asset.relative_path.as_str(),
            4_096,
        )?;
        validate_bounded_identity("Plugin UI asset media type", asset.media_type.as_str(), 256)?;
        if !asset_paths.insert(asset.relative_path.as_str())
            || asset.size_bytes > PLUGIN_UI_ASSET_MAX_BYTES
            || !is_lower_sha256(asset.sha256.as_str())
        {
            bail!("Plugin Artifact registry UI asset snapshot is invalid");
        }
        total_asset_bytes = total_asset_bytes
            .checked_add(asset.size_bytes)
            .context("Plugin UI asset size total overflow")?;
    }
    if total_asset_bytes > PLUGIN_UI_TOTAL_ASSET_MAX_BYTES {
        bail!("Plugin Artifact registry UI assets exceed the total size limit");
    }
    validate_unique_bounded_values(
        "Plugin UI bridge capability",
        ui.bridge_capabilities.as_slice(),
        256,
    )?;
    validate_unique_bounded_values(
        "Plugin UI Artifact MIME type",
        ui.artifact_mime_types.as_slice(),
        256,
    )?;
    let expected_snapshot_sha256 = plugin_ui_snapshot_sha256(
        ui.plugin_id.as_str(),
        ui.release_id.as_str(),
        ui.component_key.as_str(),
        ui.title.as_str(),
        ui.surface.as_str(),
        ui.relative_source_path.as_str(),
        ui.content_sha256.as_str(),
        ui.assets.as_slice(),
        ui.bridge_protocol_version,
        ui.bridge_capabilities.as_slice(),
        ui.artifact_mime_types.as_slice(),
        ui.content_security_policy.as_str(),
        ui.iframe_sandbox.as_str(),
    )
    .context("hash persisted Plugin UI snapshot")?;
    if expected_snapshot_sha256 != ui.snapshot_sha256 {
        bail!("Plugin Artifact registry UI snapshot hash does not match");
    }
    Ok(())
}

fn validate_persisted_artifact(
    artifact_id: &str,
    artifact: &RegisteredPluginArtifact,
) -> Result<()> {
    let descriptor = &artifact.descriptor;
    if artifact_id != descriptor.artifact_id
        || !is_plugin_artifact_id(artifact_id)
        || !descriptor.downloadable
        || descriptor.size_bytes > PLUGIN_ARTIFACT_MAX_BYTES
        || !is_lower_sha256(descriptor.sha256.as_str())
    {
        bail!("Plugin Artifact registry descriptor flags or identity are invalid");
    }
    if descriptor.mutable
        && (descriptor.size_bytes > PLUGIN_ARTIFACT_WRITE_MAX_BYTES
            || !matches!(
                descriptor.producer_tool_name.as_str(),
                PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE
                    | PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE
            ))
    {
        bail!("Plugin Artifact registry mutable descriptor is invalid");
    }
    for (label, value, limit) in [
        (
            "Artifact owner user id",
            descriptor.owner.owner_user_id.as_str(),
            256_usize,
        ),
        (
            "Artifact Run id",
            descriptor.owner.run_id.as_str(),
            256_usize,
        ),
        (
            "Artifact device id",
            descriptor.owner.device_id.as_str(),
            256_usize,
        ),
        (
            "Artifact workspace id",
            descriptor.owner.workspace_id.as_str(),
            256_usize,
        ),
        (
            "Artifact Plugin id",
            descriptor.owner.plugin_id.as_str(),
            256_usize,
        ),
        (
            "Artifact Release id",
            descriptor.owner.release_id.as_str(),
            256_usize,
        ),
        (
            "Artifact component key",
            descriptor.owner.component_key.as_str(),
            256_usize,
        ),
        (
            "Artifact adapter session id",
            descriptor.owner.adapter_session_id.as_str(),
            256_usize,
        ),
        (
            "Artifact workspace-relative path",
            descriptor.workspace_relative_path.as_str(),
            4_096_usize,
        ),
        (
            "Artifact display name",
            descriptor.display_name.as_str(),
            512_usize,
        ),
        (
            "Artifact media type",
            descriptor.media_type.as_str(),
            256_usize,
        ),
        (
            "Artifact producer tool name",
            descriptor.producer_tool_name.as_str(),
            256_usize,
        ),
    ] {
        validate_bounded_identity(label, value, limit)?;
    }
    if !is_lower_sha256(descriptor.owner.artifact_sha256.as_str()) {
        bail!("Plugin Artifact registry owner package hash is invalid");
    }
    let relative = Path::new(descriptor.workspace_relative_path.as_str());
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || relative.file_name().and_then(|value| value.to_str())
            != Some(descriptor.display_name.as_str())
        || artifact_media_type(relative) != Some(descriptor.media_type.as_str())
    {
        bail!("Plugin Artifact registry workspace path or media type is invalid");
    }
    let created_at = chrono::DateTime::parse_from_rfc3339(descriptor.created_at.as_str())
        .context("parse persisted Plugin Artifact creation time")?;
    if created_at.timestamp() != artifact.created_at_epoch_seconds {
        bail!("Plugin Artifact registry creation time does not match");
    }
    Ok(())
}

fn validate_unique_bounded_values(label: &str, values: &[String], limit: usize) -> Result<()> {
    let mut unique = BTreeSet::new();
    for value in values {
        validate_bounded_identity(label, value.as_str(), limit)?;
        if !unique.insert(value.as_str()) {
            bail!("Plugin Artifact registry {label} values must be unique");
        }
    }
    Ok(())
}

fn validate_bounded_identity(label: &str, value: &str, max_bytes: usize) -> Result<()> {
    if value.trim() != value
        || value.is_empty()
        || value.len() > max_bytes
        || value.chars().any(char::is_control)
    {
        bail!("Plugin Artifact registry {label} is invalid");
    }
    Ok(())
}

pub(super) fn is_plugin_artifact_id(value: &str) -> bool {
    value.len() == 35
        && value.starts_with("pa_")
        && value.as_bytes().iter().skip(3).all(u8::is_ascii_hexdigit)
        && !value.as_bytes().iter().skip(3).any(u8::is_ascii_uppercase)
}

pub(super) fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value.as_bytes().iter().all(u8::is_ascii_hexdigit)
        && !value.as_bytes().iter().any(u8::is_ascii_uppercase)
}
