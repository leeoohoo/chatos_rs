use super::{
    bounded_file, create_confined_directory, plugin_catalog_error, plugin_runtime_error,
    plugin_unavailable, sha256, validate_text, PluginApplicationContextSelection,
    PresentedPluginApplication, ResolvedPluginApplication,
};
use crate::platform_execution_context::resolve_project_root;
use rusqlite::{params, Connection, OpenFlags};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
};

const MAXIMUM_CONNECTOR_STATE_BYTES: u64 = 16 * 1_024 * 1_024;

#[derive(Clone)]
pub(super) struct ResolvedHostContext {
    pub(super) owner_user_id: String,
    pub(super) device_id: String,
    pub(super) workspace_id: Option<String>,
    pub(super) workspace_root: Option<PathBuf>,
    pub(super) project_id: Option<String>,
    pub(super) project_name: Option<String>,
}

pub(super) struct ResolvedRuntimeContext {
    pub(super) data_directory: PathBuf,
    pub(super) cache_directory: PathBuf,
    pub(super) environment: BTreeMap<String, String>,
    pub(super) scope_key: String,
}

#[derive(Deserialize)]
struct ConnectorState {
    user: Option<ConnectorUser>,
    #[serde(rename = "deviceID")]
    device_id: Option<String>,
}

#[derive(Deserialize)]
struct ConnectorUser {
    id: String,
}

pub(super) fn resolve_host_context(
    runtime_root: &Path,
    owner_user_id: &str,
    application: &PresentedPluginApplication,
    selection: &PluginApplicationContextSelection,
) -> Result<ResolvedHostContext, String> {
    let requires_project = matches!(
        application.context_scope.as_deref(),
        Some("project" | "workspace")
    );
    match selection {
        PluginApplicationContextSelection::None if requires_project => {
            return Err("请先选择 Plugin 应用使用的项目。".into())
        }
        PluginApplicationContextSelection::Device
            if !requires_project || application.missing_context.as_deref() != Some("device") =>
        {
            return Err("Plugin 应用不允许使用这个上下文。".into())
        }
        PluginApplicationContextSelection::Project { .. } if !requires_project => {
            return Err("Plugin 应用不允许使用这个上下文。".into())
        }
        _ => {}
    }
    let connector = runtime_root.parent().ok_or_else(plugin_runtime_error)?;
    let (device_id, connector_owner) = load_device_context(&connector.join("state.json"))?;
    if connector_owner != owner_user_id {
        return Err(plugin_unavailable());
    }
    match selection {
        PluginApplicationContextSelection::None | PluginApplicationContextSelection::Device => {
            Ok(ResolvedHostContext {
                owner_user_id: owner_user_id.to_owned(),
                device_id,
                workspace_id: None,
                workspace_root: None,
                project_id: None,
                project_name: None,
            })
        }
        PluginApplicationContextSelection::Project { project_id } => {
            let resolved = resolve_project_root(connector, owner_user_id, project_id)?;
            let project_name = load_project_name(
                &connector.join("Projects.sqlite3"),
                owner_user_id,
                project_id,
            )?;
            Ok(ResolvedHostContext {
                owner_user_id: owner_user_id.to_owned(),
                device_id: resolved.device_id,
                workspace_id: Some(resolved.workspace_scope_id),
                workspace_root: Some(resolved.root),
                project_id: Some(project_id.clone()),
                project_name: Some(project_name),
            })
        }
    }
}

fn load_device_context(path: &Path) -> Result<(String, String), String> {
    let bytes = bounded_file(path, MAXIMUM_CONNECTOR_STATE_BYTES)?;
    let state =
        serde_json::from_slice::<ConnectorState>(&bytes).map_err(|_| plugin_runtime_error())?;
    let owner = state
        .user
        .map(|user| user.id)
        .ok_or_else(plugin_unavailable)?;
    let device = state.device_id.ok_or_else(plugin_unavailable)?;
    validate_text(&owner, 512)?;
    validate_text(&device, 1_024)?;
    Ok((device, owner))
}

fn load_project_name(
    database: &Path,
    owner_user_id: &str,
    project_id: &str,
) -> Result<String, String> {
    let connection = Connection::open_with_flags(
        database,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
    )
    .map_err(|_| plugin_unavailable())?;
    let name = connection
        .query_row(
            "SELECT name FROM local_project_records \
             WHERE owner_user_id = ?1 AND id = ?2 AND status = 'active'",
            params![owner_user_id, project_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|_| plugin_unavailable())?;
    validate_text(&name, 4_096)?;
    Ok(name)
}

pub(super) fn resolve_runtime_context(
    runtime_root: &Path,
    resolved: &ResolvedPluginApplication,
    host: &ResolvedHostContext,
) -> Result<ResolvedRuntimeContext, String> {
    let plugin_hash = sha256(&resolved.record.spec.plugin_id);
    let user_hash = sha256(&host.owner_user_id);
    let user_data = PathBuf::from("data")
        .join("users")
        .join(&user_hash)
        .join(&plugin_hash);
    let user_cache = PathBuf::from("cache")
        .join("users")
        .join(&user_hash)
        .join(&plugin_hash);
    let Some(declaration) = resolved
        .manifest
        .runtime_context
        .as_ref()
        .filter(|context| {
            context
                .components
                .contains(&resolved.contribution.component_key)
        })
    else {
        return Ok(ResolvedRuntimeContext {
            data_directory: create_confined_directory(runtime_root, &user_data)?,
            cache_directory: create_confined_directory(runtime_root, &user_cache)?,
            environment: BTreeMap::new(),
            scope_key: format!("user:{user_hash}"),
        });
    };
    let requested = declaration
        .required
        .iter()
        .chain(&declaration.optional)
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let missing = declaration
        .required
        .iter()
        .filter(|field| match field.as_str() {
            "project.id" => host.project_id.is_none(),
            "workspace.id" => host.workspace_id.is_none(),
            "workspace.root" => host.workspace_root.is_none(),
            _ => true,
        });
    let missing = missing.cloned().collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!("Plugin 运行缺少必需上下文：{}", missing.join(", ")));
    }
    let resolved_scope = match declaration.scope.as_str() {
        "device" => ("device", format!("device:{}", host.device_id)),
        "workspace" => host
            .workspace_id
            .as_ref()
            .map(|id| ("workspace", format!("workspace:{id}")))
            .or_else(|| {
                (declaration.missing_context == "device")
                    .then(|| ("device", format!("device:{}", host.device_id)))
            })
            .ok_or_else(|| "Plugin 需要项目或工作区上下文。".to_owned())?,
        "project" => host
            .project_id
            .as_ref()
            .map(|id| ("project", format!("project:{id}")))
            .or_else(|| {
                (declaration.missing_context == "device")
                    .then(|| ("device", format!("device:{}", host.device_id)))
            })
            .ok_or_else(|| "Plugin 需要项目或工作区上下文。".to_owned())?,
        _ => return Err(plugin_catalog_error()),
    };
    let storage_scope = match declaration.storage_isolation.as_str() {
        "plugin" => ("plugin", "plugin".to_owned()),
        "workspace" => host
            .workspace_id
            .as_ref()
            .map(|id| ("workspace", format!("workspace:{id}")))
            .or_else(|| {
                (declaration.missing_context == "device")
                    .then(|| ("device", format!("device:{}", host.device_id)))
            })
            .unwrap_or_else(|| (resolved_scope.0, resolved_scope.1.clone())),
        "project" => host
            .project_id
            .as_ref()
            .map(|id| ("project", format!("project:{id}")))
            .or_else(|| {
                (declaration.missing_context == "device")
                    .then(|| ("device", format!("device:{}", host.device_id)))
            })
            .unwrap_or_else(|| (resolved_scope.0, resolved_scope.1.clone())),
        _ => return Err(plugin_catalog_error()),
    };
    let scope_hash = sha256(&resolved_scope.1);
    let storage_hash = sha256(&storage_scope.1);
    let (data_relative, cache_relative) = if declaration.storage_isolation == "plugin" {
        (user_data, user_cache)
    } else {
        (
            user_data
                .join("scopes")
                .join(storage_scope.0)
                .join(&storage_hash),
            user_cache
                .join("scopes")
                .join(storage_scope.0)
                .join(&storage_hash),
        )
    };
    let mut environment = BTreeMap::from([
        ("CHATOS_CONTEXT_SCOPE".into(), resolved_scope.0.into()),
        ("CHATOS_CONTEXT_SCOPE_ID".into(), scope_hash.clone()),
    ]);
    if requested.contains("project.id") {
        if let Some(project_id) = &host.project_id {
            environment.insert("CHATOS_PROJECT_ID".into(), project_id.clone());
        }
    }
    if requested.contains("workspace.id") {
        if let Some(workspace_id) = &host.workspace_id {
            environment.insert("CHATOS_WORKSPACE_ID".into(), workspace_id.clone());
        }
    }
    if requested.contains("workspace.root") {
        if let Some(workspace_root) = &host.workspace_root {
            environment.insert(
                "CHATOS_WORKSPACE".into(),
                workspace_root.to_string_lossy().into_owned(),
            );
        }
    }
    if let Some(project_name) = &host.project_name {
        environment.insert("CHATOS_PROJECT_NAME".into(), project_name.clone());
    }
    Ok(ResolvedRuntimeContext {
        data_directory: create_confined_directory(runtime_root, &data_relative)?,
        cache_directory: create_confined_directory(runtime_root, &cache_relative)?,
        environment,
        scope_key: format!("user:{user_hash}:{}:{scope_hash}", resolved_scope.0),
    })
}
