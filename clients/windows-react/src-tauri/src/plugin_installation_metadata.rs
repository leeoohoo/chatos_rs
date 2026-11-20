use crate::{app_data_transaction::ensure_owned_directory, host_command_client::HostCommandClient};
use chatos_local_agent_protocol::{
    GetPluginInstallationCommand, HostCommand, HostResult, ListPluginInstallationsCommand,
    LocalPluginInstallationRecord, LocalPluginInstallationSummary,
};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

const MAXIMUM_INSTALLATIONS: usize = 1_000;

pub(crate) async fn list_installation_summaries(
    client: &dyn HostCommandClient,
    owner_user_id: &str,
) -> Result<Vec<LocalPluginInstallationSummary>, String> {
    let mut installations = Vec::new();
    let mut before_updated_at_unix_ms = None;
    let mut before_installation_id = None;
    let mut observed_cursors = BTreeSet::new();
    loop {
        let result = client
            .execute(HostCommand::ListPluginInstallations(
                ListPluginInstallationsCommand {
                    owner_user_id: owner_user_id.to_owned(),
                    before_updated_at_unix_ms,
                    before_installation_id: before_installation_id.clone(),
                    limit: 200,
                },
            ))
            .await?;
        let HostResult::PluginInstallations { page } = result else {
            return Err(metadata_response_error());
        };
        if installations.len().saturating_add(page.installations.len()) > MAXIMUM_INSTALLATIONS {
            return Err("已安装 Plugin 数量超过本机目录上限。".into());
        }
        installations.extend(page.installations);
        match (
            page.next_before_updated_at_unix_ms,
            page.next_before_installation_id,
        ) {
            (Some(timestamp), Some(installation_id)) => {
                if timestamp < 0 {
                    return Err("Host 返回了无效的 Plugin 安装分页游标。".into());
                }
                if !observed_cursors.insert((timestamp, installation_id.clone())) {
                    return Err("Host 返回了重复的 Plugin 安装分页游标。".into());
                }
                before_updated_at_unix_ms = Some(timestamp);
                before_installation_id = Some(installation_id);
            }
            (None, None) => return Ok(installations),
            _ => return Err("Host 返回了无效的 Plugin 安装分页游标。".into()),
        }
    }
}

pub(crate) async fn get_installation(
    client: &dyn HostCommandClient,
    owner_user_id: &str,
    installation_id: &str,
) -> Result<LocalPluginInstallationRecord, String> {
    let result = client
        .execute(HostCommand::GetPluginInstallation(
            GetPluginInstallationCommand {
                owner_user_id: owner_user_id.to_owned(),
                installation_id: installation_id.to_owned(),
            },
        ))
        .await?;
    let HostResult::PluginInstallation { installation } = result else {
        return Err(metadata_response_error());
    };
    Ok(installation)
}

pub(crate) fn record_matches_summary(
    record: &LocalPluginInstallationRecord,
    summary: &LocalPluginInstallationSummary,
    owner_user_id: &str,
) -> bool {
    record.spec.validate().is_ok()
        && record.spec.owner_user_id == owner_user_id
        && record.spec.installation_id == summary.installation_id
        && record.spec.plugin_id == summary.plugin_id
        && record.spec.release_id == summary.release_id
        && record.spec.component_id == summary.component_id
        && record.spec.component_revision == summary.component_revision
        && record.spec.server_id == summary.server_id
        && record.spec.enabled == summary.enabled
        && record.version == summary.version
        && record.created_at_unix_ms == summary.created_at_unix_ms
        && record.updated_at_unix_ms == summary.updated_at_unix_ms
}

pub(crate) fn canonical_plugin_root(requested: &Path) -> Result<PathBuf, ()> {
    ensure_owned_directory(requested).map_err(|_| ())?;
    let root = requested.canonicalize().map_err(|_| ())?;
    root.is_dir().then_some(root).ok_or(())
}

fn metadata_response_error() -> String {
    "Local Agent Host 返回了无效的 Plugin 安装记录。".into()
}
