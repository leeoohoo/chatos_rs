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
                git_url, description, status, created_at, updated_at, archived_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                owner_user_id = excluded.owner_user_id,
                owner_username = excluded.owner_username,
                owner_display_name = excluded.owner_display_name,
                name = excluded.name,
                root_path = excluded.root_path,
                git_url = excluded.git_url,
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
