use super::*;

pub(in crate::api) async fn list_tasks(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Query(query): Query<TaskListQuery>,
) -> Result<Json<Vec<TaskRecord>>, ApiError> {
    let tasks = state
        .task_service
        .list_tasks_filtered(task_filters_for_user(query.into_filters(), &current_user)?)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(tasks))
}

pub(in crate::api) async fn list_tasks_page(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Query(query): Query<TaskListQuery>,
) -> Result<Json<PaginatedResponse<TaskRecord>>, ApiError> {
    let page = state
        .task_service
        .list_tasks_page(task_filters_for_user(query.into_filters(), &current_user)?)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(page))
}

pub(in crate::api) async fn list_task_summaries(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Query(query): Query<TaskSummaryQuery>,
) -> Result<Json<Vec<TaskSummaryRecord>>, ApiError> {
    let summaries = if let Some(ids) = query.ids {
        state
            .task_service
            .get_task_summaries_by_ids(parse_csv_ids(&ids))
            .await
            .map(|items| {
                items
                    .into_iter()
                    .filter(|item| {
                        owned_resource_visible_to_user(
                            item.creator_user_id.as_deref(),
                            &current_user,
                        )
                        .unwrap_or(false)
                    })
                    .collect::<Vec<_>>()
            })
    } else {
        state
            .task_service
            .list_task_summaries_filtered(task_filters_for_user(
                TaskListFilters {
                    status: query.status,
                    keyword: query.keyword,
                    limit: query.limit,
                    include_subtasks: Some(false),
                    ..TaskListFilters::default()
                },
                &current_user,
            )?)
            .await
    }
    .map_err(ApiError::bad_request)?;
    Ok(Json(summaries))
}

pub(in crate::api) async fn get_task_index(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<TaskIndexResponse>, ApiError> {
    if current_user.is_admin() {
        let index = state
            .task_service
            .task_index()
            .await
            .map_err(ApiError::bad_request)?;
        return Ok(Json(index));
    }
    let tasks = state
        .task_service
        .list_tasks_filtered(task_filters_for_user(
            TaskListFilters::default(),
            &current_user,
        )?)
        .await
        .map_err(ApiError::bad_request)?;
    let mut tags = tasks
        .iter()
        .flat_map(|task| task.tags.iter().cloned())
        .collect::<Vec<_>>();
    tags.sort();
    tags.dedup();
    let index = TaskIndexResponse {
        tasks: tasks.iter().map(TaskSummaryRecord::from).collect(),
        tags,
    };
    Ok(Json(index))
}

pub(in crate::api) async fn get_task_stats(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<TaskStatsResponse>, ApiError> {
    if current_user.is_admin() {
        let stats = state
            .task_service
            .task_stats()
            .await
            .map_err(ApiError::bad_request)?;
        return Ok(Json(stats));
    }
    let tasks = state
        .task_service
        .list_tasks_filtered(task_filters_for_user(
            TaskListFilters::default(),
            &current_user,
        )?)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(task_stats_from_tasks(&tasks)))
}
