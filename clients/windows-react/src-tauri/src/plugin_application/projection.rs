use super::{
    icons::{plugin_application_icon_url, PluginApplicationIconSource},
    manifest::{
        valid_brand_color, valid_capability, valid_environment_name, valid_health_path,
        valid_optional_text, valid_package_executable, validate_text,
    },
    plugin_catalog_error, PluginManifest, PluginRuntimeContext, PluginUiContribution,
    PluginUiRuntime, PresentedPluginApplication, TrustedAgentPluginOption,
    HOOKS_ONLY_COMPONENT_KEY, MAXIMUM_BRIDGE_CAPABILITIES, MAXIMUM_UI_ASSETS,
    SKILLS_ONLY_COMPONENT_KEY,
};
use chatos_local_agent_protocol::LocalPluginInstallationRecord;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub(super) fn project_agent_plugin(
    record: &LocalPluginInstallationRecord,
    manifest: &PluginManifest,
) -> Result<TrustedAgentPluginOption, String> {
    let component_key = &record.spec.server_id;
    if is_non_executable_component(component_key) {
        let valid_component = match component_key.as_str() {
            SKILLS_ONLY_COMPONENT_KEY => !manifest.skills.is_empty(),
            HOOKS_ONLY_COMPONENT_KEY => !manifest.hooks.is_empty() && manifest.skills.is_empty(),
            _ => false,
        };
        if !valid_component
            || !manifest.mcp_servers.is_empty()
            || !record.spec.args.is_empty()
            || !record.spec.environment_secret_refs.is_empty()
            || record.spec.tool_prefix.is_some()
            || record.spec.allowed_tools.is_some()
            || Path::new(&record.spec.executable_path)
                .file_name()
                .and_then(|value| value.to_str())
                != Some("chatos.plugin.json")
        {
            return Err(plugin_catalog_error());
        }
        let display_name = manifest
            .interface
            .as_ref()
            .and_then(|interface| interface.display_name.as_deref())
            .unwrap_or(&manifest.name)
            .trim()
            .to_owned();
        let description = manifest.description.trim().to_owned();
        validate_text(&display_name, 240)?;
        if !valid_optional_text(&description, 16 * 1_024) {
            return Err(plugin_catalog_error());
        }
        return Ok(TrustedAgentPluginOption {
            installation_id: record.spec.installation_id.clone(),
            plugin_id: record.spec.plugin_id.clone(),
            component_key: component_key.clone(),
            component_revision: record.spec.component_revision.clone(),
            release_id: record.spec.release_id.clone(),
            release_digest: record.spec.release_digest.clone(),
            manifest_version: manifest.version.clone(),
            display_name,
            description,
        });
    }
    let server = manifest
        .mcp_servers
        .get(component_key)
        .ok_or_else(plugin_catalog_error)?;
    if server.kind != "stdio"
        || !valid_package_executable(&server.bin)
        || !executable_matches_manifest_bin(&record.spec.executable_path, &server.bin)
        || server.args.len() > 256
        || server
            .args
            .iter()
            .any(|argument| !valid_optional_text(argument, 8_192))
        || server.env.len() > 256
        || server
            .env
            .iter()
            .any(|(key, value)| !valid_environment_name(key) || !valid_optional_text(value, 8_192))
    {
        return Err(plugin_catalog_error());
    }
    let has_spawn_permission = manifest.permissions.iter().any(|permission| {
        permission.permission == "process.spawn"
            && permission.required
            && (permission.components.is_empty() || permission.components.contains(component_key))
    });
    if !has_spawn_permission {
        return Err(plugin_catalog_error());
    }
    let display_name = server
        .agent
        .as_ref()
        .map(|presentation| presentation.display_name.as_str())
        .or_else(|| {
            manifest
                .interface
                .as_ref()
                .and_then(|interface| interface.display_name.as_deref())
        })
        .unwrap_or(&manifest.name)
        .trim()
        .to_owned();
    let description = server
        .agent
        .as_ref()
        .map(|presentation| presentation.description.as_str())
        .unwrap_or(&manifest.description)
        .trim()
        .to_owned();
    validate_text(&display_name, 240)?;
    if !valid_optional_text(&description, 16 * 1_024) {
        return Err(plugin_catalog_error());
    }
    Ok(TrustedAgentPluginOption {
        installation_id: record.spec.installation_id.clone(),
        plugin_id: record.spec.plugin_id.clone(),
        component_key: component_key.clone(),
        component_revision: record.spec.component_revision.clone(),
        release_id: record.spec.release_id.clone(),
        release_digest: record.spec.release_digest.clone(),
        manifest_version: manifest.version.clone(),
        display_name,
        description,
    })
}

pub(super) fn is_non_executable_component(component_key: &str) -> bool {
    matches!(
        component_key,
        SKILLS_ONLY_COMPONENT_KEY | HOOKS_ONLY_COMPONENT_KEY
    )
}

fn executable_matches_manifest_bin(executable_path: &str, manifest_bin: &str) -> bool {
    let Some(file_name) = Path::new(executable_path)
        .file_name()
        .and_then(|value| value.to_str())
    else {
        return false;
    };
    file_name == manifest_bin
        || file_name
            .strip_suffix(".exe")
            .is_some_and(|stem| stem == manifest_bin)
}

pub(super) fn project_applications(
    plugin_root: &Path,
    installation_root: &Path,
    record: &LocalPluginInstallationRecord,
    manifest: &PluginManifest,
    icon_session_token: Option<&str>,
    icons: &mut BTreeMap<String, PluginApplicationIconSource>,
) -> Vec<PresentedPluginApplication> {
    manifest
        .ui
        .iter()
        .filter(|contribution| contribution.surface.as_deref() == Some("workbench"))
        .filter_map(|contribution| {
            project_application(
                plugin_root,
                installation_root,
                record,
                manifest,
                contribution,
                icon_session_token,
                icons,
            )
            .ok()
        })
        .collect()
}

pub(super) fn project_application(
    plugin_root: &Path,
    installation_root: &Path,
    record: &LocalPluginInstallationRecord,
    manifest: &PluginManifest,
    contribution: &PluginUiContribution,
    icon_session_token: Option<&str>,
    icons: &mut BTreeMap<String, PluginApplicationIconSource>,
) -> Result<PresentedPluginApplication, String> {
    validate_text(&record.spec.plugin_id, 1_024)?;
    validate_text(&contribution.component_key, 1_024)?;
    let source =
        resolve_installed_file(plugin_root, installation_root, contribution.source.path())?;
    if !source.is_file() {
        return Err(plugin_catalog_error());
    }
    validate_ui_files(plugin_root, installation_root, contribution)?;
    if contribution.bridge_capabilities.len() > MAXIMUM_BRIDGE_CAPABILITIES {
        return Err(plugin_catalog_error());
    }
    let mut bridge_capabilities = BTreeSet::new();
    for capability in &contribution.bridge_capabilities {
        if !valid_capability(capability) || !bridge_capabilities.insert(capability.clone()) {
            return Err(plugin_catalog_error());
        }
    }
    if let Some(runtime) = &contribution.runtime {
        validate_runtime(runtime)?;
        let has_spawn_permission = manifest.permissions.iter().any(|permission| {
            permission.permission == "process.spawn"
                && permission.required
                && (permission.components.is_empty()
                    || permission.components.contains(&contribution.component_key))
        });
        if !has_spawn_permission {
            return Err(plugin_catalog_error());
        }
    }
    let runtime_context = manifest
        .runtime_context
        .as_ref()
        .filter(|context| context.components.contains(&contribution.component_key));
    if let Some(context) = runtime_context {
        validate_runtime_context(context)?;
    }
    let contribution_title = contribution
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let interface_title = manifest
        .interface
        .as_ref()
        .and_then(|interface| interface.display_name.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let display_name = contribution_title
        .or(interface_title)
        .unwrap_or(&manifest.name)
        .to_owned();
    validate_text(&display_name, 1_024)?;
    let description = manifest.description.trim().to_owned();
    if !valid_optional_text(&description, 16 * 1_024) {
        return Err(plugin_catalog_error());
    }
    let brand_color = manifest
        .interface
        .as_ref()
        .and_then(|interface| interface.brand_color.as_deref())
        .filter(|value| valid_brand_color(value))
        .map(str::to_owned);
    let icon_preview_url = icon_session_token.and_then(|session_token| {
        manifest
            .interface
            .as_ref()
            .and_then(|interface| interface.logo.as_ref())
            .and_then(|logo| {
                let source =
                    resolve_installed_file(plugin_root, installation_root, logo.path()).ok()?;
                source.is_file().then(|| PluginApplicationIconSource {
                    plugin_root: plugin_root.to_path_buf(),
                    installation_root: installation_root.to_path_buf(),
                    relative: logo.path().to_owned(),
                })
            })
            .map(|source| {
                let icon_token = uuid::Uuid::new_v4().to_string().to_lowercase();
                icons.insert(icon_token.clone(), source);
                plugin_application_icon_url(session_token, &icon_token)
            })
    });
    Ok(PresentedPluginApplication {
        plugin_id: record.spec.plugin_id.clone(),
        component_key: contribution.component_key.clone(),
        display_name,
        description,
        brand_color,
        icon_preview_url,
        requires_local_runtime: contribution.runtime.is_some(),
        context_scope: runtime_context.map(|context| context.scope.clone()),
        missing_context: runtime_context.map(|context| context.missing_context.clone()),
        bridge_capabilities: bridge_capabilities.into_iter().collect(),
        release_id: record.spec.release_id.clone(),
        release_digest: record.spec.release_digest.clone(),
    })
}

fn validate_ui_files(
    plugin_root: &Path,
    installation_root: &Path,
    contribution: &PluginUiContribution,
) -> Result<(), String> {
    let source = contribution.source.path();
    if !source.starts_with("./ui/")
        || !source
            .rsplit_once('.')
            .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("html"))
        || contribution.assets.len() > MAXIMUM_UI_ASSETS
    {
        return Err(plugin_catalog_error());
    }
    let mut observed = BTreeSet::new();
    for asset in &contribution.assets {
        let path = asset.path();
        let extension = path
            .rsplit_once('.')
            .map(|(_, extension)| extension.to_ascii_lowercase());
        let supported = extension.as_deref().is_some_and(|extension| {
            matches!(
                extension,
                "js" | "mjs"
                    | "css"
                    | "json"
                    | "svg"
                    | "png"
                    | "jpg"
                    | "jpeg"
                    | "webp"
                    | "gif"
                    | "woff"
                    | "woff2"
            )
        });
        if !path.starts_with("./ui/")
            || !supported
            || path == source
            || !observed.insert(path)
            || !resolve_installed_file(plugin_root, installation_root, path)?.is_file()
        {
            return Err(plugin_catalog_error());
        }
    }
    Ok(())
}

pub(super) fn resolve_installed_file(
    plugin_root: &Path,
    installation_root: &Path,
    relative: &str,
) -> Result<PathBuf, String> {
    if relative.is_empty()
        || relative.len() > 4_096
        || relative.chars().any(char::is_control)
        || Path::new(relative).is_absolute()
    {
        return Err(plugin_catalog_error());
    }
    let canonical = installation_root
        .join(relative)
        .canonicalize()
        .map_err(|_| plugin_catalog_error())?;
    if !canonical.starts_with(installation_root) || !canonical.starts_with(plugin_root) {
        return Err(plugin_catalog_error());
    }
    Ok(canonical)
}

fn validate_runtime(runtime: &PluginUiRuntime) -> Result<(), String> {
    if runtime.kind != "local_http"
        || !valid_package_executable(&runtime.bin)
        || runtime.args.len() > 256
        || runtime
            .args
            .iter()
            .any(|argument| !valid_optional_text(argument, 8_192))
        || runtime
            .health_path
            .as_deref()
            .is_some_and(|path| !valid_health_path(path))
        || runtime
            .launch_timeout_ms
            .is_some_and(|timeout| !(100..=120_000).contains(&timeout))
    {
        return Err(plugin_catalog_error());
    }
    Ok(())
}

fn validate_runtime_context(context: &PluginRuntimeContext) -> Result<(), String> {
    let valid_scope = matches!(context.scope.as_str(), "device" | "workspace" | "project");
    let valid_storage = matches!(
        context.storage_isolation.as_str(),
        "plugin" | "workspace" | "project"
    );
    let valid_missing = matches!(context.missing_context.as_str(), "device" | "reject");
    let valid_fields = context
        .required
        .iter()
        .chain(&context.optional)
        .all(|field| {
            matches!(
                field.as_str(),
                "project.id" | "workspace.id" | "workspace.root"
            )
        });
    if !valid_scope
        || !valid_storage
        || !valid_missing
        || context.components.len() > 256
        || context.required.len() > 16
        || context.optional.len() > 16
        || !valid_fields
    {
        return Err(plugin_catalog_error());
    }
    Ok(())
}
