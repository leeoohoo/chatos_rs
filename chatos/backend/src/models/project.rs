// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::Config;
use crate::services::{access_token_scope, project_management_api_client};

pub const PUBLIC_PROJECT_ID: &str = "-1";
pub const HARNESS_PROJECT_ROOT_PREFIX: &str = "harness://project/";

pub fn harness_project_root_path(project_id: &str) -> String {
    format!("{HARNESS_PROJECT_ROOT_PREFIX}{}", project_id.trim())
}

pub fn harness_project_id_from_root_path(root_path: &str) -> Option<&str> {
    let project_id = root_path.trim().strip_prefix(HARNESS_PROJECT_ROOT_PREFIX)?;
    if project_id.is_empty() || project_id.contains('/') {
        return None;
    }
    Some(project_id)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub git_url: Option<String>,
    pub source_type: Option<String>,
    pub execution_plane: Option<String>,
    pub cloud_import_source: Option<String>,
    pub import_status: Option<String>,
    pub source_git_url: Option<String>,
    pub harness_space_identifier: Option<String>,
    pub harness_repo_identifier: Option<String>,
    pub harness_repo_path: Option<String>,
    pub harness_git_url: Option<String>,
    pub harness_git_ssh_url: Option<String>,
    pub import_error: Option<String>,
    pub import_started_at: Option<String>,
    pub import_finished_at: Option<String>,
    pub description: Option<String>,
    pub user_id: Option<String>,
    pub latest_session_id: Option<String>,
    pub last_message_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Project {
    pub fn new(
        name: String,
        root_path: String,
        git_url: Option<String>,
        description: Option<String>,
        user_id: Option<String>,
    ) -> Project {
        let now = crate::core::time::now_rfc3339();
        Project {
            id: Uuid::new_v4().to_string(),
            name,
            root_path,
            git_url,
            source_type: Some("local".to_string()),
            execution_plane: Some("local_connector".to_string()),
            cloud_import_source: Some("none".to_string()),
            import_status: Some("none".to_string()),
            source_git_url: None,
            harness_space_identifier: None,
            harness_repo_identifier: None,
            harness_repo_path: None,
            harness_git_url: None,
            harness_git_ssh_url: None,
            import_error: None,
            import_started_at: None,
            import_finished_at: None,
            description,
            user_id,
            latest_session_id: None,
            last_message_at: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

pub struct ProjectService;

impl ProjectService {
    pub async fn create(data: Project) -> Result<String, String> {
        let cfg = Config::try_get()?;
        let access_token = current_access_token_required()?;
        let project = project_management_api_client::create_project_service_project(
            cfg.project_service_base_url.as_str(),
            access_token.as_str(),
            &project_management_api_client::CreateProjectServiceProjectRequest {
                name: data.name,
                root_path: normalize_optional_text(Some(data.root_path)),
                git_url: normalize_optional_text(data.git_url),
                description: normalize_optional_text(data.description),
            },
        )
        .await?;
        Ok(project.id)
    }

    pub async fn create_cloud(
        name: String,
        git_url: Option<String>,
        zip: Option<(String, Vec<u8>)>,
        description: Option<String>,
    ) -> Result<Project, String> {
        let cfg = Config::try_get()?;
        let access_token = current_access_token_required()?;
        let project = project_management_api_client::create_cloud_project_service_project(
            cfg.project_service_base_url.as_str(),
            access_token.as_str(),
            &project_management_api_client::CreateCloudProjectServiceProjectRequest {
                name,
                git_url: normalize_optional_text(git_url),
                description: normalize_optional_text(description),
                zip,
            },
        )
        .await?;
        Ok(project_from_project_service(project))
    }

    pub async fn get_by_id(id: &str) -> Result<Option<Project>, String> {
        let id = normalize_project_id(id);
        let cfg = Config::try_get()?;
        let record = if let Some(access_token) = access_token_scope::get_current_access_token() {
            project_management_api_client::get_project_service_project(
                cfg.project_service_base_url.as_str(),
                access_token.as_str(),
                id.as_str(),
            )
            .await?
        } else {
            let Some(secret) = sync_secret(cfg)? else {
                return Err("project service sync secret is not configured".to_string());
            };
            project_management_api_client::sync_get_project_service_project(
                cfg.project_service_base_url.as_str(),
                secret.as_str(),
                id.as_str(),
            )
            .await?
        };
        Ok(record.map(project_from_project_service))
    }

    pub async fn list(user_id: Option<String>) -> Result<Vec<Project>, String> {
        let cfg = Config::try_get()?;
        let records = if let Some(access_token) = access_token_scope::get_current_access_token() {
            project_management_api_client::list_project_service_projects(
                cfg.project_service_base_url.as_str(),
                access_token.as_str(),
                Some("active"),
            )
            .await?
        } else {
            let Some(secret) = sync_secret(cfg)? else {
                return Err("project service sync secret is not configured".to_string());
            };
            project_management_api_client::sync_list_project_service_projects(
                cfg.project_service_base_url.as_str(),
                secret.as_str(),
                Some("active"),
            )
            .await?
        };
        let user_id = user_id.and_then(|value| normalize_optional_text(Some(value)));
        Ok(records
            .into_iter()
            .filter(|record| record.id != PUBLIC_PROJECT_ID)
            .filter(|record| {
                user_id
                    .as_deref()
                    .is_none_or(|value| record.owner_user_id.as_deref() == Some(value))
            })
            .map(project_from_project_service)
            .collect())
    }

    pub async fn update(
        id: &str,
        name: Option<String>,
        root_path: Option<String>,
        git_url: Option<String>,
        description: Option<String>,
    ) -> Result<(), String> {
        let cfg = Config::try_get()?;
        let access_token = current_access_token_required()?;
        let id = normalize_project_id(id);
        project_management_api_client::update_project_service_project(
            cfg.project_service_base_url.as_str(),
            access_token.as_str(),
            id.as_str(),
            &project_management_api_client::UpdateProjectServiceProjectRequest {
                name: normalize_optional_text(name),
                root_path: normalize_optional_text(root_path),
                git_url: normalize_optional_text(git_url),
                description: normalize_optional_text(description),
            },
        )
        .await?;
        Ok(())
    }

    pub async fn delete(id: &str) -> Result<(), String> {
        let cfg = Config::try_get()?;
        let access_token = current_access_token_required()?;
        let id = normalize_project_id(id);
        project_management_api_client::archive_project_service_project(
            cfg.project_service_base_url.as_str(),
            access_token.as_str(),
            id.as_str(),
        )
        .await?;
        Ok(())
    }
}

fn project_from_project_service(
    record: project_management_api_client::ProjectServiceProjectRecord,
) -> Project {
    let is_cloud_project = record
        .source_type
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| value.eq_ignore_ascii_case("cloud"));
    let root_path = record
        .root_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            if is_cloud_project {
                harness_project_root_path(record.id.as_str())
            } else {
                String::new()
            }
        });
    Project {
        id: record.id,
        name: record.name,
        root_path,
        git_url: record.git_url,
        source_type: record.source_type,
        execution_plane: record.execution_plane,
        cloud_import_source: record.cloud_import_source,
        import_status: record.import_status,
        source_git_url: record.source_git_url,
        harness_space_identifier: record.harness_space_identifier,
        harness_repo_identifier: record.harness_repo_identifier,
        harness_repo_path: record.harness_repo_path,
        harness_git_url: record.harness_git_url,
        harness_git_ssh_url: record.harness_git_ssh_url,
        import_error: record.import_error,
        import_started_at: record.import_started_at,
        import_finished_at: record.import_finished_at,
        description: record.description,
        user_id: record.owner_user_id,
        latest_session_id: None,
        last_message_at: None,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

fn current_access_token_required() -> Result<String, String> {
    access_token_scope::get_current_access_token()
        .ok_or_else(|| "current user access token is required for project mutation".to_string())
}

fn sync_secret(cfg: &Config) -> Result<Option<String>, String> {
    Ok(cfg
        .project_service_sync_secret
        .as_deref()
        .or(cfg.task_runner_callback_secret.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned))
}

pub fn normalize_project_id(id: &str) -> String {
    let trimmed = id.trim();
    if trimmed.is_empty() || trimmed == "0" {
        PUBLIC_PROJECT_ID.to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{
        harness_project_id_from_root_path, project_from_project_service,
        HARNESS_PROJECT_ROOT_PREFIX,
    };
    use crate::services::project_management_api_client::ProjectServiceProjectRecord;

    fn project_record(source_type: &str, root_path: Option<&str>) -> ProjectServiceProjectRecord {
        ProjectServiceProjectRecord {
            id: "project-1".to_string(),
            owner_user_id: Some("user-1".to_string()),
            name: "Project".to_string(),
            root_path: root_path.map(ToOwned::to_owned),
            git_url: None,
            source_type: Some(source_type.to_string()),
            execution_plane: None,
            cloud_import_source: None,
            import_status: None,
            source_git_url: None,
            harness_space_identifier: None,
            harness_repo_identifier: None,
            harness_repo_path: None,
            harness_git_url: None,
            harness_git_ssh_url: None,
            import_error: None,
            import_started_at: None,
            import_finished_at: None,
            description: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn cloud_project_uses_internal_harness_virtual_root() {
        let project = project_from_project_service(project_record("cloud", None));
        assert_eq!(
            project.root_path,
            format!("{HARNESS_PROJECT_ROOT_PREFIX}project-1")
        );
        assert_eq!(project.source_type.as_deref(), Some("cloud"));
    }

    #[test]
    fn local_project_keeps_its_real_root_path() {
        let project =
            project_from_project_service(project_record("local", Some("/workspace/local-project")));
        assert_eq!(project.root_path, "/workspace/local-project");
        assert_eq!(project.source_type.as_deref(), Some("local"));
    }

    #[test]
    fn harness_root_exposes_only_the_project_id() {
        assert_eq!(
            harness_project_id_from_root_path("harness://project/project-1"),
            Some("project-1")
        );
        assert_eq!(
            harness_project_id_from_root_path("harness://project/project-1/src"),
            None
        );
        assert_eq!(
            harness_project_id_from_root_path("/workspace/project-1"),
            None
        );
    }
}
