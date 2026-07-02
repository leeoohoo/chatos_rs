// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl TaskProjectService {
    #[cfg(test)]
    pub(crate) fn new(store: AppStore) -> Self {
        Self {
            config: None,
            store,
        }
    }

    pub(crate) fn new_with_config(store: AppStore, config: AppConfig) -> Self {
        Self {
            config: Some(config),
            store,
        }
    }

    fn project_service_config(&self) -> Option<&AppConfig> {
        self.config
            .as_ref()
            .filter(|config| super::project_management_api_client::project_service_enabled(config))
    }

    pub async fn ensure_public_project(&self) -> Result<TaskProjectRecord, String> {
        let now = now_rfc3339();
        let existing = self.store.get_task_project(PUBLIC_PROJECT_ID).await?;
        if let Some(project) = existing {
            return Ok(project);
        }
        self.store
            .save_task_project(TaskProjectRecord {
                id: PUBLIC_PROJECT_ID.to_string(),
                owner_user_id: None,
                owner_username: None,
                owner_display_name: None,
                name: "Public".to_string(),
                root_path: None,
                git_url: None,
                description: Some("Default public project space".to_string()),
                status: TaskProjectStatus::Active,
                created_at: now.clone(),
                updated_at: now,
                archived_at: None,
            })
            .await
    }

    pub async fn list_projects(&self) -> Result<Vec<TaskProjectRecord>, String> {
        if let Some(config) = self.project_service_config() {
            return super::project_management_api_client::sync_list_projects(config, None).await;
        }
        let mut projects = self.store.list_task_projects().await?;
        if !projects
            .iter()
            .any(|project| project.id == PUBLIC_PROJECT_ID)
        {
            projects.push(self.ensure_public_project().await?);
            projects.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        }
        Ok(projects)
    }

    pub async fn list_projects_for_user(
        &self,
        user: &CurrentUser,
    ) -> Result<Vec<TaskProjectRecord>, String> {
        if let Some(config) = self.project_service_config() {
            let mut projects =
                super::project_management_api_client::list_projects_for_user(config, None).await?;
            projects.retain(|project| project.id != PUBLIC_PROJECT_ID);
            projects.insert(0, self.public_project_for_user(user).await?);
            return Ok(projects);
        }
        let mut projects = self
            .list_projects()
            .await?
            .into_iter()
            .filter(|project| project.id != PUBLIC_PROJECT_ID)
            .collect::<Vec<_>>();
        projects.insert(0, self.public_project_for_user(user).await?);
        Ok(projects)
    }

    pub async fn get_project(&self, id: &str) -> Result<Option<TaskProjectRecord>, String> {
        let id = normalize_project_lookup_id(id);
        if id == PUBLIC_PROJECT_ID {
            return self.ensure_public_project().await.map(Some);
        }
        if let Some(config) = self.project_service_config() {
            return super::project_management_api_client::sync_get_project(config, id.as_str())
                .await;
        }
        self.store.get_task_project(id.as_str()).await
    }

    pub async fn get_project_for_user(
        &self,
        id: &str,
        user: &CurrentUser,
    ) -> Result<Option<TaskProjectRecord>, String> {
        let id = normalize_project_lookup_id(id);
        if id == PUBLIC_PROJECT_ID {
            return self.public_project_for_user(user).await.map(Some);
        }
        if let Some(config) = self.project_service_config() {
            return super::project_management_api_client::get_project_for_user(config, id.as_str())
                .await;
        }
        self.store.get_task_project(id.as_str()).await
    }

    pub async fn public_project_for_user(
        &self,
        user: &CurrentUser,
    ) -> Result<TaskProjectRecord, String> {
        let template = self.ensure_public_project().await?;
        Ok(TaskProjectRecord {
            owner_user_id: Some(public_owner_user_id(user)),
            owner_username: public_owner_username(user),
            owner_display_name: public_owner_display_name(user),
            description: Some("Default public project space for this user".to_string()),
            ..template
        })
    }

    pub async fn create_project(
        &self,
        input: CreateTaskProjectRequest,
        creator: &CurrentUser,
    ) -> Result<TaskProjectRecord, String> {
        if let Some(config) = self.project_service_config() {
            let _ = creator;
            return super::project_management_api_client::create_project(config, &input).await;
        }
        validate_required("name", &input.name)?;
        let owner_user_id = creator
            .effective_owner_user_id()
            .map(ToOwned::to_owned)
            .ok_or_else(|| "当前登录态缺少用户归属信息，无法创建项目".to_string())?;
        let now = now_rfc3339();
        let project = TaskProjectRecord {
            id: Uuid::new_v4().to_string(),
            owner_user_id: Some(owner_user_id),
            owner_username: creator.effective_owner_username().map(ToOwned::to_owned),
            owner_display_name: creator
                .effective_owner_display_name()
                .map(ToOwned::to_owned)
                .or_else(|| creator.effective_owner_username().map(ToOwned::to_owned)),
            name: input.name.trim().to_string(),
            root_path: normalized_optional(input.root_path),
            git_url: normalize_git_url(input.git_url)?,
            description: normalized_optional(input.description),
            status: TaskProjectStatus::Active,
            created_at: now.clone(),
            updated_at: now,
            archived_at: None,
        };
        self.store.save_task_project(project).await
    }

    pub async fn import_chatos_project(
        &self,
        input: ChatosProjectImportRequest,
    ) -> Result<TaskProjectRecord, String> {
        if let Some(config) = self.project_service_config() {
            return super::project_management_api_client::import_project(config, &input).await;
        }
        let id = input.id.trim();
        validate_required("id", id)?;
        if id == PUBLIC_PROJECT_ID {
            return Err("public project cannot be imported or overwritten".to_string());
        }
        validate_required("name", &input.name)?;
        let now = now_rfc3339();
        let status = input.status.unwrap_or(TaskProjectStatus::Active);
        let archived_at = if status == TaskProjectStatus::Archived {
            normalized_optional(input.archived_at).or_else(|| Some(now.clone()))
        } else {
            None
        };
        let project = TaskProjectRecord {
            id: id.to_string(),
            owner_user_id: normalized_optional(input.owner_user_id),
            owner_username: normalized_optional(input.owner_username),
            owner_display_name: normalized_optional(input.owner_display_name),
            name: input.name.trim().to_string(),
            root_path: normalized_optional(input.root_path),
            git_url: normalize_git_url(input.git_url)?,
            description: normalized_optional(input.description),
            status,
            created_at: normalized_optional(input.created_at).unwrap_or_else(|| now.clone()),
            updated_at: normalized_optional(input.updated_at).unwrap_or_else(|| now.clone()),
            archived_at,
        };
        self.store.save_task_project(project).await
    }

    pub async fn update_project(
        &self,
        id: &str,
        patch: UpdateTaskProjectRequest,
    ) -> Result<Option<TaskProjectRecord>, String> {
        let id = normalize_project_lookup_id(id);
        if id == PUBLIC_PROJECT_ID {
            return Err("public project cannot be updated".to_string());
        }
        if let Some(config) = self.project_service_config() {
            return super::project_management_api_client::update_project(
                config,
                id.as_str(),
                &patch,
            )
            .await;
        }
        let Some(mut project) = self.store.get_task_project(id.as_str()).await? else {
            return Ok(None);
        };
        if let Some(name) = patch.name {
            validate_required("name", &name)?;
            project.name = name.trim().to_string();
        }
        if let Some(root_path) = patch.root_path {
            project.root_path = normalized_optional(Some(root_path));
        }
        if patch.git_url.is_some() {
            project.git_url = normalize_git_url(patch.git_url)?;
        }
        if let Some(description) = patch.description {
            project.description = normalized_optional(Some(description));
        }
        project.updated_at = now_rfc3339();
        Ok(Some(self.store.save_task_project(project).await?))
    }

    pub async fn archive_project(&self, id: &str) -> Result<Option<TaskProjectRecord>, String> {
        let id = normalize_project_lookup_id(id);
        if id == PUBLIC_PROJECT_ID {
            return Err("public project cannot be archived".to_string());
        }
        if let Some(config) = self.project_service_config() {
            return super::project_management_api_client::archive_project(config, id.as_str())
                .await;
        }
        let Some(mut project) = self.store.get_task_project(id.as_str()).await? else {
            return Ok(None);
        };
        let now = now_rfc3339();
        project.status = TaskProjectStatus::Archived;
        project.archived_at = Some(now.clone());
        project.updated_at = now;
        Ok(Some(self.store.save_task_project(project).await?))
    }
}

impl TaskService {
    pub(super) async fn ensure_project_available_for_task(
        &self,
        project_id: &str,
        current_user: Option<&CurrentUser>,
    ) -> Result<(), String> {
        if self
            .config
            .project_service_base_url
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty())
        {
            let project = super::project_management_api_client::get_project_from_project_service(
                &self.config,
                project_id,
            )
            .await?
            .ok_or_else(|| format!("项目不存在: {project_id}"))?;
            ensure_project_active_for_user(&project, current_user)?;
            return Ok(());
        }

        let Some(project) = self.store.get_task_project(project_id).await? else {
            return Err(format!("项目不存在: {project_id}"));
        };
        ensure_project_active_for_user(&project, current_user)
    }
}

pub(crate) fn normalize_project_lookup_id(id: &str) -> String {
    let trimmed = id.trim();
    if trimmed.is_empty() || trimmed == "0" {
        PUBLIC_PROJECT_ID.to_string()
    } else {
        trimmed.to_string()
    }
}

pub(crate) fn project_visible_to_user(project: &TaskProjectRecord, user: &CurrentUser) -> bool {
    if project.id == PUBLIC_PROJECT_ID || user.is_admin() {
        return true;
    }
    let Some(owner_user_id) = project.owner_user_id.as_deref().map(str::trim) else {
        return false;
    };
    user.effective_owner_user_id() == Some(owner_user_id)
}

pub(crate) fn ensure_project_active_for_user(
    project: &TaskProjectRecord,
    user: Option<&CurrentUser>,
) -> Result<(), String> {
    if project.status != TaskProjectStatus::Active {
        return Err(format!("项目已归档，不能继续使用: {}", project.id));
    }
    if let Some(user) = user {
        if !project_visible_to_user(project, user) {
            return Err("当前用户无权访问该项目".to_string());
        }
    }
    Ok(())
}

fn normalize_git_url(value: Option<String>) -> Result<Option<String>, String> {
    let Some(value) = normalized_optional(value) else {
        return Ok(None);
    };
    if value.len() > 2048 {
        return Err("git_url 过长".to_string());
    }
    if value.chars().any(char::is_whitespace) {
        return Err("git_url 不能包含空白字符".to_string());
    }
    let lower = value.to_ascii_lowercase();
    let is_supported = lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("ssh://")
        || lower.starts_with("git@");
    if !is_supported {
        return Err(
            "git_url 需要是常见 Git 地址，例如 https://、ssh:// 或 git@host:path".to_string(),
        );
    }
    Ok(Some(value))
}

fn public_owner_user_id(user: &CurrentUser) -> String {
    user.effective_owner_user_id()
        .or_else(|| non_empty_text(user.id.as_str()))
        .unwrap_or("unknown")
        .to_string()
}

fn public_owner_username(user: &CurrentUser) -> Option<String> {
    user.effective_owner_username()
        .or_else(|| non_empty_text(user.username.as_str()))
        .map(ToOwned::to_owned)
}

fn public_owner_display_name(user: &CurrentUser) -> Option<String> {
    user.effective_owner_display_name()
        .or_else(|| user.effective_owner_username())
        .or_else(|| non_empty_text(user.display_name.as_str()))
        .or_else(|| non_empty_text(user.username.as_str()))
        .map(ToOwned::to_owned)
}

fn non_empty_text(value: &str) -> Option<&str> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, StoreMode};
    use crate::models::UserRole;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    fn test_config() -> AppConfig {
        AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            role: crate::config::TaskRunnerRole::All,
            store_mode: StoreMode::Memory,
            database_url: "memory://task-project-service-test".to_string(),
            memory_engine_base_url: None,
            memory_engine_source_id: "task".to_string(),
            memory_engine_operator_token: None,
            default_tenant_id: "tenant".to_string(),
            default_subject_id: "subject".to_string(),
            default_workspace_dir: ".".to_string(),
            memory_timeout: Duration::from_millis(1000),
            execution_timeout: Duration::from_millis(1000),
            scheduler_poll_interval: Duration::from_millis(1000),
            worker_id: "test-worker".to_string(),
            worker_poll_interval: Duration::from_millis(1_000),
            worker_claim_ttl: Duration::from_millis(120_000),
            worker_concurrency: 4,
            auto_memory_summary: false,
            default_task_execution_max_iterations: 1,
            default_tool_result_model_max_chars: 1000,
            default_tool_results_model_total_max_chars: 2000,
            default_execution_environment_mode: "local".to_string(),
            default_sandbox_manager_base_url: "http://127.0.0.1:8095".to_string(),
            sandbox_manager_client_id: None,
            sandbox_manager_client_key: None,
            default_sandbox_lease_ttl_seconds: 7_200,
            chatos_callback_url: None,
            chatos_callback_secret: None,
            internal_api_secret: None,
            callback_timeout: Duration::from_millis(1000),
            admin_username: "admin".to_string(),
            admin_password: "admin".to_string(),
            admin_display_name: "Admin".to_string(),
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_request_timeout: Duration::from_millis(5000),
            project_service_base_url: None,
            project_service_sync_secret: None,
            project_service_request_timeout: Duration::from_millis(5000),
        }
    }

    async fn test_service() -> TaskProjectService {
        let config = test_config();
        let store = AppStore::new(&config).await.expect("store");
        TaskProjectService::new(store)
    }

    fn creator() -> CurrentUser {
        CurrentUser {
            id: "agent-1".to_string(),
            username: "agent".to_string(),
            display_name: "Agent".to_string(),
            role: UserRole::Agent,
            owner_user_id: Some("owner-1".to_string()),
            owner_username: Some("owner".to_string()),
            owner_display_name: Some("Owner".to_string()),
        }
    }

    #[tokio::test]
    async fn get_project_normalizes_legacy_zero_to_public() {
        let service = test_service().await;

        let project = service
            .get_project("0")
            .await
            .expect("get project")
            .expect("public project");

        assert_eq!(project.id, PUBLIC_PROJECT_ID);
        assert_eq!(project.git_url, None);
    }

    #[tokio::test]
    async fn get_project_for_user_returns_owner_scoped_public_project() {
        let service = test_service().await;
        let user = creator();

        let project = service
            .get_project_for_user("0", &user)
            .await
            .expect("get project")
            .expect("public project");

        assert_eq!(project.id, PUBLIC_PROJECT_ID);
        assert_eq!(project.owner_user_id.as_deref(), Some("owner-1"));
        assert_eq!(project.owner_username.as_deref(), Some("owner"));
        assert_eq!(project.owner_display_name.as_deref(), Some("Owner"));
    }

    #[tokio::test]
    async fn list_projects_for_user_replaces_global_public_template() {
        let service = test_service().await;
        let user = creator();
        service
            .ensure_public_project()
            .await
            .expect("ensure public template");

        let projects = service
            .list_projects_for_user(&user)
            .await
            .expect("list projects");
        let public_projects = projects
            .iter()
            .filter(|project| project.id == PUBLIC_PROJECT_ID)
            .collect::<Vec<_>>();

        assert_eq!(public_projects.len(), 1);
        assert_eq!(public_projects[0].owner_user_id.as_deref(), Some("owner-1"));
    }

    #[tokio::test]
    async fn create_project_accepts_common_git_url() {
        let service = test_service().await;
        let user = creator();

        let project = service
            .create_project(
                CreateTaskProjectRequest {
                    name: "Repo Project".to_string(),
                    root_path: None,
                    git_url: Some(" git@github.com:org/repo.git ".to_string()),
                    description: None,
                },
                &user,
            )
            .await
            .expect("create project");

        assert_eq!(
            project.git_url.as_deref(),
            Some("git@github.com:org/repo.git")
        );
    }

    #[tokio::test]
    async fn create_project_rejects_unsupported_git_url() {
        let service = test_service().await;
        let user = creator();

        let err = service
            .create_project(
                CreateTaskProjectRequest {
                    name: "Repo Project".to_string(),
                    root_path: None,
                    git_url: Some("example.com/org/repo.git".to_string()),
                    description: None,
                },
                &user,
            )
            .await
            .expect_err("unsupported git url should be rejected");

        assert!(err.contains("git_url"));
    }

    #[tokio::test]
    async fn update_project_can_clear_optional_git_url() {
        let service = test_service().await;
        let user = creator();
        let project = service
            .create_project(
                CreateTaskProjectRequest {
                    name: "Repo Project".to_string(),
                    root_path: None,
                    git_url: Some("https://example.com/org/repo.git".to_string()),
                    description: None,
                },
                &user,
            )
            .await
            .expect("create project");

        let updated = service
            .update_project(
                project.id.as_str(),
                UpdateTaskProjectRequest {
                    git_url: Some("   ".to_string()),
                    ..UpdateTaskProjectRequest::default()
                },
            )
            .await
            .expect("update project")
            .expect("project");

        assert_eq!(updated.git_url, None);
    }
}
