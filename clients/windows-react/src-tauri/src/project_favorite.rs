use crate::{
    authentication::AuthenticationManager,
    host_lifecycle::validate_owner,
    project_creation::{recover_project_creations, ProjectCreationManager},
    workspace_index::open_project_registry,
};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

const CURRENT_VERSION: u32 = 1;
const MAXIMUM_FILE_BYTES: u64 = 2 * 1_024 * 1_024;
const MAXIMUM_FAVORITES: usize = 10_000;

#[derive(Clone)]
pub(crate) struct ProjectFavoriteStore {
    path: PathBuf,
    lock: Arc<Mutex<()>>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectFavoriteSnapshot {
    project_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectFavoriteFile {
    version: u32,
    #[serde(default)]
    favorite_project_ids: BTreeSet<String>,
}

impl Default for ProjectFavoriteFile {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            favorite_project_ids: BTreeSet::new(),
        }
    }
}

impl ProjectFavoriteStore {
    pub(crate) fn new(app_data_directory: PathBuf) -> Self {
        Self {
            path: app_data_directory
                .join("pet")
                .join("project-favorites.json"),
            lock: Arc::new(Mutex::new(())),
        }
    }

    fn snapshot(
        &self,
        active_project_ids: &BTreeSet<String>,
    ) -> Result<ProjectFavoriteSnapshot, String> {
        let _guard = self.lock.lock().map_err(|_| favorite_error())?;
        let file = self.load()?;
        Ok(ProjectFavoriteSnapshot {
            project_ids: file
                .favorite_project_ids
                .intersection(active_project_ids)
                .cloned()
                .collect(),
        })
    }

    fn set(
        &self,
        project_id: &str,
        is_favorite: bool,
        active_project_ids: &BTreeSet<String>,
    ) -> Result<ProjectFavoriteSnapshot, String> {
        validate_identifier(project_id)?;
        if !active_project_ids.contains(project_id) {
            return Err("项目不存在或已经移除。".into());
        }
        let _guard = self.lock.lock().map_err(|_| favorite_error())?;
        let mut file = self.load()?;
        if is_favorite {
            if file.favorite_project_ids.len() >= MAXIMUM_FAVORITES
                && !file.favorite_project_ids.contains(project_id)
            {
                return Err("宠物快捷访问项目数量超过安全上限。".into());
            }
            file.favorite_project_ids.insert(project_id.to_owned());
        } else {
            file.favorite_project_ids.remove(project_id);
        }
        self.save(&file)?;
        Ok(ProjectFavoriteSnapshot {
            project_ids: file
                .favorite_project_ids
                .intersection(active_project_ids)
                .cloned()
                .collect(),
        })
    }

    fn load(&self) -> Result<ProjectFavoriteFile, String> {
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(ProjectFavoriteFile::default())
            }
            Err(_) => return Err(favorite_error()),
        };
        let metadata = file.metadata().map_err(|_| favorite_error())?;
        if !metadata.is_file() || metadata.len() > MAXIMUM_FILE_BYTES {
            return Err(favorite_error());
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take(MAXIMUM_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| favorite_error())?;
        if bytes.len() as u64 > MAXIMUM_FILE_BYTES {
            return Err(favorite_error());
        }
        let parsed: ProjectFavoriteFile =
            serde_json::from_slice(&bytes).map_err(|_| favorite_error())?;
        validate_file(&parsed)?;
        Ok(parsed)
    }

    fn save(&self, file: &ProjectFavoriteFile) -> Result<(), String> {
        validate_file(file)?;
        let bytes = serde_json::to_vec(file).map_err(|_| favorite_error())?;
        if bytes.len() as u64 > MAXIMUM_FILE_BYTES {
            return Err(favorite_error());
        }
        let parent = self.path.parent().ok_or_else(favorite_error)?;
        fs::create_dir_all(parent).map_err(|_| favorite_error())?;
        let temporary = parent.join(format!(".project-favorites-{}.tmp", Uuid::new_v4()));
        let result =
            write_private_file(&temporary, &bytes).and_then(|_| fs::rename(&temporary, &self.path));
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
            return Err(favorite_error());
        }
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok(())
    }
}

#[tauri::command]
pub(crate) async fn load_project_favorites(
    app: AppHandle,
    authentication: State<'_, AuthenticationManager>,
    creation_manager: State<'_, ProjectCreationManager>,
    favorites: State<'_, ProjectFavoriteStore>,
    owner_user_id: String,
) -> Result<ProjectFavoriteSnapshot, String> {
    validate_owner(&owner_user_id)?;
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    let data_directory = app.path().app_data_dir().map_err(|_| favorite_error())?;
    let transaction_lock = creation_manager.transaction_lock();
    let favorites = favorites.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = transaction_lock.lock().map_err(|_| favorite_error())?;
        recover_project_creations(&data_directory, &owner_user_id)?;
        let active = active_project_ids(
            &data_directory
                .join("NativeConnector")
                .join("Projects.sqlite3"),
            &owner_user_id,
        )?;
        favorites.snapshot(&active)
    })
    .await
    .map_err(|_| "宠物快捷访问读取任务异常退出。".to_owned())?
}

#[tauri::command]
pub(crate) async fn set_project_favorite(
    app: AppHandle,
    authentication: State<'_, AuthenticationManager>,
    creation_manager: State<'_, ProjectCreationManager>,
    favorites: State<'_, ProjectFavoriteStore>,
    owner_user_id: String,
    project_id: String,
    is_favorite: bool,
) -> Result<ProjectFavoriteSnapshot, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&project_id)?;
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    let data_directory = app.path().app_data_dir().map_err(|_| favorite_error())?;
    let transaction_lock = creation_manager.transaction_lock();
    let favorites = favorites.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = transaction_lock.lock().map_err(|_| favorite_error())?;
        recover_project_creations(&data_directory, &owner_user_id)?;
        let active = active_project_ids(
            &data_directory
                .join("NativeConnector")
                .join("Projects.sqlite3"),
            &owner_user_id,
        )?;
        favorites.set(&project_id, is_favorite, &active)
    })
    .await
    .map_err(|_| "宠物快捷访问保存任务异常退出。".to_owned())?
}

fn active_project_ids(
    database_path: &Path,
    owner_user_id: &str,
) -> Result<BTreeSet<String>, String> {
    let connection = open_project_registry(database_path)?;
    let mut statement = connection
        .prepare(
            "SELECT id FROM local_project_records \
             WHERE owner_user_id = ?1 AND status = 'active' ORDER BY id",
        )
        .map_err(|_| favorite_error())?;
    let values = statement
        .query_map(params![owner_user_id], |row| row.get::<_, String>(0))
        .map_err(|_| favorite_error())?
        .collect::<Result<BTreeSet<_>, _>>()
        .map_err(|_| favorite_error())?;
    for value in &values {
        validate_identifier(value)?;
    }
    Ok(values)
}

fn validate_file(file: &ProjectFavoriteFile) -> Result<(), String> {
    if file.version != CURRENT_VERSION || file.favorite_project_ids.len() > MAXIMUM_FAVORITES {
        return Err(favorite_error());
    }
    for project_id in &file.favorite_project_ids {
        validate_identifier(project_id)?;
    }
    Ok(())
}

fn validate_identifier(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > 1_024
        || value.chars().any(char::is_control)
    {
        Err("宠物快捷访问请求包含无效字段。".into())
    } else {
        Ok(())
    }
}

fn write_private_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn favorite_error() -> String {
    "无法读取或保存宠物快捷访问偏好。".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace_index::PROJECT_REGISTRY_SCHEMA;

    #[test]
    fn favorites_are_device_wide_but_projected_through_the_current_owner_projects() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("Projects.sqlite3");
        let connection = rusqlite::Connection::open(&database).unwrap();
        connection.execute_batch(PROJECT_REGISTRY_SCHEMA).unwrap();
        for (owner, id, status) in [
            ("alice", "alice-active", "active"),
            ("alice", "alice-removed", "removed"),
            ("bob", "bob-active", "active"),
        ] {
            connection
                .execute(
                    "INSERT INTO local_project_records VALUES \
                     (?1, ?2, 'Project', '', 'workspace', '', \
                      'software_development', 1, ?3, 1, 1)",
                    params![owner, id, status],
                )
                .unwrap();
        }
        drop(connection);
        let store = ProjectFavoriteStore {
            path: directory.path().join("favorites.json"),
            lock: Arc::new(Mutex::new(())),
        };
        let alice = active_project_ids(&database, "alice").unwrap();
        let bob = active_project_ids(&database, "bob").unwrap();

        assert_eq!(
            store.set("alice-active", true, &alice).unwrap().project_ids,
            ["alice-active"]
        );
        assert!(store.snapshot(&bob).unwrap().project_ids.is_empty());
        assert!(store.set("alice-removed", true, &alice).is_err());
        assert_eq!(
            store.snapshot(&alice).unwrap().project_ids,
            ["alice-active"]
        );
    }
}
