use crate::{
    app_data_transaction::create_owned_file, authentication::AuthenticationManager,
    connector_state, host_lifecycle::validate_owner, workspace_index::open_project_registry,
};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager, State};

const LEGACY_PROJECT_TYPE: &str = "software_development";
const JOURNAL_SCHEMA_VERSION: u32 = 1;
const MAXIMUM_JOURNALS: usize = 64;
const MAXIMUM_JOURNAL_BYTES: u64 = 1_024 * 1_024;
const MAXIMUM_PENDING_SELECTIONS: usize = 32;
const SELECTION_LIFETIME: Duration = Duration::from_secs(15 * 60);

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTypeDefinition {
    key: &'static str,
    label: &'static str,
    description: &'static str,
    category_key: &'static str,
    category_label: &'static str,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDirectorySelection {
    selection_id: String,
    directory_name: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreatedLocalProject {
    id: String,
    name: String,
    revision: i64,
    project_type_key: String,
}

#[derive(Clone, Debug)]
struct PendingSelection {
    owner_user_id: String,
    presentation_id: String,
    canonical_root: PathBuf,
    created_at: Instant,
    saving: bool,
}

#[derive(Default)]
pub struct ProjectCreationManager {
    selections: Mutex<HashMap<String, PendingSelection>>,
    closed_presentations: Mutex<HashMap<String, Instant>>,
    transaction: Arc<Mutex<()>>,
}

impl ProjectCreationManager {
    pub(crate) fn transaction_lock(&self) -> Arc<Mutex<()>> {
        Arc::clone(&self.transaction)
    }

    fn stage(
        &self,
        owner_user_id: &str,
        presentation_id: &str,
        canonical_root: PathBuf,
        directory_name: String,
    ) -> Result<ProjectDirectorySelection, String> {
        let presentation_key = presentation_key(owner_user_id, presentation_id);
        let mut closed = self
            .closed_presentations
            .lock()
            .map_err(|_| creation_error())?;
        closed.retain(|_, closed_at| closed_at.elapsed() <= SELECTION_LIFETIME);
        if closed.contains_key(&presentation_key) {
            return Err(selection_expired());
        }
        drop(closed);
        let mut selections = self.selections.lock().map_err(|_| creation_error())?;
        selections.retain(|_, selection| {
            selection.created_at.elapsed() <= SELECTION_LIFETIME
                && !(selection.owner_user_id == owner_user_id
                    && selection.presentation_id == presentation_id)
        });
        if selections.len() >= MAXIMUM_PENDING_SELECTIONS {
            return Err("待处理的文件夹选择过多，请关闭其他创建窗口后重试。".into());
        }
        let selection_id = format!("folder-selection-{}", uuid::Uuid::new_v4());
        selections.insert(
            selection_id.clone(),
            PendingSelection {
                owner_user_id: owner_user_id.to_owned(),
                presentation_id: presentation_id.to_owned(),
                canonical_root,
                created_at: Instant::now(),
                saving: false,
            },
        );
        Ok(ProjectDirectorySelection {
            selection_id,
            directory_name,
        })
    }

    fn begin_save(
        &self,
        owner_user_id: &str,
        presentation_id: &str,
        selection_id: &str,
    ) -> Result<PendingSelection, String> {
        let mut selections = self.selections.lock().map_err(|_| creation_error())?;
        selections.retain(|_, selection| selection.created_at.elapsed() <= SELECTION_LIFETIME);
        let selection = selections
            .get_mut(selection_id)
            .ok_or_else(selection_expired)?;
        if selection.owner_user_id != owner_user_id
            || selection.presentation_id != presentation_id
            || selection.saving
        {
            return Err(selection_expired());
        }
        selection.saving = true;
        Ok(selection.clone())
    }

    fn finish_save(&self, selection_id: &str, succeeded: bool) {
        let Ok(mut selections) = self.selections.lock() else {
            return;
        };
        if succeeded {
            selections.remove(selection_id);
        } else if let Some(selection) = selections.get_mut(selection_id) {
            selection.saving = false;
        }
    }

    fn discard(&self, owner_user_id: &str, presentation_id: &str) {
        if let Ok(mut closed) = self.closed_presentations.lock() {
            closed.retain(|_, closed_at| closed_at.elapsed() <= SELECTION_LIFETIME);
            if closed.len() >= MAXIMUM_PENDING_SELECTIONS * 2 {
                if let Some(oldest) = closed
                    .iter()
                    .min_by_key(|(_, closed_at)| *closed_at)
                    .map(|(key, _)| key.clone())
                {
                    closed.remove(&oldest);
                }
            }
            closed.insert(
                presentation_key(owner_user_id, presentation_id),
                Instant::now(),
            );
        }
        let Ok(mut selections) = self.selections.lock() else {
            return;
        };
        selections.retain(|_, selection| {
            selection.owner_user_id != owner_user_id
                || selection.presentation_id != presentation_id
                || selection.saving
        });
    }
}

fn presentation_key(owner_user_id: &str, presentation_id: &str) -> String {
    format!("{owner_user_id}\0{presentation_id}")
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct WorkspaceGrant {
    id: String,
    absolute_root: String,
    fingerprint: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct JournalProject {
    id: String,
    name: String,
    description: String,
    project_type_key: String,
    created_at_unix_ms: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct CreationJournal {
    schema_version: u32,
    owner_user_id: String,
    workspace: WorkspaceGrant,
    project: JournalProject,
}

#[derive(Clone, Debug)]
struct RegistryBinding {
    workspace_id: String,
    relative_root: String,
}

#[tauri::command]
pub fn list_project_types() -> Vec<ProjectTypeDefinition> {
    PROJECT_TYPES.to_vec()
}

#[tauri::command]
pub async fn pick_project_directory(
    owner_user_id: String,
    presentation_id: String,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, ProjectCreationManager>,
) -> Result<Option<ProjectDirectorySelection>, String> {
    validate_owner(&owner_user_id)?;
    validate_route_identifier(&presentation_id, "presentationId")?;
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    let selected = rfd::AsyncFileDialog::new()
        .set_title("选择项目根目录")
        .pick_folder()
        .await;
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    let Some(selected) = selected else {
        return Ok(None);
    };
    let requested = selected.path().to_path_buf();
    let (canonical_root, directory_name) =
        tauri::async_runtime::spawn_blocking(move || prepare_selected_directory(&requested))
            .await
            .map_err(|_| "文件夹选择任务异常退出。".to_owned())??;
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    manager
        .stage(
            &owner_user_id,
            &presentation_id,
            canonical_root,
            directory_name,
        )
        .map(Some)
}

#[tauri::command]
pub async fn discard_project_directory_selection(
    owner_user_id: String,
    presentation_id: String,
    manager: State<'_, ProjectCreationManager>,
) -> Result<(), String> {
    validate_owner(&owner_user_id)?;
    validate_route_identifier(&presentation_id, "presentationId")?;
    manager.discard(&owner_user_id, &presentation_id);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn create_local_project(
    app: AppHandle,
    owner_user_id: String,
    presentation_id: String,
    selection_id: String,
    name: String,
    description: String,
    project_type_key: String,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, ProjectCreationManager>,
) -> Result<CreatedLocalProject, String> {
    validate_owner(&owner_user_id)?;
    validate_route_identifier(&presentation_id, "presentationId")?;
    validate_route_identifier(&selection_id, "selectionId")?;
    let name = validate_project_name(name)?;
    let description = validate_project_description(description)?;
    validate_project_type(&project_type_key)?;
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    let selection = manager.begin_save(&owner_user_id, &presentation_id, &selection_id)?;
    let transaction_lock = Arc::clone(&manager.transaction);
    let app_data_directory = app.path().app_data_dir().map_err(|_| creation_error())?;
    let owner = owner_user_id.clone();
    let project_type = project_type_key.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let _guard = transaction_lock.lock().map_err(|_| creation_error())?;
        create_project_transaction(
            &app_data_directory,
            &owner,
            &selection,
            &name,
            &description,
            &project_type,
        )
    })
    .await
    .map_err(|_| "项目创建任务异常退出。".to_owned())?;
    manager.finish_save(&selection_id, result.is_ok());
    result
}

pub(crate) fn recover_project_creations(
    app_data_directory: &Path,
    owner_user_id: &str,
) -> Result<(), String> {
    let connector_directory = connector_directory(app_data_directory)?;
    let journal_directory = journal_directory(&connector_directory)?;
    let journals = load_journals(&journal_directory)?;
    for (path, journal) in journals {
        if journal.owner_user_id != owner_user_id {
            continue;
        }
        validate_journal(&journal)?;
        let database = connector_directory.join("Projects.sqlite3");
        let mut connection = open_project_registry(&database)?;
        ensure_journal_project(&mut connection, &journal)?;
        ensure_workspace_grant(app_data_directory, owner_user_id, &journal.workspace)?;
        let _ = remove_journal(&path);
    }
    Ok(())
}

fn create_project_transaction(
    app_data_directory: &Path,
    owner_user_id: &str,
    selection: &PendingSelection,
    name: &str,
    description: &str,
    project_type_key: &str,
) -> Result<CreatedLocalProject, String> {
    if selection.owner_user_id != owner_user_id {
        return Err(selection_expired());
    }
    let (canonical_root, _) = prepare_selected_directory(&selection.canonical_root)?;
    if canonical_root != selection.canonical_root {
        return Err("所选项目目录在保存前发生了变化，请重新选择。".into());
    }
    recover_project_creations(app_data_directory, owner_user_id)?;
    let connector_directory = connector_directory(app_data_directory)?;
    let database = connector_directory.join("Projects.sqlite3");
    let mut connection = open_project_registry(&database)?;
    let state = connector_state::read_owner_state(app_data_directory, owner_user_id)?;
    let workspace = select_workspace_grant(&state, owner_user_id, &canonical_root)?;
    reject_duplicate_project(
        &connection,
        &state,
        owner_user_id,
        &workspace,
        &canonical_root,
    )?;

    let created_at_unix_ms = unix_milliseconds()?;
    let project = JournalProject {
        id: uuid::Uuid::new_v4().to_string().to_lowercase(),
        name: name.to_owned(),
        description: description.to_owned(),
        project_type_key: project_type_key.to_owned(),
        created_at_unix_ms,
    };
    let journal = CreationJournal {
        schema_version: JOURNAL_SCHEMA_VERSION,
        owner_user_id: owner_user_id.to_owned(),
        workspace,
        project,
    };
    validate_journal(&journal)?;
    let journal_path = persist_journal(&connector_directory, &journal)?;
    if let Err(error) = insert_journal_project(&mut connection, &journal) {
        let _ = remove_journal(&journal_path);
        return Err(error);
    }
    if let Err(error) =
        ensure_workspace_grant(app_data_directory, owner_user_id, &journal.workspace)
    {
        return Err(format!(
            "项目已安全写入恢复队列，但文件夹授权尚未完成：{error}"
        ));
    }
    // Both authoritative stores are committed. A transient cleanup failure must not turn a
    // successful project into an apparent failure; recovery is idempotent and retries it.
    let _ = remove_journal(&journal_path);
    Ok(CreatedLocalProject {
        id: journal.project.id,
        name: journal.project.name,
        revision: 1,
        project_type_key: journal.project.project_type_key,
    })
}

fn prepare_selected_directory(path: &Path) -> Result<(PathBuf, String), String> {
    if !path.is_absolute() {
        return Err(invalid_directory());
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| invalid_directory())?;
    if !metadata.is_dir() || is_link_or_reparse_point(&metadata) {
        return Err("项目根目录不能是文件、符号链接或 Windows 重解析点。".into());
    }
    let canonical = fs::canonicalize(path).map_err(|_| invalid_directory())?;
    let canonical_text = canonical.to_str().ok_or_else(invalid_directory)?;
    if canonical_text.is_empty()
        || canonical_text.len() > 16_384
        || canonical_text.chars().any(char::is_control)
    {
        return Err(invalid_directory());
    }
    let canonical_metadata = fs::metadata(&canonical).map_err(|_| invalid_directory())?;
    if !canonical_metadata.is_dir() {
        return Err(invalid_directory());
    }
    let directory_name = canonical
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty() && !value.chars().any(char::is_control))
        .map(str::to_owned)
        .unwrap_or_else(|| "所选磁盘根目录".into());
    Ok((canonical, directory_name))
}

#[cfg(windows)]
fn is_link_or_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_type().is_symlink() || metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_link_or_reparse_point(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn select_workspace_grant(
    state: &Map<String, Value>,
    owner_user_id: &str,
    canonical_root: &Path,
) -> Result<WorkspaceGrant, String> {
    let root_key = path_key(canonical_root)?;
    for workspace in workspace_grants(state)? {
        let existing = PathBuf::from(&workspace.absolute_root);
        if existing.is_absolute()
            && fs::canonicalize(&existing)
                .ok()
                .and_then(|path| path_key(&path).ok())
                .as_deref()
                == Some(root_key.as_str())
        {
            return Ok(workspace);
        }
    }
    let digest = sha256(&format!(
        "chatos.windows.workspace.id.v1\0{owner_user_id}\0{root_key}"
    ));
    let id = format!("windows-folder-{}", &digest[..32]);
    let fingerprint = sha256(&format!(
        "chatos.windows.workspace.fingerprint.v1\0{owner_user_id}\0{root_key}"
    ));
    Ok(WorkspaceGrant {
        id,
        absolute_root: canonical_root
            .to_str()
            .ok_or_else(invalid_directory)?
            .to_owned(),
        fingerprint,
    })
}

fn reject_duplicate_project(
    connection: &Connection,
    state: &Map<String, Value>,
    owner_user_id: &str,
    selected_workspace: &WorkspaceGrant,
    canonical_root: &Path,
) -> Result<(), String> {
    let selected_key = path_key(canonical_root)?;
    let grants = workspace_grants(state)?;
    let mut statement = connection
        .prepare(
            "SELECT workspace_id, relative_root FROM local_project_records \
             WHERE owner_user_id = ?1 AND status IN ('active', 'archived')",
        )
        .map_err(|_| creation_error())?;
    let bindings = statement
        .query_map(params![owner_user_id], |row| {
            Ok(RegistryBinding {
                workspace_id: row.get(0)?,
                relative_root: row.get(1)?,
            })
        })
        .map_err(|_| creation_error())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| creation_error())?;
    for binding in bindings {
        if binding.workspace_id == selected_workspace.id && binding.relative_root.is_empty() {
            return Err(duplicate_project());
        }
        if resolved_binding_key(&binding, &grants).as_deref() == Some(selected_key.as_str()) {
            return Err(duplicate_project());
        }
    }
    Ok(())
}

fn resolved_binding_key(binding: &RegistryBinding, grants: &[WorkspaceGrant]) -> Option<String> {
    if !valid_relative_root(&binding.relative_root) {
        return None;
    }
    let workspace = grants
        .iter()
        .find(|workspace| workspace.id == binding.workspace_id)?;
    let root = fs::canonicalize(&workspace.absolute_root).ok()?;
    let candidate = if binding.relative_root.is_empty() {
        root.clone()
    } else {
        root.join(&binding.relative_root)
    };
    let resolved = fs::canonicalize(candidate).ok()?;
    if !resolved.starts_with(root) || !resolved.metadata().ok()?.is_dir() {
        return None;
    }
    path_key(&resolved).ok()
}

fn insert_journal_project(
    connection: &mut Connection,
    journal: &CreationJournal,
) -> Result<(), String> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| creation_error())?;
    transaction
        .execute(
            "INSERT INTO local_project_records (owner_user_id, id, name, description, \
             workspace_id, relative_root, project_type_key, revision, status, \
             created_at_unix_ms, updated_at_unix_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5, '', ?6, 1, 'active', ?7, ?7)",
            params![
                journal.owner_user_id,
                journal.project.id,
                journal.project.name,
                journal.project.description,
                journal.workspace.id,
                journal.project.project_type_key,
                journal.project.created_at_unix_ms,
            ],
        )
        .map_err(|_| creation_error())?;
    transaction.commit().map_err(|_| creation_error())
}

fn ensure_journal_project(
    connection: &mut Connection,
    journal: &CreationJournal,
) -> Result<(), String> {
    let existing = connection
        .query_row(
            "SELECT name, description, workspace_id, relative_root, project_type_key, \
             revision, status, created_at_unix_ms, updated_at_unix_ms \
             FROM local_project_records WHERE owner_user_id = ?1 AND id = ?2",
            params![journal.owner_user_id, journal.project.id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                ))
            },
        )
        .optional()
        .map_err(|_| creation_error())?;
    if let Some(existing) = existing {
        let expected = (
            journal.project.name.clone(),
            journal.project.description.clone(),
            journal.workspace.id.clone(),
            String::new(),
            journal.project.project_type_key.clone(),
            1,
            "active".to_owned(),
            journal.project.created_at_unix_ms,
            journal.project.created_at_unix_ms,
        );
        return if existing == expected {
            Ok(())
        } else {
            Err("项目创建恢复记录与本地注册表冲突。".into())
        };
    }
    insert_journal_project(connection, journal)
}

fn ensure_workspace_grant(
    app_data_directory: &Path,
    owner_user_id: &str,
    grant: &WorkspaceGrant,
) -> Result<(), String> {
    connector_state::update_owner_state(app_data_directory, owner_user_id, |state| {
        let values = state
            .entry("workspaces")
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or_else(creation_error)?;
        let mut matched = false;
        let grant_key = path_key(Path::new(&grant.absolute_root))?;
        for value in values.iter() {
            let existing: WorkspaceGrant =
                serde_json::from_value(value.clone()).map_err(|_| creation_error())?;
            if existing.id == grant.id {
                if existing != *grant {
                    return Err("文件夹授权标识发生冲突。".into());
                }
                matched = true;
            } else if path_key(Path::new(&existing.absolute_root)).ok().as_deref()
                == Some(grant_key.as_str())
            {
                return Err("文件夹授权路径发生冲突。".into());
            }
        }
        if !matched {
            if values.len() >= 1_024 {
                return Err("本机文件夹授权数量已达上限。".into());
            }
            values.push(json!(grant));
        }
        Ok(())
    })
}

fn workspace_grants(state: &Map<String, Value>) -> Result<Vec<WorkspaceGrant>, String> {
    state
        .get("workspaces")
        .and_then(Value::as_array)
        .ok_or_else(creation_error)?
        .iter()
        .map(|value| serde_json::from_value(value.clone()).map_err(|_| creation_error()))
        .collect()
}

fn persist_journal(
    connector_directory: &Path,
    journal: &CreationJournal,
) -> Result<PathBuf, String> {
    let directory = journal_directory(connector_directory)?;
    let path = directory.join(format!("{}.json", journal.project.id));
    if path.exists() {
        return Err(creation_error());
    }
    let mut bytes = serde_json::to_vec_pretty(journal).map_err(|_| creation_error())?;
    bytes.push(b'\n');
    if bytes.len() as u64 > MAXIMUM_JOURNAL_BYTES {
        return Err(creation_error());
    }
    create_owned_file(&directory, &path, ".project-creation-", &bytes)
        .map_err(|_| creation_error())?;
    Ok(path)
}

fn load_journals(directory: &Path) -> Result<Vec<(PathBuf, CreationJournal)>, String> {
    let mut paths = fs::read_dir(directory)
        .map_err(|_| creation_error())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    paths.sort();
    if paths.len() > MAXIMUM_JOURNALS {
        return Err("项目创建恢复记录过多。".into());
    }
    paths
        .into_iter()
        .map(|path| {
            let metadata = fs::symlink_metadata(&path).map_err(|_| creation_error())?;
            if !metadata.is_file()
                || is_link_or_reparse_point(&metadata)
                || metadata.len() > MAXIMUM_JOURNAL_BYTES
            {
                return Err(creation_error());
            }
            let mut bytes = Vec::with_capacity(metadata.len() as usize);
            File::open(&path)
                .and_then(|file| file.take(MAXIMUM_JOURNAL_BYTES + 1).read_to_end(&mut bytes))
                .map_err(|_| creation_error())?;
            if bytes.len() as u64 > MAXIMUM_JOURNAL_BYTES {
                return Err(creation_error());
            }
            let journal = serde_json::from_slice(&bytes).map_err(|_| creation_error())?;
            Ok((path, journal))
        })
        .collect()
}

fn remove_journal(path: &Path) -> Result<(), String> {
    fs::remove_file(path).map_err(|_| creation_error())
}

fn connector_directory(app_data_directory: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(app_data_directory).map_err(|_| creation_error())?;
    let app_data = fs::canonicalize(app_data_directory).map_err(|_| creation_error())?;
    let connector = app_data.join("NativeConnector");
    fs::create_dir_all(&connector).map_err(|_| creation_error())?;
    let connector = fs::canonicalize(connector).map_err(|_| creation_error())?;
    if !connector.starts_with(app_data) {
        return Err(creation_error());
    }
    Ok(connector)
}

fn journal_directory(connector_directory: &Path) -> Result<PathBuf, String> {
    let canonical_connector =
        fs::canonicalize(connector_directory).map_err(|_| creation_error())?;
    let requested = canonical_connector.join("ProjectCreationJournal");
    fs::create_dir_all(&requested).map_err(|_| creation_error())?;
    let canonical = fs::canonicalize(requested).map_err(|_| creation_error())?;
    if !canonical.starts_with(canonical_connector) {
        return Err(creation_error());
    }
    Ok(canonical)
}

fn validate_journal(journal: &CreationJournal) -> Result<(), String> {
    if journal.schema_version != JOURNAL_SCHEMA_VERSION {
        return Err(creation_error());
    }
    validate_owner(&journal.owner_user_id)?;
    validate_route_identifier(&journal.workspace.id, "workspaceId")?;
    validate_route_identifier(&journal.project.id, "projectId")?;
    if validate_project_name(journal.project.name.clone())? != journal.project.name
        || validate_project_description(journal.project.description.clone())?
            != journal.project.description
    {
        return Err(creation_error());
    }
    validate_project_type(&journal.project.project_type_key)?;
    if journal.project.created_at_unix_ms < 0
        || journal.workspace.fingerprint.len() != 64
        || !journal
            .workspace
            .fingerprint
            .bytes()
            .all(|value| value.is_ascii_hexdigit())
    {
        return Err(creation_error());
    }
    let root = Path::new(&journal.workspace.absolute_root);
    if !root.is_absolute()
        || journal.workspace.absolute_root.is_empty()
        || journal.workspace.absolute_root.len() > 16_384
        || journal
            .workspace
            .absolute_root
            .chars()
            .any(char::is_control)
    {
        return Err(creation_error());
    }
    Ok(())
}

fn validate_project_name(value: String) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 1_024 || value.chars().any(char::is_control) {
        Err("请输入有效的项目名称。".into())
    } else {
        Ok(value.to_owned())
    }
}

fn validate_project_description(value: String) -> Result<String, String> {
    let value = value.trim();
    if value.len() > 16_384 || value.contains('\0') {
        Err("项目说明无效。".into())
    } else {
        Ok(value.to_owned())
    }
}

fn validate_project_type(value: &str) -> Result<(), String> {
    if PROJECT_TYPES
        .iter()
        .any(|project_type| project_type.key == value)
    {
        Ok(())
    } else {
        Err("项目类型无效。".into())
    }
}

fn validate_route_identifier(value: &str, field: &str) -> Result<(), String> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > 1_024
        || value.chars().any(char::is_control)
        || value.contains('/')
        || value.contains('\\')
        || value == "."
        || value == ".."
    {
        Err(format!("{field} 无效。"))
    } else {
        Ok(())
    }
}

fn valid_relative_root(value: &str) -> bool {
    value.trim() == value
        && !value.contains('\\')
        && !value.contains(':')
        && !value.chars().any(char::is_control)
        && (value.is_empty()
            || value
                .split('/')
                .all(|component| !component.is_empty() && component != "." && component != ".."))
}

fn path_key(path: &Path) -> Result<String, String> {
    let value = path.to_str().ok_or_else(invalid_directory)?;
    #[cfg(windows)]
    let value = value.to_lowercase();
    #[cfg(not(windows))]
    let value = value.to_owned();
    Ok(value)
}

fn unix_milliseconds() -> Result<i64, String> {
    let value = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| creation_error())?
        .as_millis();
    i64::try_from(value).map_err(|_| creation_error())
}

fn sha256(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn selection_expired() -> String {
    "文件夹授权已过期或属于其他创建窗口，请重新选择。".into()
}

fn invalid_directory() -> String {
    "请选择可访问的本机文件夹作为项目根目录。".into()
}

fn duplicate_project() -> String {
    "该文件夹已经注册为当前账号的项目。".into()
}

fn creation_error() -> String {
    "无法安全创建项目。".into()
}

const PROJECT_TYPES: &[ProjectTypeDefinition] = &[
    ProjectTypeDefinition {
        key: LEGACY_PROJECT_TYPE,
        label: "通用软件开发",
        description: "无法进一步归入明确形态的软件产品、服务或工程项目。",
        category_key: "software_product",
        category_label: "软件与数字产品",
    },
    ProjectTypeDefinition {
        key: "web_application",
        label: "Web 应用与网站",
        description: "面向浏览器的业务应用、门户、管理后台、官网和全栈 Web 产品。",
        category_key: "software_product",
        category_label: "软件与数字产品",
    },
    ProjectTypeDefinition {
        key: "mobile_application",
        label: "移动端应用",
        description: "iOS、Android、Flutter、React Native、PDA 和移动设备应用。",
        category_key: "software_product",
        category_label: "软件与数字产品",
    },
    ProjectTypeDefinition {
        key: "desktop_application",
        label: "桌面应用",
        description: "Windows、macOS、Linux 桌面软件、客户端和跨平台桌面产品。",
        category_key: "software_product",
        category_label: "软件与数字产品",
    },
    ProjectTypeDefinition {
        key: "backend_service",
        label: "后端服务与 API",
        description: "微服务、单体后端、开放 API、网关和集成服务。",
        category_key: "software_product",
        category_label: "软件与数字产品",
    },
    ProjectTypeDefinition {
        key: "library_sdk",
        label: "库、SDK 与开发工具",
        description: "公共库、SDK、CLI、编译工具、插件和开发者基础设施。",
        category_key: "software_product",
        category_label: "软件与数字产品",
    },
    ProjectTypeDefinition {
        key: "game_development",
        label: "游戏开发",
        description: "2D/3D 游戏、互动体验、关卡、游戏服务和内容工具链。",
        category_key: "software_product",
        category_label: "软件与数字产品",
    },
    ProjectTypeDefinition {
        key: "iot_embedded_system",
        label: "IoT 与嵌入式系统",
        description: "设备固件、边缘计算、工业协议、硬件控制和设备云。",
        category_key: "software_product",
        category_label: "软件与数字产品",
    },
    ProjectTypeDefinition {
        key: "enterprise_erp",
        label: "ERP 企业资源计划",
        description: "覆盖财务、采购、销售、库存、制造、人力或项目核算的企业系统。",
        category_key: "enterprise_system",
        category_label: "企业业务系统",
    },
    ProjectTypeDefinition {
        key: "warehouse_management_system",
        label: "WMS 仓储管理系统",
        description: "覆盖入库、上架、库存、补货、波次、拣选、复核、出库和盘点的仓储系统。",
        category_key: "enterprise_system",
        category_label: "企业业务系统",
    },
    ProjectTypeDefinition {
        key: "customer_relationship_management",
        label: "CRM 客户关系管理",
        description: "覆盖线索、客户、联系人、商机、报价、活动和客户成功的业务系统。",
        category_key: "enterprise_system",
        category_label: "企业业务系统",
    },
    ProjectTypeDefinition {
        key: "manufacturing_execution_system",
        label: "MES 制造执行系统",
        description: "覆盖工单、BOM、工艺、在制品、质量、追溯、设备和生产绩效的制造系统。",
        category_key: "enterprise_system",
        category_label: "企业业务系统",
    },
    ProjectTypeDefinition {
        key: "ecommerce_platform",
        label: "电商与交易平台",
        description: "商城、交易中台、订单、支付、促销、履约、售后和多渠道零售系统。",
        category_key: "enterprise_system",
        category_label: "企业业务系统",
    },
    ProjectTypeDefinition {
        key: "data_analysis",
        label: "数据分析",
        description: "数据清洗、探索分析、指标体系、报表、可视化和决策结论。",
        category_key: "data_research",
        category_label: "数据、AI 与研究",
    },
    ProjectTypeDefinition {
        key: "data_engineering_platform",
        label: "数据工程与平台",
        description: "数据仓库、湖仓、ETL/ELT、流处理、调度、数据质量和数据服务。",
        category_key: "data_research",
        category_label: "数据、AI 与研究",
    },
    ProjectTypeDefinition {
        key: "machine_learning_system",
        label: "机器学习与模型系统",
        description: "训练、评估、推理、推荐、搜索、预测和 MLOps 系统。",
        category_key: "data_research",
        category_label: "数据、AI 与研究",
    },
    ProjectTypeDefinition {
        key: "research",
        label: "研究调研",
        description: "资料调研、竞品分析、学术研究、行业研究和可行性论证。",
        category_key: "data_research",
        category_label: "数据、AI 与研究",
    },
    ProjectTypeDefinition {
        key: "product_design",
        label: "产品与体验设计",
        description: "产品方案、用户旅程、信息架构、交互、视觉和服务设计。",
        category_key: "design_content",
        category_label: "设计、内容与知识",
    },
    ProjectTypeDefinition {
        key: "design_system_brand",
        label: "设计系统与品牌",
        description: "设计 Token、组件库、品牌识别、视觉规范和多端一致性建设。",
        category_key: "design_content",
        category_label: "设计、内容与知识",
    },
    ProjectTypeDefinition {
        key: "novel_writing",
        label: "小说创作",
        description: "长篇、短篇、连载小说和其他叙事作品。",
        category_key: "design_content",
        category_label: "设计、内容与知识",
    },
    ProjectTypeDefinition {
        key: "general_writing",
        label: "专业写作",
        description: "文章、报告、白皮书、演讲稿、脚本和非虚构内容。",
        category_key: "design_content",
        category_label: "设计、内容与知识",
    },
    ProjectTypeDefinition {
        key: "documentation",
        label: "文档与知识库",
        description: "产品文档、API 文档、操作手册、教程、规范和知识库。",
        category_key: "design_content",
        category_label: "设计、内容与知识",
    },
    ProjectTypeDefinition {
        key: "marketing_content",
        label: "市场、品牌与增长内容",
        description: "营销活动、品牌传播、社媒、增长实验和销售内容。",
        category_key: "design_content",
        category_label: "设计、内容与知识",
    },
    ProjectTypeDefinition {
        key: "automation",
        label: "自动化、Agent 与集成",
        description: "工作流、脚本、Agent、MCP、机器人和重复任务自动化。",
        category_key: "operations_delivery",
        category_label: "自动化、运营与交付",
    },
    ProjectTypeDefinition {
        key: "operations",
        label: "运营与持续服务",
        description: "产品运营、平台运营、内容运营、客户运营和持续服务。",
        category_key: "operations_delivery",
        category_label: "自动化、运营与交付",
    },
    ProjectTypeDefinition {
        key: "implementation_migration",
        label: "实施、迁移与上线",
        description: "企业软件实施、数据迁移、系统切换、培训、UAT 和上线交接。",
        category_key: "operations_delivery",
        category_label: "自动化、运营与交付",
    },
    ProjectTypeDefinition {
        key: "general",
        label: "通用项目",
        description: "尚不属于其他明确类别的协作项目。",
        category_key: "general",
        category_label: "通用项目",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(root: &Path) -> PendingSelection {
        PendingSelection {
            owner_user_id: "alice".into(),
            presentation_id: "presentation-1".into(),
            canonical_root: fs::canonicalize(root).unwrap(),
            created_at: Instant::now(),
            saving: false,
        }
    }

    #[test]
    fn catalog_matches_the_macos_project_type_keys() {
        assert_eq!(PROJECT_TYPES.len(), 27);
        assert_eq!(PROJECT_TYPES[0].key, LEGACY_PROJECT_TYPE);
        let keys = PROJECT_TYPES
            .iter()
            .map(|value| value.key)
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(keys.len(), PROJECT_TYPES.len());
        assert!(validate_project_type("desktop_application").is_ok());
        assert!(validate_project_type("invented").is_err());

        let macos_catalog: Value = serde_json::from_str(include_str!(
            "../../../macos/Sources/ChatOSCore/AgentSkills/Catalog/ChatOSSkillCatalog.json"
        ))
        .unwrap();
        let macos_types = macos_catalog["projectTypes"].as_array().unwrap();
        let windows_projection = PROJECT_TYPES
            .iter()
            .map(|value| {
                (
                    value.key,
                    value.label,
                    value.description,
                    value.category_key,
                    value.category_label,
                )
            })
            .collect::<Vec<_>>();
        let macos_projection = macos_types
            .iter()
            .map(|value| {
                (
                    value["key"].as_str().unwrap(),
                    value["label"].as_str().unwrap(),
                    value["description"].as_str().unwrap(),
                    value["category_key"].as_str().unwrap(),
                    value["category_label"].as_str().unwrap(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(windows_projection, macos_projection);
    }

    #[test]
    fn selection_tokens_are_owner_and_presentation_scoped() {
        let directory = tempfile::tempdir().unwrap();
        let manager = ProjectCreationManager::default();
        let selected = manager
            .stage("alice", "sheet-1", directory.path().into(), "repo".into())
            .unwrap();
        assert!(manager
            .begin_save("bob", "sheet-1", &selected.selection_id)
            .is_err());
        assert!(manager
            .begin_save("alice", "sheet-2", &selected.selection_id)
            .is_err());
        let accepted = manager
            .begin_save("alice", "sheet-1", &selected.selection_id)
            .unwrap();
        assert_eq!(accepted.canonical_root, directory.path());
        assert!(manager
            .begin_save("alice", "sheet-1", &selected.selection_id)
            .is_err());
        manager.finish_save(&selected.selection_id, false);
        assert!(manager
            .begin_save("alice", "sheet-1", &selected.selection_id)
            .is_ok());
    }

    #[test]
    fn a_closed_sheet_rejects_a_late_native_picker_result() {
        let directory = tempfile::tempdir().unwrap();
        let manager = ProjectCreationManager::default();
        manager.discard("alice", "sheet-closed");
        assert!(manager
            .stage(
                "alice",
                "sheet-closed",
                directory.path().into(),
                "repo".into(),
            )
            .is_err());
    }

    #[test]
    fn project_commit_persists_registry_and_grant_without_exposing_path() {
        let app_data = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        connector_state::ensure_owner_state(app_data.path(), "alice").unwrap();
        let created = create_project_transaction(
            app_data.path(),
            "alice",
            &pending(root.path()),
            "Local",
            "Description",
            LEGACY_PROJECT_TYPE,
        )
        .unwrap();
        assert_eq!(created.name, "Local");
        let state = connector_state::read_owner_state(app_data.path(), "alice").unwrap();
        let grants = workspace_grants(&state).unwrap();
        assert_eq!(grants.len(), 1);
        assert_eq!(
            Path::new(&grants[0].absolute_root),
            fs::canonicalize(root.path()).unwrap()
        );
        let database = app_data.path().join("NativeConnector/Projects.sqlite3");
        let connection = open_project_registry(&database).unwrap();
        let row = connection
            .query_row(
                "SELECT name, workspace_id, relative_root, status FROM local_project_records \
                 WHERE owner_user_id = 'alice' AND id = ?1",
                params![created.id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(row.0, "Local");
        assert_eq!(row.1, grants[0].id);
        assert_eq!(row.2, "");
        assert_eq!(row.3, "active");
        assert!(load_journals(
            &journal_directory(&connector_directory(app_data.path()).unwrap()).unwrap()
        )
        .unwrap()
        .is_empty());
    }

    #[test]
    fn active_and_archived_projects_block_duplicate_roots_but_removed_projects_do_not() {
        for status in ["active", "archived", "removed"] {
            let app_data = tempfile::tempdir().unwrap();
            let root = tempfile::tempdir().unwrap();
            connector_state::ensure_owner_state(app_data.path(), "alice").unwrap();
            let first = create_project_transaction(
                app_data.path(),
                "alice",
                &pending(root.path()),
                "First",
                "",
                LEGACY_PROJECT_TYPE,
            )
            .unwrap();
            let database = app_data.path().join("NativeConnector/Projects.sqlite3");
            let connection = open_project_registry(&database).unwrap();
            connection.execute(
                "UPDATE local_project_records SET status = ?1 WHERE owner_user_id = 'alice' AND id = ?2",
                params![status, first.id],
            ).unwrap();
            let second = create_project_transaction(
                app_data.path(),
                "alice",
                &pending(root.path()),
                "Second",
                "",
                LEGACY_PROJECT_TYPE,
            );
            assert_eq!(second.is_ok(), status == "removed", "status {status}");
        }
    }

    #[test]
    fn journal_recovery_completes_a_registry_only_commit_even_if_root_is_offline() {
        let app_data = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        connector_state::ensure_owner_state(app_data.path(), "alice").unwrap();
        let state = connector_state::read_owner_state(app_data.path(), "alice").unwrap();
        let workspace =
            select_workspace_grant(&state, "alice", &fs::canonicalize(root.path()).unwrap())
                .unwrap();
        let journal = CreationJournal {
            schema_version: JOURNAL_SCHEMA_VERSION,
            owner_user_id: "alice".into(),
            workspace: workspace.clone(),
            project: JournalProject {
                id: "recovered-project".into(),
                name: "Recovered".into(),
                description: "".into(),
                project_type_key: LEGACY_PROJECT_TYPE.into(),
                created_at_unix_ms: 1,
            },
        };
        let connector = connector_directory(app_data.path()).unwrap();
        let journal_path = persist_journal(&connector, &journal).unwrap();
        let mut connection = open_project_registry(&connector.join("Projects.sqlite3")).unwrap();
        insert_journal_project(&mut connection, &journal).unwrap();
        drop(root);
        recover_project_creations(app_data.path(), "alice").unwrap();
        recover_project_creations(app_data.path(), "alice").unwrap();
        assert!(!journal_path.exists());
        let restored = connector_state::read_owner_state(app_data.path(), "alice").unwrap();
        assert_eq!(workspace_grants(&restored).unwrap(), vec![workspace]);
        let count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM local_project_records WHERE owner_user_id = 'alice' AND id = 'recovered-project'",
            [], |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 1);
    }

    #[cfg(unix)]
    #[test]
    fn direct_symbolic_link_selection_is_rejected() {
        use std::os::unix::fs::symlink;
        let parent = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let link = parent.path().join("linked");
        symlink(root.path(), &link).unwrap();
        assert!(prepare_selected_directory(&link).is_err());
    }
}
