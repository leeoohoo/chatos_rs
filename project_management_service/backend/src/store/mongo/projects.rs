// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::{doc, Document};
use uuid::Uuid;

use super::super::common::normalize_git_url;
use super::{find_many, upsert_by_id, upsert_one, MongoStore};
use crate::auth::CurrentUser;
use crate::models::*;

impl MongoStore {
    pub async fn list_projects(
        &self,
        user: &CurrentUser,
        status: Option<ProjectStatus>,
    ) -> Result<Vec<ProjectRecord>, String> {
        let mut filter = Document::new();
        if !user.is_admin() {
            let owner_user_id = user
                .effective_owner_user_id()
                .ok_or_else(|| "当前登录态缺少用户归属信息".to_string())?;
            filter.insert("owner_user_id", owner_user_id);
        }
        if let Some(status) = status {
            filter.insert("status", status.as_str());
        }
        let projects = find_many(
            &self.projects,
            filter,
            Some(doc! { "updated_at": -1, "id": 1 }),
        )
        .await?;
        Ok(projects
            .into_iter()
            .map(normalize_project_execution_plane)
            .collect())
    }

    pub async fn list_all_projects(
        &self,
        status: Option<ProjectStatus>,
    ) -> Result<Vec<ProjectRecord>, String> {
        let mut filter = Document::new();
        if let Some(status) = status {
            filter.insert("status", status.as_str());
        }
        let projects = find_many(
            &self.projects,
            filter,
            Some(doc! { "updated_at": -1, "id": 1 }),
        )
        .await?;
        Ok(projects
            .into_iter()
            .map(normalize_project_execution_plane)
            .collect())
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
            execution_plane: source_type.execution_plane(),
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
        upsert_by_id(&self.projects, &project.id, &project).await?;
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
            execution_plane: source_type.execution_plane(),
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
                normalized_optional(input.archived_at).or(Some(now))
            } else {
                None
            },
        };
        upsert_by_id(&self.projects, &project.id, &project).await?;
        Ok(project)
    }

    pub async fn save_project_record(&self, project: &ProjectRecord) -> Result<(), String> {
        let project = normalize_project_execution_plane(project.clone());
        upsert_by_id(&self.projects, &project.id, &project).await
    }

    pub async fn get_project(&self, id: &str) -> Result<Option<ProjectRecord>, String> {
        let project = self
            .projects
            .find_one(doc! { "id": id.trim() }, None)
            .await
            .map_err(|err| err.to_string())?;
        Ok(project.map(normalize_project_execution_plane))
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
        upsert_by_id(&self.projects, &project.id, &project).await?;
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
        upsert_by_id(&self.projects, &project.id, &project).await?;
        Ok(Some(project))
    }

    pub async fn get_project_profile(
        &self,
        project_id: &str,
    ) -> Result<Option<ProjectProfileRecord>, String> {
        self.project_profiles
            .find_one(doc! { "project_id": project_id }, None)
            .await
            .map_err(|err| err.to_string())
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
        upsert_one(
            &self.project_profiles,
            doc! { "project_id": project_id },
            &profile,
        )
        .await?;
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

fn normalize_project_execution_plane(mut project: ProjectRecord) -> ProjectRecord {
    project.execution_plane = project.source_type.execution_plane();
    project
}

#[cfg(test)]
mod execution_plane_tests {
    use super::*;

    #[test]
    fn project_source_type_selects_execution_plane() {
        assert_eq!(ProjectSourceType::default(), ProjectSourceType::Cloud);
        assert_eq!(
            ProjectExecutionPlane::default(),
            ProjectExecutionPlane::Cloud
        );
        assert_eq!(
            ProjectSourceType::Cloud.execution_plane(),
            ProjectExecutionPlane::Cloud
        );
        assert_eq!(
            ProjectSourceType::Local.execution_plane(),
            ProjectExecutionPlane::LocalConnector
        );
        assert_eq!(
            ProjectSourceType::LocalConnector.execution_plane(),
            ProjectExecutionPlane::LocalConnector
        );
    }
}
