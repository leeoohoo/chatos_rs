use super::installation::list_installation_records;
use crate::{
    host_command_client::HostCommandClient, plugin_integrity::verify_installation_integrity,
};
use chatos_local_agent_protocol::{
    HostCommand, HostResult, ListPluginHookBundleRecordsCommand, ListPluginHookBundlesCommand,
    LocalPluginHookBundleRecord, LocalPluginHookBundleSpec, LocalPluginInstallationRecord,
    PutPluginHookBundleCommand, SetActivePluginHookBundlesCommand,
};
use std::{collections::BTreeSet, path::Path};

pub(super) async fn publish_inactive_hook_bundles(
    host: &dyn HostCommandClient,
    owner_user_id: &str,
    bundles: &[LocalPluginHookBundleSpec],
) -> Result<(), String> {
    let result = host
        .execute(HostCommand::SetActivePluginHookBundles(
            SetActivePluginHookBundlesCommand {
                owner_user_id: owner_user_id.to_owned(),
                bundle_ids: Vec::new(),
            },
        ))
        .await?;
    if !matches!(
        result,
        HostResult::ActivePluginHookBundlesUpdated { active_bundle_ids }
            if active_bundle_ids.is_empty()
    ) {
        return Err("Local Agent Host 未确认 Hook Bundle 安全停用。".into());
    }
    for bundle in bundles {
        let result = host
            .execute(HostCommand::PutPluginHookBundle(
                PutPluginHookBundleCommand {
                    bundle: bundle.clone(),
                },
            ))
            .await?;
        if !matches!(
            result,
            HostResult::PluginHookBundle { bundle: record }
                if record.spec == *bundle
        ) {
            return Err("Local Agent Host 返回了无效的 Hook Bundle 记录。".into());
        }
    }
    Ok(())
}

pub(super) async fn list_hook_bundle_records(
    host: &dyn HostCommandClient,
    owner_user_id: &str,
) -> Result<Vec<LocalPluginHookBundleRecord>, String> {
    match host
        .execute(HostCommand::ListPluginHookBundles(
            ListPluginHookBundlesCommand {
                owner_user_id: owner_user_id.to_owned(),
            },
        ))
        .await?
    {
        HostResult::PluginHookBundles { bundles } => Ok(bundles),
        _ => Err("Local Agent Host 返回了无效的 Hook Bundle 清单。".into()),
    }
}

pub(super) async fn list_all_hook_bundle_records(
    host: &dyn HostCommandClient,
    owner_user_id: &str,
) -> Result<Vec<LocalPluginHookBundleRecord>, String> {
    match host
        .execute(HostCommand::ListPluginHookBundleRecords(
            ListPluginHookBundleRecordsCommand {
                owner_user_id: owner_user_id.to_owned(),
            },
        ))
        .await?
    {
        HostResult::PluginHookBundles { bundles } => Ok(bundles),
        _ => Err("Local Agent Host 返回了无效的 Hook Bundle 记录清单。".into()),
    }
}

pub(super) async fn reconcile_current_hook_bundles(
    host: &dyn HostCommandClient,
    plugin_root: &Path,
    owner_user_id: &str,
) -> Result<(), String> {
    let records = list_installation_records(host, owner_user_id).await?;
    let bundles = list_all_hook_bundle_records(host, owner_user_id).await?;
    reconcile_active_hook_bundles(host, owner_user_id, plugin_root, &records, &bundles).await
}

pub(super) async fn reconcile_active_hook_bundles(
    host: &dyn HostCommandClient,
    owner_user_id: &str,
    plugin_root: &Path,
    records: &[LocalPluginInstallationRecord],
    candidates: &[LocalPluginHookBundleRecord],
) -> Result<(), String> {
    let mut active_ids = BTreeSet::new();
    for bundle in candidates {
        if bundle.spec.owner_user_id != owner_user_id
            || !bundle_matches_installation(plugin_root, records, &bundle.spec)
        {
            continue;
        }
        active_ids.insert(bundle.spec.bundle_id.clone());
    }
    let bundle_ids = active_ids.into_iter().collect::<Vec<_>>();
    match host
        .execute(HostCommand::SetActivePluginHookBundles(
            SetActivePluginHookBundlesCommand {
                owner_user_id: owner_user_id.to_owned(),
                bundle_ids: bundle_ids.clone(),
            },
        ))
        .await?
    {
        HostResult::ActivePluginHookBundlesUpdated { active_bundle_ids }
            if active_bundle_ids == bundle_ids =>
        {
            Ok(())
        }
        _ => Err("Local Agent Host 未确认 Hook Bundle reconciliation。".into()),
    }
}

pub(super) async fn deactivate_hook_bundles(
    host: &dyn HostCommandClient,
    owner_user_id: &str,
) -> Result<(), String> {
    match host
        .execute(HostCommand::SetActivePluginHookBundles(
            SetActivePluginHookBundlesCommand {
                owner_user_id: owner_user_id.to_owned(),
                bundle_ids: Vec::new(),
            },
        ))
        .await?
    {
        HostResult::ActivePluginHookBundlesUpdated { active_bundle_ids }
            if active_bundle_ids.is_empty() =>
        {
            Ok(())
        }
        _ => Err("Local Agent Host 未确认 Hook Bundle 停用。".into()),
    }
}

fn bundle_matches_installation(
    plugin_root: &Path,
    records: &[LocalPluginInstallationRecord],
    bundle: &LocalPluginHookBundleSpec,
) -> bool {
    if !bundle.enabled || bundle.validate().is_err() {
        return false;
    }
    let Ok(bundle_root) = Path::new(&bundle.installation_root).canonicalize() else {
        return false;
    };
    let Some(record) = records.iter().find(|record| {
        record.spec.owner_user_id == bundle.owner_user_id
            && record.spec.plugin_id == bundle.plugin_id
            && record.spec.release_id == bundle.release_id
            && record.spec.enabled
            && record.spec.release_digest == format!("sha256:{}", bundle.artifact_sha256)
            && record
                .spec
                .working_directory
                .as_deref()
                .is_some_and(|path| {
                    Path::new(path)
                        .canonicalize()
                        .ok()
                        .is_some_and(|root| root == bundle_root)
                })
    }) else {
        return false;
    };
    let Some(working_directory) = record.spec.working_directory.as_deref() else {
        return false;
    };
    let Ok(installation_root) = Path::new(working_directory).canonicalize() else {
        return false;
    };
    verify_installation_integrity(plugin_root, &installation_root, &record.spec.release_digest)
        .is_ok()
}
