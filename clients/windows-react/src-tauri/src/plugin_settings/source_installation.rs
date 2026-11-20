use super::{
    hooks::{
        deactivate_hook_bundles, list_hook_bundle_records, publish_inactive_hook_bundles,
        reconcile_active_hook_bundles,
    },
    installation::{
        installation_roots, list_installation_records, local_installation_specs,
        remove_installation_roots, replace_installation_records,
    },
    presentation::safe_text,
    PluginSettingsManager,
};
use crate::{
    authentication::AuthenticationManager,
    host_lifecycle::HostManager,
    plugin_application::PluginApplicationManager,
    plugin_installer::{
        install_git_package, install_local_package, install_npm_mcp, install_npm_package_source,
        install_program_mcp as install_program_mcp_package, trusted_local_hook_bundles,
        CommittedPluginPackage, NpmMcpDraft, NpmMcpInspection, ProgramMcpDraft,
        ProgramMcpInspection,
    },
};
use chatos_local_agent_protocol::LocalPluginHookBundleRecord;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) struct LocalPluginInstallResult {
    pub(crate) plugin_id: String,
    pub(crate) display_name: String,
    pub(crate) version: String,
    pub(crate) component_count: usize,
    pub(crate) fingerprint: String,
}

pub(crate) async fn install_local_plugin_source(
    authentication: &AuthenticationManager,
    host: &HostManager,
    settings: &PluginSettingsManager,
    applications: &PluginApplicationManager,
    owner_user_id: &str,
    source_path: String,
    subdirectory: Option<String>,
    expected_fingerprint: String,
) -> Result<LocalPluginInstallResult, String> {
    let generation = authentication
        .session_generation_for_owner(owner_user_id)
        .await?;
    let _transaction = settings.installation_transactions.lock().await;
    if source_path.is_empty() || source_path.len() > 4_096 || source_path.contains('\0') {
        return Err("本地 Plugin 来源路径无效。".into());
    }
    let plugin_root = settings.plugin_root.clone();
    let package = tauri::async_runtime::spawn_blocking(move || {
        install_local_package(
            &plugin_root,
            Path::new(&source_path),
            subdirectory.as_deref(),
            &expected_fingerprint,
        )
    })
    .await
    .map_err(|_| "本地 Plugin 安装任务异常退出。".to_owned())??;
    finish_local_plugin_install(
        authentication,
        host,
        settings,
        applications,
        owner_user_id,
        generation,
        package,
    )
    .await
}

pub(crate) async fn install_git_plugin_source(
    authentication: &AuthenticationManager,
    host: &HostManager,
    settings: &PluginSettingsManager,
    applications: &PluginApplicationManager,
    owner_user_id: &str,
    repository_url: String,
    git_reference: Option<String>,
    subdirectory: Option<String>,
    expected_fingerprint: String,
) -> Result<LocalPluginInstallResult, String> {
    let _resource_permit = settings.acquire_install_resource().await?;
    let generation = authentication
        .session_generation_for_owner(owner_user_id)
        .await?;
    let _transaction = settings.installation_transactions.lock().await;
    let package = install_git_package(
        &settings.plugin_root,
        &repository_url,
        git_reference.as_deref(),
        subdirectory.as_deref(),
        &expected_fingerprint,
    )
    .await?;
    finish_local_plugin_install(
        authentication,
        host,
        settings,
        applications,
        owner_user_id,
        generation,
        package,
    )
    .await
}

pub(crate) async fn install_npm_plugin_source(
    authentication: &AuthenticationManager,
    host: &HostManager,
    settings: &PluginSettingsManager,
    applications: &PluginApplicationManager,
    owner_user_id: &str,
    package_name: String,
    exact_version: String,
    subdirectory: Option<String>,
    expected_fingerprint: String,
) -> Result<LocalPluginInstallResult, String> {
    let _resource_permit = settings.acquire_install_resource().await?;
    let generation = authentication
        .session_generation_for_owner(owner_user_id)
        .await?;
    let _transaction = settings.installation_transactions.lock().await;
    let package = install_npm_package_source(
        &settings.plugin_root,
        &package_name,
        &exact_version,
        subdirectory.as_deref(),
        &expected_fingerprint,
    )
    .await?;
    finish_local_plugin_install(
        authentication,
        host,
        settings,
        applications,
        owner_user_id,
        generation,
        package,
    )
    .await
}

pub(crate) async fn install_custom_npm_mcp(
    authentication: &AuthenticationManager,
    host: &HostManager,
    settings: &PluginSettingsManager,
    applications: &PluginApplicationManager,
    owner_user_id: &str,
    draft: NpmMcpDraft,
    expected_fingerprint: String,
) -> Result<(LocalPluginInstallResult, NpmMcpInspection), String> {
    let _resource_permit = settings.acquire_install_resource().await?;
    let generation = authentication
        .session_generation_for_owner(owner_user_id)
        .await?;
    let _transaction = settings.installation_transactions.lock().await;
    let (package, inspection) =
        install_npm_mcp(&settings.plugin_root, &draft, &expected_fingerprint).await?;
    let result = finish_local_plugin_install(
        authentication,
        host,
        settings,
        applications,
        owner_user_id,
        generation,
        package,
    )
    .await?;
    Ok((result, inspection))
}

pub(crate) async fn install_program_mcp_source(
    authentication: &AuthenticationManager,
    host: &HostManager,
    settings: &PluginSettingsManager,
    applications: &PluginApplicationManager,
    owner_user_id: &str,
    draft: ProgramMcpDraft,
    expected_fingerprint: String,
) -> Result<(LocalPluginInstallResult, ProgramMcpInspection), String> {
    let _resource_permit = settings.acquire_install_resource().await?;
    let generation = authentication
        .session_generation_for_owner(owner_user_id)
        .await?;
    let _transaction = settings.installation_transactions.lock().await;
    let (package, inspection) =
        install_program_mcp_package(&settings.plugin_root, &draft, &expected_fingerprint).await?;
    let result = finish_local_plugin_install(
        authentication,
        host,
        settings,
        applications,
        owner_user_id,
        generation,
        package,
    )
    .await?;
    Ok((result, inspection))
}

async fn finish_local_plugin_install(
    authentication: &AuthenticationManager,
    host: &HostManager,
    settings: &PluginSettingsManager,
    applications: &PluginApplicationManager,
    owner_user_id: &str,
    generation: u64,
    package: CommittedPluginPackage,
) -> Result<LocalPluginInstallResult, String> {
    if let Err(error) = authentication
        .require_session_generation(owner_user_id, generation)
        .await
    {
        package.rollback();
        return Err(error);
    }
    let plugin_id = format!(
        "local-plugin-{:x}",
        Sha256::digest(package.manifest.name.as_bytes())
    );
    let release_id = format!("local-release-{}", package.artifact_sha256);
    let hook_bundles =
        match trusted_local_hook_bundles(&package, owner_user_id, &plugin_id, &release_id) {
            Ok(bundles) => bundles,
            Err(error) => {
                package.rollback();
                return Err(error);
            }
        };
    let records = match list_installation_records(host, owner_user_id).await {
        Ok(records) => records,
        Err(error) => {
            package.rollback();
            return Err(error);
        }
    };
    let previous = records
        .clone()
        .into_iter()
        .filter(|record| record.spec.plugin_id == plugin_id)
        .collect::<Vec<_>>();
    let bundles_before = match list_hook_bundle_records(host, owner_user_id).await {
        Ok(bundles) => bundles,
        Err(error) => {
            package.rollback();
            return Err(error);
        }
    };
    let previous_roots = installation_roots(&settings.plugin_root, &previous).unwrap_or_default();
    if let Err(error) = publish_inactive_hook_bundles(host, owner_user_id, &hook_bundles).await {
        package.rollback();
        return Err(error);
    }
    if let Err(error) = authentication
        .require_session_generation(owner_user_id, generation)
        .await
    {
        package.rollback();
        return Err(error);
    }
    let specs =
        match local_installation_specs(owner_user_id, &plugin_id, &release_id, &package, &previous)
        {
            Ok(specs) => specs,
            Err(error) => {
                package.rollback();
                return Err(error);
            }
        };
    let component_count = specs.len();
    applications.reset().await;
    if let Err(error) = replace_installation_records(host, previous, specs).await {
        package.rollback();
        return Err(error);
    }
    let records_after = match list_installation_records(host, owner_user_id).await {
        Ok(records) => records,
        Err(error) => {
            let _ = deactivate_hook_bundles(host, owner_user_id).await;
            eprintln!("Hook Bundle reconciliation skipped: {error}");
            Vec::new()
        }
    };
    if !records_after.is_empty() {
        let mut bundles_to_reconcile = bundles_before;
        bundles_to_reconcile.extend(hook_bundles.iter().cloned().map(|bundle| {
            LocalPluginHookBundleRecord {
                spec: bundle,
                created_at_unix_ms: 0,
            }
        }));
        if let Err(error) = reconcile_active_hook_bundles(
            host,
            owner_user_id,
            &settings.plugin_root,
            &records_after,
            &bundles_to_reconcile,
        )
        .await
        {
            let _ = deactivate_hook_bundles(host, owner_user_id).await;
            eprintln!("Hook Bundle activation skipped: {error}");
        }
    }
    let result = LocalPluginInstallResult {
        plugin_id,
        display_name: safe_text(
            &package.manifest.interface.display_name,
            &package.manifest.name,
        ),
        version: package.manifest.version.clone(),
        component_count,
        fingerprint: package.artifact_sha256.clone(),
    };
    let installed_root = package.installation_root.clone();
    let _ = package.finish();
    let stale_roots = previous_roots
        .into_iter()
        .filter(|path| path != &installed_root)
        .collect();
    let _ = remove_installation_roots(&settings.plugin_root, stale_roots).await;
    applications.reset().await;
    Ok(result)
}
