use super::{
    metadata_response_error, plugin_catalog_error, presentation::installed_projection,
    MAXIMUM_MANIFEST_BYTES,
};
use crate::{
    host_command_client::HostCommandClient,
    host_lifecycle::HostManager,
    plugin_installation_metadata::{
        canonical_plugin_root, get_installation, list_installation_summaries,
        record_matches_summary,
    },
    plugin_integrity::{requires_installation_integrity, verify_installation_integrity},
};
use chatos_local_agent_protocol::{
    HostCommand, HostResult, LocalPluginInstallationRecord, LocalPluginInstallationSummary,
    PutPluginInstallationCommand,
};
use chatos_plugin_management_sdk::{parse_plugin_manifest, PluginManifest};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
};

pub(super) fn group_records(
    records: Vec<LocalPluginInstallationRecord>,
    owner_user_id: &str,
) -> Result<BTreeMap<String, Vec<LocalPluginInstallationRecord>>, String> {
    let mut grouped = BTreeMap::<String, Vec<LocalPluginInstallationRecord>>::new();
    for record in records {
        record
            .spec
            .validate()
            .map_err(|_| metadata_response_error())?;
        if record.spec.owner_user_id != owner_user_id
            || record.version == 0
            || record.created_at_unix_ms < 0
            || record.updated_at_unix_ms < record.created_at_unix_ms
        {
            return Err(metadata_response_error());
        }
        grouped
            .entry(record.spec.plugin_id.clone())
            .or_default()
            .push(record);
    }
    Ok(grouped)
}

pub(crate) async fn list_installation_records_for_owner(
    host: &HostManager,
    owner_user_id: &str,
) -> Result<Vec<LocalPluginInstallationRecord>, String> {
    list_installation_records(host, owner_user_id).await
}

/// Return the user-facing Plugin version from an installed manifest without exposing
/// the installation path or any manifest credentials to callers. Installation records
/// store a monotonically increasing host record version, not the semantic Plugin version.
pub(crate) fn installed_manifest_version(
    plugin_root: &Path,
    records: &[LocalPluginInstallationRecord],
) -> Option<String> {
    installed_projection(plugin_root, records)
        .manifest
        .map(|manifest| manifest.version)
}

pub(super) async fn list_installation_records(
    host: &dyn HostCommandClient,
    owner_user_id: &str,
) -> Result<Vec<LocalPluginInstallationRecord>, String> {
    let mut records = Vec::new();
    for summary in list_installation_summaries(host, owner_user_id).await? {
        let installation = get_installation(host, owner_user_id, &summary.installation_id).await?;
        validate_record(&installation, &summary, owner_user_id)?;
        records.push(installation);
    }
    Ok(records)
}

fn validate_record(
    record: &LocalPluginInstallationRecord,
    summary: &LocalPluginInstallationSummary,
    owner_user_id: &str,
) -> Result<(), String> {
    record_matches_summary(record, summary, owner_user_id)
        .then_some(())
        .ok_or_else(metadata_response_error)
}

pub(super) fn matching_records(
    records: Vec<LocalPluginInstallationRecord>,
    plugin_id: &str,
) -> Result<Vec<LocalPluginInstallationRecord>, String> {
    let records = records
        .into_iter()
        .filter(|record| record.spec.plugin_id == plugin_id)
        .collect::<Vec<_>>();
    if records.is_empty() {
        Err("Plugin 尚未安装。".into())
    } else {
        Ok(records)
    }
}

pub(super) fn is_local_plugin_installation(record: &LocalPluginInstallationRecord) -> bool {
    record.spec.plugin_id.starts_with("local-plugin-")
        && record.spec.release_id.starts_with("local-release-")
}

pub(super) async fn set_records_enabled(
    host: &dyn HostCommandClient,
    records: Vec<LocalPluginInstallationRecord>,
    enabled: bool,
) -> Result<(), String> {
    let mut updated = Vec::<LocalPluginInstallationRecord>::new();
    for record in records {
        if record.spec.enabled == enabled {
            continue;
        }
        let mut next = record.spec.clone();
        next.enabled = enabled;
        let result = host
            .execute(HostCommand::PutPluginInstallation(
                PutPluginInstallationCommand {
                    installation: next,
                    expected_version: Some(record.version),
                },
            ))
            .await;
        match result {
            Ok(HostResult::PluginInstallation { installation }) => updated.push(installation),
            Ok(_) | Err(_) => {
                for changed in updated.into_iter().rev() {
                    let mut rollback = changed.spec.clone();
                    rollback.enabled = !enabled;
                    let _ = host
                        .execute(HostCommand::PutPluginInstallation(
                            PutPluginInstallationCommand {
                                installation: rollback,
                                expected_version: Some(changed.version),
                            },
                        ))
                        .await;
                }
                return Err("Plugin 状态更新失败，已尝试恢复原状态。".into());
            }
        }
    }
    Ok(())
}

pub(super) fn load_installed_manifest(
    plugin_root: &Path,
    record: &LocalPluginInstallationRecord,
) -> Result<PluginManifest, String> {
    let root = confined_plugin_root(plugin_root)?;
    let manifest_path = installation_manifest_path(&root, record)?;
    let installation_root = manifest_path.parent().ok_or_else(plugin_catalog_error)?;
    if requires_installation_integrity(&record.spec.component_revision) {
        verify_installation_integrity(&root, installation_root, &record.spec.release_digest)
            .map_err(|_| plugin_catalog_error())?;
    }
    let metadata = manifest_path
        .metadata()
        .map_err(|_| plugin_catalog_error())?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAXIMUM_MANIFEST_BYTES {
        return Err(plugin_catalog_error());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize + 1);
    fs::File::open(&manifest_path)
        .and_then(|file| {
            file.take(MAXIMUM_MANIFEST_BYTES + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|_| plugin_catalog_error())?;
    if bytes.len() as u64 > MAXIMUM_MANIFEST_BYTES {
        return Err(plugin_catalog_error());
    }
    let raw = String::from_utf8(bytes).map_err(|_| plugin_catalog_error())?;
    parse_plugin_manifest(&raw).map_err(|_| plugin_catalog_error())
}

pub(super) fn installation_manifest_path(
    plugin_root: &Path,
    record: &LocalPluginInstallationRecord,
) -> Result<PathBuf, String> {
    let executable = PathBuf::from(&record.spec.executable_path)
        .canonicalize()
        .map_err(|_| plugin_catalog_error())?;
    if !executable.is_file() || !executable.starts_with(plugin_root) {
        return Err(plugin_catalog_error());
    }
    let mut candidate = executable.parent().ok_or_else(plugin_catalog_error)?;
    loop {
        if !candidate.starts_with(plugin_root) {
            return Err(plugin_catalog_error());
        }
        let manifest = candidate.join("chatos.plugin.json");
        if manifest.is_file() {
            let manifest = manifest
                .canonicalize()
                .map_err(|_| plugin_catalog_error())?;
            return manifest
                .starts_with(candidate)
                .then_some(manifest)
                .ok_or_else(plugin_catalog_error);
        }
        if candidate == plugin_root {
            return Err(plugin_catalog_error());
        }
        candidate = candidate.parent().ok_or_else(plugin_catalog_error)?;
    }
}

pub(super) fn confined_plugin_root(plugin_root: &Path) -> Result<PathBuf, String> {
    canonical_plugin_root(plugin_root).map_err(|_| plugin_catalog_error())
}

pub(super) fn installation_roots(
    plugin_root: &Path,
    records: &[LocalPluginInstallationRecord],
) -> Result<BTreeSet<PathBuf>, String> {
    let root = confined_plugin_root(plugin_root)?;
    records
        .iter()
        .map(|record| {
            installation_manifest_path(&root, record).and_then(|path| {
                path.parent()
                    .filter(|directory| **directory != root && directory.starts_with(&root))
                    .map(Path::to_path_buf)
                    .ok_or_else(plugin_catalog_error)
            })
        })
        .collect()
}

pub(super) async fn remove_installation_roots(
    plugin_root: &Path,
    installation_roots: BTreeSet<PathBuf>,
) -> Result<(), String> {
    let plugin_root = plugin_root.to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        let root = confined_plugin_root(&plugin_root)?;
        for installation_root in installation_roots {
            let canonical = installation_root
                .canonicalize()
                .map_err(|_| plugin_catalog_error())?;
            if canonical == root || !canonical.starts_with(&root) || !canonical.is_dir() {
                return Err(plugin_catalog_error());
            }
            fs::remove_dir_all(&canonical).map_err(|_| "Plugin 文件删除失败。".to_owned())?;
            if let Some(parent) = canonical.parent() {
                if parent != root
                    && parent.starts_with(&root)
                    && fs::read_dir(parent)
                        .map(|mut entries| entries.next().is_none())
                        .unwrap_or(false)
                {
                    let _ = fs::remove_dir(parent);
                }
            }
        }
        Ok(())
    })
    .await
    .map_err(|_| "Plugin 文件删除任务异常退出。".to_owned())?
}
