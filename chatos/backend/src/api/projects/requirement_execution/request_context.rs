// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use crate::config::Config;
use crate::core::auth::AuthUser;
use crate::core::project_access::{ensure_owned_project, ProjectAccessError};
use crate::core::project_execution::project_uses_local_runtime;
use crate::models::project::Project;
use crate::services::{access_token_scope, project_management_api_client};

use super::errors::HandlerError;

pub(in crate::api::projects) struct RequirementExecutionRequestContext {
    pub(in crate::api::projects) cfg: &'static Config,
    pub(in crate::api::projects) project: Project,
    pub(in crate::api::projects) access_token: String,
    pub(in crate::api::projects) project_sync_secret: String,
    pub(in crate::api::projects) plan: Value,
}

pub(in crate::api::projects) async fn load_requirement_execution_request_context(
    auth: &AuthUser,
    project_id: &str,
) -> Result<RequirementExecutionRequestContext, HandlerError> {
    let project = ensure_owned_project(project_id, auth)
        .await
        .map_err(|err| match err {
            ProjectAccessError::NotFound => HandlerError::not_found("项目不存在"),
            ProjectAccessError::Forbidden => HandlerError::forbidden("无权访问该项目"),
            ProjectAccessError::Internal(err) => HandlerError::internal("读取项目失败", err),
        })?;
    ensure_cloud_requirement_execution_project(&project)?;
    let cfg = Config::try_get().map_err(|err| HandlerError::internal("配置未初始化", err))?;
    let access_token = access_token_scope::get_current_access_token()
        .ok_or_else(|| HandlerError::unauthorized("current user access token is required"))?;
    let project_sync_secret = cfg
        .project_service_sync_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            HandlerError::internal(
                "项目执行需要配置项目管理同步密钥",
                "CHATOS_PROJECT_SERVICE_SYNC_SECRET / PROJECT_SERVICE_SYNC_SECRET is required",
            )
        })?
        .to_string();
    let plan = project_management_api_client::get_project_service_plan(
        cfg.project_service_base_url.as_str(),
        access_token.as_str(),
        project.id.as_str(),
        false,
    )
    .await
    .map_err(|err| HandlerError::bad_gateway("read project plan snapshot failed", err))?;

    Ok(RequirementExecutionRequestContext {
        cfg,
        project,
        access_token,
        project_sync_secret,
        plan,
    })
}

fn ensure_cloud_requirement_execution_project(project: &Project) -> Result<(), HandlerError> {
    if project_uses_local_runtime(project) {
        return Err(HandlerError::conflict(
            "本地项目只能在 Local Connector 执行，禁止创建云端执行任务",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ensure_cloud_requirement_execution_project;
    use crate::models::project::Project;
    use axum::http::StatusCode;

    fn project(source_type: &str, execution_plane: &str) -> Project {
        let mut project = Project::new(
            "Execution plane test".to_string(),
            "/workspace/project".to_string(),
            None,
            None,
            Some("user-1".to_string()),
        );
        project.source_type = Some(source_type.to_string());
        project.execution_plane = Some(execution_plane.to_string());
        project
    }

    #[test]
    fn cloud_requirement_execution_rejects_local_projects_before_planning() {
        let error = ensure_cloud_requirement_execution_project(&project(
            "local_connector",
            "local_connector",
        ))
        .expect_err("local projects must never enter the cloud requirement executor");

        assert_eq!(error.status, StatusCode::CONFLICT);
        assert!(error.error.contains("禁止创建云端执行任务"));
    }

    #[test]
    fn cloud_requirement_execution_accepts_cloud_projects() {
        ensure_cloud_requirement_execution_project(&project("cloud", "cloud"))
            .expect("cloud project should use cloud requirement execution");
    }
}
