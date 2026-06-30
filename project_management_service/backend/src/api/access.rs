use super::ApiError;
use crate::auth::CurrentUser;
use crate::models::{ProjectRecord, ProjectStatus, ProjectWorkItemRecord, RequirementRecord};
use crate::state::AppState;

pub(in crate::api) async fn require_project_access(
    state: &AppState,
    project_id: &str,
    user: &CurrentUser,
) -> Result<ProjectRecord, ApiError> {
    let project = state
        .store
        .get_project(project_id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("项目不存在: {project_id}")))?;
    if user.can_access_owned_resource(project.owner_user_id.as_deref()) {
        Ok(project)
    } else {
        Err(ApiError::forbidden("无权访问该项目"))
    }
}

pub(in crate::api) async fn require_requirement_access(
    state: &AppState,
    requirement_id: &str,
    user: &CurrentUser,
) -> Result<RequirementRecord, ApiError> {
    let requirement = state
        .store
        .get_requirement(requirement_id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("需求不存在: {requirement_id}")))?;
    require_project_access(state, &requirement.project_id, user).await?;
    Ok(requirement)
}

pub(in crate::api) async fn require_work_item_access(
    state: &AppState,
    work_item_id: &str,
    user: &CurrentUser,
) -> Result<ProjectWorkItemRecord, ApiError> {
    let item = state
        .store
        .get_work_item(work_item_id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("项目工作项不存在: {work_item_id}")))?;
    require_project_access(state, &item.project_id, user).await?;
    Ok(item)
}

pub(in crate::api) fn ensure_project_writable(project: &ProjectRecord) -> Result<(), ApiError> {
    if project.status == ProjectStatus::Archived {
        Err(ApiError::bad_request("项目已归档，不能继续写入"))
    } else {
        Ok(())
    }
}
