use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::Config;
use crate::services::{access_token_scope, task_runner_api_client};

pub const PUBLIC_PROJECT_ID: &str = "-1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub git_url: Option<String>,
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
        let project = task_runner_api_client::create_task_runner_project(
            cfg.task_runner_base_url.as_str(),
            access_token.as_str(),
            &task_runner_api_client::CreateTaskRunnerProjectRequest {
                name: data.name,
                root_path: normalize_optional_text(Some(data.root_path)),
                git_url: normalize_optional_text(data.git_url),
                description: normalize_optional_text(data.description),
            },
        )
        .await?;
        Ok(project.id)
    }

    pub async fn get_by_id(id: &str) -> Result<Option<Project>, String> {
        let id = normalize_project_id(id);
        let cfg = Config::try_get()?;
        let record = if let Some(access_token) = access_token_scope::get_current_access_token() {
            task_runner_api_client::get_task_runner_project(
                cfg.task_runner_base_url.as_str(),
                access_token.as_str(),
                id.as_str(),
            )
            .await?
        } else {
            let Some(secret) = sync_secret(cfg)? else {
                return Err("task runner sync secret is not configured".to_string());
            };
            task_runner_api_client::sync_get_task_runner_project(
                cfg.task_runner_base_url.as_str(),
                secret.as_str(),
                id.as_str(),
            )
            .await?
        };
        Ok(record.map(project_from_task_runner))
    }

    pub async fn list(user_id: Option<String>) -> Result<Vec<Project>, String> {
        let cfg = Config::try_get()?;
        let records = if let Some(access_token) = access_token_scope::get_current_access_token() {
            task_runner_api_client::list_task_runner_projects(
                cfg.task_runner_base_url.as_str(),
                access_token.as_str(),
                Some("active"),
            )
            .await?
        } else {
            let Some(secret) = sync_secret(cfg)? else {
                return Err("task runner sync secret is not configured".to_string());
            };
            task_runner_api_client::sync_list_task_runner_projects(
                cfg.task_runner_base_url.as_str(),
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
            .map(project_from_task_runner)
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
        task_runner_api_client::update_task_runner_project(
            cfg.task_runner_base_url.as_str(),
            access_token.as_str(),
            id.as_str(),
            &task_runner_api_client::UpdateTaskRunnerProjectRequest {
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
        task_runner_api_client::archive_task_runner_project(
            cfg.task_runner_base_url.as_str(),
            access_token.as_str(),
            id.as_str(),
        )
        .await?;
        Ok(())
    }
}

fn project_from_task_runner(record: task_runner_api_client::TaskRunnerProjectRecord) -> Project {
    Project {
        id: record.id,
        name: record.name,
        root_path: record.root_path.unwrap_or_default(),
        git_url: record.git_url,
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
        .task_runner_callback_secret
        .as_deref()
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
