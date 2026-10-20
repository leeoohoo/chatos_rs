use crate::{
    authentication::AuthenticationManager,
    host_lifecycle::validate_owner,
    project_creation::{recover_project_creations, ProjectCreationManager},
    project_run::{ProjectRunManager, ProjectRunPreferencesStore},
    workspace_index::open_project_registry,
};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager, State};

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectRemovalReceipt {
    project_id: String,
    removed_revision: i64,
    registry_revision: i64,
    directory_preserved: bool,
    preferences_cleaned: bool,
}

#[tauri::command]
pub(crate) async fn remove_local_project(
    app: AppHandle,
    authentication: State<'_, AuthenticationManager>,
    creation_manager: State<'_, ProjectCreationManager>,
    project_runs: State<'_, ProjectRunManager>,
    preferences: State<'_, ProjectRunPreferencesStore>,
    owner_user_id: String,
    project_id: String,
) -> Result<ProjectRemovalReceipt, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&project_id)?;
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    let data_directory = app.path().app_data_dir().map_err(|_| removal_error())?;
    let transaction_lock = creation_manager.transaction_lock();
    let preferences = preferences.inner().clone();
    let owner = owner_user_id.clone();
    let project = project_id.clone();
    let (removed_revision, registry_revision) = tauri::async_runtime::spawn_blocking(move || {
        let _guard = transaction_lock.lock().map_err(|_| removal_error())?;
        recover_project_creations(&data_directory, &owner)?;
        let database = data_directory
            .join("NativeConnector")
            .join("Projects.sqlite3");
        remove_registry_record(&database, &owner, &project)
    })
    .await
    .map_err(|_| "项目移除任务异常退出。".to_owned())??;
    project_runs
        .release_project(&owner_user_id, &project_id)
        .await;
    let cleanup_owner = owner_user_id.clone();
    let cleanup_project = project_id.clone();
    let preferences_cleaned = tauri::async_runtime::spawn_blocking(move || {
        preferences
            .remove_project(&cleanup_owner, &cleanup_project)
            .is_ok()
    })
    .await
    .unwrap_or(false);
    Ok(ProjectRemovalReceipt {
        project_id,
        removed_revision,
        registry_revision,
        directory_preserved: true,
        preferences_cleaned,
    })
}

fn remove_registry_record(
    database_path: &Path,
    owner_user_id: &str,
    project_id: &str,
) -> Result<(i64, i64), String> {
    let mut connection = open_project_registry(database_path)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| removal_error())?;
    let record = transaction
        .query_row(
            "SELECT revision, status, updated_at_unix_ms FROM local_project_records \
             WHERE owner_user_id = ?1 AND id = ?2",
            params![owner_user_id, project_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|_| removal_error())?
        .filter(|(_, status, _)| status != "removed")
        .ok_or_else(project_unavailable)?;
    let registry_revision = record.0.checked_add(1).ok_or_else(removal_error)?;
    let updated_at = now_unix_ms().max(record.2.saturating_add(1));
    let updated = transaction
        .execute(
            "UPDATE local_project_records SET status = 'removed', revision = ?1, \
             updated_at_unix_ms = ?2 WHERE owner_user_id = ?3 AND id = ?4 \
             AND revision = ?5 AND status != 'removed'",
            params![
                registry_revision,
                updated_at,
                owner_user_id,
                project_id,
                record.0
            ],
        )
        .map_err(|_| removal_error())?;
    if updated != 1 {
        return Err("项目已被其他操作更新，请刷新后重试。".into());
    }
    transaction.commit().map_err(|_| removal_error())?;
    Ok((record.0, registry_revision))
}

fn validate_identifier(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > 1_024
        || value.chars().any(char::is_control)
    {
        Err("项目移除请求包含无效字段。".into())
    } else {
        Ok(())
    }
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

fn project_unavailable() -> String {
    "项目不存在或已经移除。".into()
}

fn removal_error() -> String {
    "无法移除本地项目。".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace_index::PROJECT_REGISTRY_SCHEMA;
    use std::fs;

    #[test]
    fn removal_is_owner_revision_scoped_and_never_deletes_the_directory() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("Projects.sqlite3");
        let project_root = directory.path().join("project-root");
        fs::create_dir(&project_root).unwrap();
        fs::write(project_root.join("keep.txt"), "keep").unwrap();
        let connection = rusqlite::Connection::open(&database).unwrap();
        connection.execute_batch(PROJECT_REGISTRY_SCHEMA).unwrap();
        connection
            .execute(
                "INSERT INTO local_project_records VALUES \
                 ('owner', 'project', 'Project', '', 'workspace', '', \
                  'software_development', 7, 'active', 1, 1)",
                [],
            )
            .unwrap();
        drop(connection);

        assert!(remove_registry_record(&database, "other", "project").is_err());
        assert_eq!(
            remove_registry_record(&database, "owner", "project").unwrap(),
            (7, 8)
        );

        let connection = rusqlite::Connection::open(&database).unwrap();
        let record = connection
            .query_row(
                "SELECT revision, status FROM local_project_records \
                 WHERE owner_user_id = 'owner' AND id = 'project'",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .unwrap();
        assert_eq!(record, (8, "removed".into()));
        assert!(project_root.join("keep.txt").is_file());
        assert!(remove_registry_record(&database, "owner", "project").is_err());
    }
}
