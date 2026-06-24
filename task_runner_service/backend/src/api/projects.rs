use super::*;

#[derive(Debug, Default, Deserialize)]
pub(super) struct ProjectListQuery {
    status: Option<TaskProjectStatus>,
}

pub(super) async fn list_projects(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Query(query): Query<ProjectListQuery>,
) -> Result<Json<Vec<TaskProjectRecord>>, ApiError> {
    let projects = state
        .task_project_service
        .list_projects_for_user(&current_user)
        .await
        .map_err(ApiError::bad_request)?;
    let projects = projects
        .into_iter()
        .filter(|project| query.status.is_none_or(|status| project.status == status))
        .filter(|project| project_visible_to_user(project, &current_user).unwrap_or(false))
        .collect::<Vec<_>>();
    Ok(Json(projects))
}

pub(super) async fn create_project(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<CreateTaskProjectRequest>,
) -> Result<(StatusCode, Json<TaskProjectRecord>), ApiError> {
    let project = state
        .task_project_service
        .create_project(input, &current_user)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(project)))
}

pub(super) async fn get_project(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<TaskProjectRecord>, ApiError> {
    let project = state
        .task_project_service
        .get_project_for_user(&id, &current_user)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("项目不存在: {id}")))?;
    ensure_project_access(&project, &current_user)?;
    Ok(Json(project))
}

pub(super) async fn update_project(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<UpdateTaskProjectRequest>,
) -> Result<Json<TaskProjectRecord>, ApiError> {
    let existing = state
        .task_project_service
        .get_project_for_user(&id, &current_user)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("项目不存在: {id}")))?;
    ensure_project_access(&existing, &current_user)?;
    let project = state
        .task_project_service
        .update_project(&id, input)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("项目不存在: {id}")))?;
    Ok(Json(project))
}

pub(super) async fn delete_project(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<TaskProjectRecord>, ApiError> {
    let existing = state
        .task_project_service
        .get_project_for_user(&id, &current_user)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("项目不存在: {id}")))?;
    ensure_project_access(&existing, &current_user)?;
    let project = state
        .task_project_service
        .archive_project(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("项目不存在: {id}")))?;
    Ok(Json(project))
}

pub(super) async fn list_project_tasks(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<Vec<TaskRecord>>, ApiError> {
    let project = state
        .task_project_service
        .get_project_for_user(&id, &current_user)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("项目不存在: {id}")))?;
    ensure_project_access(&project, &current_user)?;
    let filters = task_filters_for_user(
        TaskListFilters {
            project_id: Some(project.id),
            ..TaskListFilters::default()
        },
        &current_user,
    )?;
    let tasks = state
        .task_service
        .list_tasks_filtered(filters)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(tasks))
}

pub(super) async fn import_chatos_project(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ChatosProjectImportRequest>,
) -> Result<Json<TaskProjectRecord>, ApiError> {
    super::router::require_chatos_sync_secret(&state, &headers)?;
    let project = state
        .task_project_service
        .import_chatos_project(input)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(project))
}

pub(super) async fn sync_list_projects(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ProjectListQuery>,
) -> Result<Json<Vec<TaskProjectRecord>>, ApiError> {
    super::router::require_chatos_sync_secret(&state, &headers)?;
    let projects = state
        .task_project_service
        .list_projects()
        .await
        .map_err(ApiError::bad_request)?
        .into_iter()
        .filter(|project| query.status.is_none_or(|status| project.status == status))
        .collect::<Vec<_>>();
    Ok(Json(projects))
}

pub(super) async fn sync_get_project(
    Path(id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<TaskProjectRecord>, ApiError> {
    super::router::require_chatos_sync_secret(&state, &headers)?;
    let project = state
        .task_project_service
        .get_project(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("项目不存在: {id}")))?;
    Ok(Json(project))
}

fn project_visible_to_user(
    project: &TaskProjectRecord,
    current_user: &CurrentUser,
) -> Result<bool, ApiError> {
    if project.id == PUBLIC_PROJECT_ID {
        return Ok(true);
    }
    owned_resource_visible_to_user(project.owner_user_id.as_deref(), current_user)
}

fn ensure_project_access(
    project: &TaskProjectRecord,
    current_user: &CurrentUser,
) -> Result<(), ApiError> {
    if project_visible_to_user(project, current_user)? {
        Ok(())
    } else {
        Err(ApiError::forbidden("无权访问该项目"))
    }
}
