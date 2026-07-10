// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

use crate::models::task_project_status_to_str;

impl SqliteStore {
    pub(in crate::store) async fn list_task_projects(
        &self,
    ) -> Result<Vec<TaskProjectRecord>, String> {
        let rows =
            sqlx::query("SELECT * FROM task_projects ORDER BY datetime(updated_at) DESC, id DESC")
                .fetch_all(&self.pool)
                .await
                .map_err(|err| err.to_string())?;
        rows.iter().map(task_project_from_row).collect()
    }

    pub(in crate::store) async fn get_task_project(
        &self,
        id: &str,
    ) -> Result<Option<TaskProjectRecord>, String> {
        let row = sqlx::query("SELECT * FROM task_projects WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        row.as_ref().map(task_project_from_row).transpose()
    }

    pub(in crate::store) async fn save_task_project(
        &self,
        project: TaskProjectRecord,
    ) -> Result<TaskProjectRecord, String> {
        sqlx::query(
            "INSERT INTO task_projects (
                id, owner_user_id, owner_username, owner_display_name, name, root_path,
                git_url, source_type, cloud_import_source, import_status, source_git_url,
                harness_space_identifier, harness_repo_identifier, harness_repo_path,
                harness_git_url, harness_git_ssh_url, harness_default_branch,
                harness_provision_status, harness_provision_error, harness_provisioned_at,
                description, status, created_at, updated_at, archived_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                owner_user_id = excluded.owner_user_id,
                owner_username = excluded.owner_username,
                owner_display_name = excluded.owner_display_name,
                name = excluded.name,
                root_path = excluded.root_path,
                git_url = excluded.git_url,
                source_type = excluded.source_type,
                cloud_import_source = excluded.cloud_import_source,
                import_status = excluded.import_status,
                source_git_url = excluded.source_git_url,
                harness_space_identifier = excluded.harness_space_identifier,
                harness_repo_identifier = excluded.harness_repo_identifier,
                harness_repo_path = excluded.harness_repo_path,
                harness_git_url = excluded.harness_git_url,
                harness_git_ssh_url = excluded.harness_git_ssh_url,
                harness_default_branch = excluded.harness_default_branch,
                harness_provision_status = excluded.harness_provision_status,
                harness_provision_error = excluded.harness_provision_error,
                harness_provisioned_at = excluded.harness_provisioned_at,
                description = excluded.description,
                status = excluded.status,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at,
                archived_at = excluded.archived_at",
        )
        .bind(&project.id)
        .bind(project.owner_user_id.clone())
        .bind(project.owner_username.clone())
        .bind(project.owner_display_name.clone())
        .bind(&project.name)
        .bind(project.root_path.clone())
        .bind(project.git_url.clone())
        .bind(project.source_type.clone())
        .bind(project.cloud_import_source.clone())
        .bind(project.import_status.clone())
        .bind(project.source_git_url.clone())
        .bind(project.harness_space_identifier.clone())
        .bind(project.harness_repo_identifier.clone())
        .bind(project.harness_repo_path.clone())
        .bind(project.harness_git_url.clone())
        .bind(project.harness_git_ssh_url.clone())
        .bind(project.harness_default_branch.clone())
        .bind(project.harness_provision_status.clone())
        .bind(project.harness_provision_error.clone())
        .bind(project.harness_provisioned_at.clone())
        .bind(project.description.clone())
        .bind(task_project_status_to_str(project.status))
        .bind(&project.created_at)
        .bind(&project.updated_at)
        .bind(project.archived_at.clone())
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(project)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::str::FromStr;
    use std::sync::Arc;

    use parking_lot::RwLock;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use tokio::sync::broadcast;
    use uuid::Uuid;

    use super::*;
    use crate::models::{now_rfc3339, TaskProjectStatus};

    struct TestDatabase(PathBuf);

    impl TestDatabase {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!(
                "chatos-task-project-harness-test-{}.db",
                Uuid::new_v4()
            )))
        }

        fn path(&self) -> &Path {
            self.0.as_path()
        }

        fn url(&self) -> String {
            format!("sqlite://{}", self.0.display())
        }
    }

    impl Drop for TestDatabase {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
            let _ = fs::remove_file(format!("{}-shm", self.0.display()));
            let _ = fs::remove_file(format!("{}-wal", self.0.display()));
        }
    }

    #[tokio::test]
    async fn task_project_sqlite_round_trip_preserves_harness_fields_and_local_source() {
        let database = TestDatabase::new();
        let (sender, _) = broadcast::channel(8);
        let connect_options = SqliteConnectOptions::from_str(database.url().as_str())
            .expect("parse SQLite URL")
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(connect_options)
            .await
            .expect("connect SQLite task store");
        sqlx::query(
            "CREATE TABLE task_projects (
                id TEXT PRIMARY KEY,
                owner_user_id TEXT,
                owner_username TEXT,
                owner_display_name TEXT,
                name TEXT NOT NULL,
                root_path TEXT,
                git_url TEXT,
                description TEXT,
                status TEXT NOT NULL DEFAULT 'active',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                archived_at TEXT
            )",
        )
        .execute(&pool)
        .await
        .expect("create baseline task_projects table");
        for statement in [
            "ALTER TABLE task_projects ADD COLUMN source_type TEXT",
            "ALTER TABLE task_projects ADD COLUMN cloud_import_source TEXT",
            "ALTER TABLE task_projects ADD COLUMN import_status TEXT",
            "ALTER TABLE task_projects ADD COLUMN source_git_url TEXT",
            "ALTER TABLE task_projects ADD COLUMN harness_space_identifier TEXT",
            "ALTER TABLE task_projects ADD COLUMN harness_repo_identifier TEXT",
            "ALTER TABLE task_projects ADD COLUMN harness_repo_path TEXT",
            "ALTER TABLE task_projects ADD COLUMN harness_git_url TEXT",
            "ALTER TABLE task_projects ADD COLUMN harness_git_ssh_url TEXT",
            "ALTER TABLE task_projects ADD COLUMN harness_default_branch TEXT",
            "ALTER TABLE task_projects ADD COLUMN harness_provision_status TEXT",
            "ALTER TABLE task_projects ADD COLUMN harness_provision_error TEXT",
            "ALTER TABLE task_projects ADD COLUMN harness_provisioned_at TEXT",
        ] {
            sqlx::query(statement)
                .execute(&pool)
                .await
                .expect("apply Harness project column");
        }
        let store = SqliteStore {
            pool,
            cancel_requested_runs: Arc::new(RwLock::new(HashSet::new())),
            run_event_sender: sender,
        };
        let now = now_rfc3339();
        let project = TaskProjectRecord {
            id: "local-project-1".to_string(),
            owner_user_id: Some("user-1".to_string()),
            owner_username: Some("owner".to_string()),
            owner_display_name: Some("Owner".to_string()),
            name: "Local Project".to_string(),
            root_path: Some("/workspace/local-project".to_string()),
            git_url: Some("https://example.com/user/project.git".to_string()),
            source_type: Some("local".to_string()),
            cloud_import_source: Some("none".to_string()),
            import_status: Some("none".to_string()),
            source_git_url: None,
            harness_space_identifier: Some("users/user-1".to_string()),
            harness_repo_identifier: Some("local-project-1".to_string()),
            harness_repo_path: Some("users/user-1/local-project-1".to_string()),
            harness_git_url: Some(
                "https://harness.example/git/users/user-1/local-project-1.git".to_string(),
            ),
            harness_git_ssh_url: Some(
                "ssh://git@harness.example/users/user-1/local-project-1.git".to_string(),
            ),
            harness_default_branch: Some("main".to_string()),
            harness_provision_status: Some("ready".to_string()),
            harness_provision_error: None,
            harness_provisioned_at: Some(now.clone()),
            description: None,
            status: TaskProjectStatus::Active,
            created_at: now.clone(),
            updated_at: now,
            archived_at: None,
        };
        store
            .save_task_project(project)
            .await
            .expect("save task project");

        let loaded = store
            .get_task_project("local-project-1")
            .await
            .expect("load task project")
            .expect("task project exists");
        assert_eq!(loaded.source_type.as_deref(), Some("local"));
        assert_eq!(
            loaded.root_path.as_deref(),
            Some("/workspace/local-project")
        );
        assert_eq!(
            loaded.git_url.as_deref(),
            Some("https://example.com/user/project.git")
        );
        assert_eq!(
            loaded.harness_git_url.as_deref(),
            Some("https://harness.example/git/users/user-1/local-project-1.git")
        );
        assert_eq!(loaded.harness_default_branch.as_deref(), Some("main"));
        assert_eq!(loaded.harness_provision_status.as_deref(), Some("ready"));

        drop(store);
        assert!(database.path().exists());
    }
}
