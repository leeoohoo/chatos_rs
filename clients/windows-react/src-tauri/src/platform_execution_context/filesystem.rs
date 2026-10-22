use rusqlite::{params, Connection, OpenFlags};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    path::{Component, Path, PathBuf},
};

const MAXIMUM_STATE_BYTES: u64 = 16 * 1_024 * 1_024;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConnectorState {
    user: Option<ConnectorUser>,
    #[serde(rename = "deviceID")]
    device_id: Option<String>,
    #[serde(default)]
    workspaces: Vec<ConnectorWorkspace>,
}

#[derive(Debug, Deserialize)]
struct ConnectorUser {
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConnectorWorkspace {
    id: String,
    absolute_root: String,
    fingerprint: String,
}

pub(crate) struct ResolvedExecutionRoot {
    pub(crate) root: PathBuf,
    pub(crate) workspace_root: PathBuf,
    pub(crate) workspace_scope_id: String,
    pub(crate) device_id: String,
}

pub(crate) fn resolve_project_root(
    connector_directory: &Path,
    owner_user_id: &str,
    project_id: &str,
) -> Result<ResolvedExecutionRoot, String> {
    let (workspace_id, relative_root) = load_project(
        &connector_directory.join("Projects.sqlite3"),
        owner_user_id,
        project_id,
    )?;
    let state = load_state(&connector_directory.join("state.json"))?;
    if state.user.as_ref().map(|user| user.id.as_str()) != Some(owner_user_id) {
        return Err(project_unavailable());
    }
    let device_id = state.device_id.as_deref().ok_or_else(project_unavailable)?;
    validate_identifier(device_id)?;
    let matches = state
        .workspaces
        .iter()
        .filter(|workspace| workspace.id == workspace_id)
        .collect::<Vec<_>>();
    let [workspace] = matches.as_slice() else {
        return Err(project_unavailable());
    };
    validate_identifier(&workspace.id)?;
    validate_identifier(&workspace.fingerprint)?;
    let root = PathBuf::from(&workspace.absolute_root);
    if !root.is_absolute() || workspace.absolute_root.chars().any(char::is_control) {
        return Err(project_unavailable());
    }
    let canonical_workspace = fs::canonicalize(root).map_err(|_| project_unavailable())?;
    let candidate = if relative_root.is_empty() {
        canonical_workspace.clone()
    } else {
        canonical_workspace.join(relative_root)
    };
    let canonical_project = fs::canonicalize(candidate).map_err(|_| project_unavailable())?;
    if !canonical_project.starts_with(&canonical_workspace)
        || !canonical_project
            .metadata()
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false)
    {
        return Err(project_unavailable());
    }
    Ok(ResolvedExecutionRoot {
        root: canonical_project,
        workspace_root: canonical_workspace,
        workspace_scope_id: workspace.id.clone(),
        device_id: device_id.to_owned(),
    })
}

pub(crate) fn resolve_contact_root(
    connector_directory: &Path,
    owner_user_id: &str,
    conversation_id: &str,
) -> Result<ResolvedExecutionRoot, String> {
    let state = load_state(&connector_directory.join("state.json"))?;
    if state.user.as_ref().map(|user| user.id.as_str()) != Some(owner_user_id) {
        return Err(project_unavailable());
    }
    let device_id = state.device_id.as_deref().ok_or_else(project_unavailable)?;
    validate_identifier(device_id)?;
    validate_identifier(conversation_id)?;
    let canonical_connector =
        fs::canonicalize(connector_directory).map_err(|_| project_unavailable())?;
    let owner_scope = sha256(owner_user_id);
    let conversation_scope = sha256(conversation_id);
    let root = canonical_connector
        .join("LocalAgent")
        .join("ConversationWorkspaces")
        .join(owner_scope)
        .join(conversation_scope);
    fs::create_dir_all(&root).map_err(|_| project_unavailable())?;
    let canonical = fs::canonicalize(root).map_err(|_| project_unavailable())?;
    if !canonical.starts_with(&canonical_connector)
        || !canonical
            .metadata()
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false)
    {
        return Err(project_unavailable());
    }
    Ok(ResolvedExecutionRoot {
        root: canonical,
        workspace_root: canonical_connector,
        workspace_scope_id: format!(
            "contact:{}",
            sha256(&format!("{owner_user_id}\n{conversation_id}"))
        ),
        device_id: device_id.to_owned(),
    })
}

pub(crate) fn resolve_execution_directory(
    execution_root: &Path,
    requested_path: &str,
) -> Result<PathBuf, String> {
    let root = fs::canonicalize(execution_root).map_err(|_| invalid_path())?;
    let requested = Path::new(requested_path);
    let candidate = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        root.join(requested)
    };
    let resolved = fs::canonicalize(candidate).map_err(|_| invalid_path())?;
    if !resolved.starts_with(&root)
        || !resolved
            .metadata()
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false)
    {
        return Err(invalid_path());
    }
    Ok(resolved)
}

fn load_project(
    database_path: &Path,
    owner_user_id: &str,
    project_id: &str,
) -> Result<(String, String), String> {
    validate_identifier(owner_user_id)?;
    validate_identifier(project_id)?;
    let connection = Connection::open_with_flags(
        database_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
    )
    .map_err(|_| project_unavailable())?;
    let result = connection
        .query_row(
            "SELECT workspace_id, relative_root FROM local_project_records \
             WHERE owner_user_id = ?1 AND id = ?2 AND status = 'active'",
            params![owner_user_id, project_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .map_err(|_| project_unavailable())?;
    validate_identifier(&result.0)?;
    validate_relative_root(&result.1)?;
    Ok(result)
}

fn load_state(path: &Path) -> Result<ConnectorState, String> {
    let metadata = path.metadata().map_err(|_| project_unavailable())?;
    if !metadata.is_file() || metadata.len() > MAXIMUM_STATE_BYTES {
        return Err(project_unavailable());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(path)
        .and_then(|file| file.take(MAXIMUM_STATE_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|_| project_unavailable())?;
    if bytes.len() as u64 > MAXIMUM_STATE_BYTES {
        return Err(project_unavailable());
    }
    serde_json::from_slice(&bytes).map_err(|_| project_unavailable())
}

fn validate_relative_root(value: &str) -> Result<(), String> {
    if value.trim() != value
        || value.contains('\\')
        || value.contains(':')
        || value.chars().any(char::is_control)
        || Path::new(value)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
            && !value.is_empty()
    {
        Err(project_unavailable())
    } else {
        Ok(())
    }
}

fn validate_identifier(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > 1_024
        || value.chars().any(char::is_control)
    {
        Err(project_unavailable())
    } else {
        Ok(())
    }
}

fn sha256(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn project_unavailable() -> String {
    "The project is unavailable for this conversation.".into()
}

fn invalid_path() -> String {
    "The local platform tool field is invalid: path.".into()
}
