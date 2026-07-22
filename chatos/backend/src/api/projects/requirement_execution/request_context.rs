// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use crate::config::Config;
use crate::core::auth::AuthUser;
use crate::core::project_access::{ensure_owned_project, ProjectAccessError};
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
