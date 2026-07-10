// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use uuid::Uuid;

use super::super::common::normalize_git_url;
use super::super::sqlite_rows::{project_from_row, project_profile_from_row};
use super::SqliteStore;
use crate::auth::CurrentUser;
use crate::models::*;

impl SqliteStore {
    pub async fn list_projects(
        &self,
        user: &CurrentUser,
        status: Option<ProjectStatus>,
    ) -> Result<Vec<ProjectRecord>, String> {
        let mut projects: Vec<ProjectRecord> = if user.is_admin() {
            let rows = sqlx::query(
                "SELECT * FROM projects
                 WHERE (?1 IS NULL OR status = ?1)
                 ORDER BY updated_at DESC",
            )
            .bind(status.map(|status| status.as_str().to_string()))
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
            rows.iter().map(project_from_row).collect()
        } else {
            let owner_user_id = user
                .effective_owner_user_id()
                .ok_or_else(|| "当前登录态缺少用户归属信息".to_string())?;
            let rows = sqlx::query(
                "SELECT * FROM projects
                 WHERE owner_user_id = ?1 AND (?2 IS NULL OR status = ?2)
                 ORDER BY updated_at DESC",
            )
            .bind(owner_user_id)
            .bind(status.map(|status| status.as_str().to_string()))
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
            rows.iter().map(project_from_row).collect()
        };
        projects.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        Ok(projects)
    }

    pub async fn list_all_projects(
        &self,
        status: Option<ProjectStatus>,
    ) -> Result<Vec<ProjectRecord>, String> {
        let rows = sqlx::query(
            "SELECT * FROM projects
             WHERE (?1 IS NULL OR status = ?1)
             ORDER BY updated_at DESC",
        )
        .bind(status.map(|status| status.as_str().to_string()))
        .fetch_all(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(rows.iter().map(project_from_row).collect())
    }

    pub async fn create_project(
        &self,
        input: CreateProjectRequest,
        user: &CurrentUser,
    ) -> Result<ProjectRecord, String> {
        validate_required("name", &input.name)?;
        let owner_user_id = user
            .effective_owner_user_id()
            .map(ToOwned::to_owned)
            .ok_or_else(|| "当前登录态缺少用户归属信息，无法创建项目".to_string())?;
        let now = now_rfc3339();
        let root_path = normalized_optional(input.root_path);
        let git_url = normalize_git_url(input.git_url)?;
        let source_git_url = normalize_git_url(input.source_git_url)?;
        let source_type = input
            .source_type
            .unwrap_or_else(|| project_source_type_from_root(root_path.as_deref()));
        let project = ProjectRecord {
            id: Uuid::new_v4().to_string(),
            creator_user_id: Some(user.id.clone()),
            creator_username: Some(user.username.clone()),
            creator_display_name: Some(user.display_name.clone()),
            owner_user_id: Some(owner_user_id),
            owner_username: user.effective_owner_username().map(ToOwned::to_owned),
            owner_display_name: user
                .effective_owner_display_name()
                .map(ToOwned::to_owned)
                .or_else(|| user.effective_owner_username().map(ToOwned::to_owned)),
            name: input.name.trim().to_string(),
            root_path,
            git_url,
            source_type,
            cloud_import_source: input.cloud_import_source.unwrap_or_default(),
            import_status: input.import_status.unwrap_or_default(),
            source_git_url,
            harness_space_identifier: None,
            harness_repo_identifier: None,
            harness_repo_path: None,
            harness_git_url: None,
            harness_git_ssh_url: None,
            harness_default_branch: None,
            harness_provision_status: Some("pending".to_string()),
            harness_provision_error: None,
            harness_provisioned_at: None,
            import_error: None,
            import_started_at: None,
            import_finished_at: None,
            description: normalized_optional(input.description),
            status: ProjectStatus::Active,
            created_at: now.clone(),
            updated_at: now,
            archived_at: None,
        };
        self.save_project(&project).await?;
        Ok(project)
    }

    pub async fn import_project(
        &self,
        input: ImportProjectRequest,
    ) -> Result<ProjectRecord, String> {
        let id = input.id.trim();
        validate_required("id", id)?;
        validate_required("name", &input.name)?;
        let now = now_rfc3339();
        let status = input.status.unwrap_or(ProjectStatus::Active);
        let root_path = normalized_optional(input.root_path);
        let git_url = normalize_git_url(input.git_url)?;
        let source_git_url = normalize_git_url(input.source_git_url)?;
        let harness_git_url = normalize_git_url(input.harness_git_url)?;
        let source_type = input
            .source_type
            .unwrap_or_else(|| project_source_type_from_root(root_path.as_deref()));
        let project = ProjectRecord {
            id: id.to_string(),
            creator_user_id: None,
            creator_username: None,
            creator_display_name: None,
            owner_user_id: normalized_optional(input.owner_user_id),
            owner_username: normalized_optional(input.owner_username),
            owner_display_name: normalized_optional(input.owner_display_name),
            name: input.name.trim().to_string(),
            root_path,
            git_url,
            source_type,
            cloud_import_source: input.cloud_import_source.unwrap_or_default(),
            import_status: input.import_status.unwrap_or_default(),
            source_git_url,
            harness_space_identifier: normalized_optional(input.harness_space_identifier),
            harness_repo_identifier: normalized_optional(input.harness_repo_identifier),
            harness_repo_path: normalized_optional(input.harness_repo_path),
            harness_git_url,
            harness_git_ssh_url: normalized_optional(input.harness_git_ssh_url),
            harness_default_branch: normalized_optional(input.harness_default_branch),
            harness_provision_status: normalized_optional(input.harness_provision_status),
            harness_provision_error: normalized_optional(input.harness_provision_error),
            harness_provisioned_at: normalized_optional(input.harness_provisioned_at),
            import_error: normalized_optional(input.import_error),
            import_started_at: normalized_optional(input.import_started_at),
            import_finished_at: normalized_optional(input.import_finished_at),
            description: normalized_optional(input.description),
            status,
            created_at: normalized_optional(input.created_at).unwrap_or_else(|| now.clone()),
            updated_at: normalized_optional(input.updated_at).unwrap_or_else(|| now.clone()),
            archived_at: if status == ProjectStatus::Archived {
                normalized_optional(input.archived_at).or_else(|| Some(now))
            } else {
                None
            },
        };
        self.save_project(&project).await?;
        Ok(project)
    }

    pub async fn get_project(&self, id: &str) -> Result<Option<ProjectRecord>, String> {
        let row = sqlx::query("SELECT * FROM projects WHERE id = ?1")
            .bind(id.trim())
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(row.as_ref().map(project_from_row))
    }

    pub async fn update_project(
        &self,
        id: &str,
        patch: UpdateProjectRequest,
    ) -> Result<Option<ProjectRecord>, String> {
        let Some(mut project) = self.get_project(id).await? else {
            return Ok(None);
        };
        if let Some(name) = patch.name {
            validate_required("name", &name)?;
            project.name = name.trim().to_string();
        }
        if patch.root_path.is_some() {
            project.root_path = normalized_optional(patch.root_path);
        }
        if patch.git_url.is_some() {
            project.git_url = normalize_git_url(patch.git_url)?;
        }
        if patch.description.is_some() {
            project.description = normalized_optional(patch.description);
        }
        project.updated_at = now_rfc3339();
        self.save_project(&project).await?;
        Ok(Some(project))
    }

    pub async fn archive_project(&self, id: &str) -> Result<Option<ProjectRecord>, String> {
        let Some(mut project) = self.get_project(id).await? else {
            return Ok(None);
        };
        let now = now_rfc3339();
        project.status = ProjectStatus::Archived;
        project.archived_at = Some(now.clone());
        project.updated_at = now;
        self.save_project(&project).await?;
        Ok(Some(project))
    }

    async fn save_project(&self, project: &ProjectRecord) -> Result<(), String> {
        sqlx::query(
            "INSERT INTO projects (
                id, creator_user_id, creator_username, creator_display_name,
                owner_user_id, owner_username, owner_display_name, name, root_path,
                git_url, source_type, cloud_import_source, import_status, source_git_url,
                harness_space_identifier, harness_repo_identifier, harness_repo_path,
                harness_git_url, harness_git_ssh_url, harness_default_branch,
                harness_provision_status, harness_provision_error, harness_provisioned_at,
                import_error, import_started_at, import_finished_at, description, status,
                created_at, updated_at, archived_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31)
             ON CONFLICT(id) DO UPDATE SET
                creator_user_id = excluded.creator_user_id,
                creator_username = excluded.creator_username,
                creator_display_name = excluded.creator_display_name,
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
                import_error = excluded.import_error,
                import_started_at = excluded.import_started_at,
                import_finished_at = excluded.import_finished_at,
                description = excluded.description,
                status = excluded.status,
                updated_at = excluded.updated_at,
                archived_at = excluded.archived_at",
        )
        .bind(&project.id)
        .bind(&project.creator_user_id)
        .bind(&project.creator_username)
        .bind(&project.creator_display_name)
        .bind(&project.owner_user_id)
        .bind(&project.owner_username)
        .bind(&project.owner_display_name)
        .bind(&project.name)
        .bind(&project.root_path)
        .bind(&project.git_url)
        .bind(project.source_type.as_str())
        .bind(project.cloud_import_source.as_str())
        .bind(project.import_status.as_str())
        .bind(&project.source_git_url)
        .bind(&project.harness_space_identifier)
        .bind(&project.harness_repo_identifier)
        .bind(&project.harness_repo_path)
        .bind(&project.harness_git_url)
        .bind(&project.harness_git_ssh_url)
        .bind(&project.harness_default_branch)
        .bind(&project.harness_provision_status)
        .bind(&project.harness_provision_error)
        .bind(&project.harness_provisioned_at)
        .bind(&project.import_error)
        .bind(&project.import_started_at)
        .bind(&project.import_finished_at)
        .bind(&project.description)
        .bind(project.status.as_str())
        .bind(&project.created_at)
        .bind(&project.updated_at)
        .bind(&project.archived_at)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn save_project_record(&self, project: &ProjectRecord) -> Result<(), String> {
        self.save_project(project).await
    }

    pub async fn get_project_profile(
        &self,
        project_id: &str,
    ) -> Result<Option<ProjectProfileRecord>, String> {
        let row = sqlx::query("SELECT * FROM project_profiles WHERE project_id = ?1")
            .bind(project_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(row.as_ref().map(project_profile_from_row))
    }

    pub async fn upsert_project_profile(
        &self,
        project_id: &str,
        input: UpsertProjectProfileRequest,
        user: &CurrentUser,
    ) -> Result<ProjectProfileRecord, String> {
        let now = now_rfc3339();
        let existing = self.get_project_profile(project_id).await?;
        let profile = ProjectProfileRecord {
            project_id: project_id.to_string(),
            creator_user_id: existing
                .as_ref()
                .and_then(|profile| profile.creator_user_id.clone())
                .or_else(|| Some(user.id.clone())),
            creator_username: existing
                .as_ref()
                .and_then(|profile| profile.creator_username.clone())
                .or_else(|| Some(user.username.clone())),
            creator_display_name: existing
                .as_ref()
                .and_then(|profile| profile.creator_display_name.clone())
                .or_else(|| Some(user.display_name.clone())),
            owner_user_id: existing
                .as_ref()
                .and_then(|profile| profile.owner_user_id.clone())
                .or_else(|| user.effective_owner_user_id().map(ToOwned::to_owned)),
            owner_username: existing
                .as_ref()
                .and_then(|profile| profile.owner_username.clone())
                .or_else(|| user.effective_owner_username().map(ToOwned::to_owned)),
            owner_display_name: existing
                .as_ref()
                .and_then(|profile| profile.owner_display_name.clone())
                .or_else(|| {
                    user.effective_owner_display_name()
                        .map(ToOwned::to_owned)
                        .or_else(|| user.effective_owner_username().map(ToOwned::to_owned))
                }),
            background: normalized_optional(input.background),
            introduction: normalized_optional(input.introduction),
            created_at: existing
                .as_ref()
                .map(|profile| profile.created_at.clone())
                .unwrap_or_else(|| now.clone()),
            updated_at: now,
        };
        sqlx::query(
            "INSERT INTO project_profiles (
                project_id, creator_user_id, creator_username, creator_display_name,
                owner_user_id, owner_username, owner_display_name,
                background, introduction, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(project_id) DO UPDATE SET
                creator_user_id = excluded.creator_user_id,
                creator_username = excluded.creator_username,
                creator_display_name = excluded.creator_display_name,
                owner_user_id = excluded.owner_user_id,
                owner_username = excluded.owner_username,
                owner_display_name = excluded.owner_display_name,
                background = excluded.background,
                introduction = excluded.introduction,
                updated_at = excluded.updated_at",
        )
        .bind(&profile.project_id)
        .bind(&profile.creator_user_id)
        .bind(&profile.creator_username)
        .bind(&profile.creator_display_name)
        .bind(&profile.owner_user_id)
        .bind(&profile.owner_username)
        .bind(&profile.owner_display_name)
        .bind(&profile.background)
        .bind(&profile.introduction)
        .bind(&profile.created_at)
        .bind(&profile.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(profile)
    }
}

fn project_source_type_from_root(root_path: Option<&str>) -> ProjectSourceType {
    if root_path
        .map(str::trim)
        .is_some_and(|value| value.starts_with("local://connector/"))
    {
        ProjectSourceType::LocalConnector
    } else {
        ProjectSourceType::Local
    }
}
