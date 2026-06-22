use super::*;

pub(in crate::api) async fn list_tasks(
    State(state): State<AppState>,
    Query(query): Query<TaskListQuery>,
) -> Result<Json<Vec<TaskRecord>>, ApiError> {
    let tasks = state
        .task_service
        .list_tasks_filtered(query.into_filters())
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(tasks))
}

pub(in crate::api) async fn list_tasks_page(
    State(state): State<AppState>,
    Query(query): Query<TaskListQuery>,
) -> Result<Json<PaginatedResponse<TaskRecord>>, ApiError> {
    let page = state
        .task_service
        .list_tasks_page(query.into_filters())
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(page))
}

pub(in crate::api) async fn list_task_summaries(
    State(state): State<AppState>,
    Query(query): Query<TaskSummaryQuery>,
) -> Result<Json<Vec<TaskSummaryRecord>>, ApiError> {
    let summaries = if let Some(ids) = query.ids {
        state
            .task_service
            .get_task_summaries_by_ids(parse_csv_ids(&ids))
            .await
    } else {
        state
            .task_service
            .list_task_summaries_filtered(TaskListFilters {
                status: query.status,
                keyword: query.keyword,
                creator_user_id: None,
                limit: query.limit,
                ..TaskListFilters::default()
            })
            .await
    }
    .map_err(ApiError::bad_request)?;
    Ok(Json(summaries))
}

pub(in crate::api) async fn get_task_index(
    State(state): State<AppState>,
) -> Result<Json<TaskIndexResponse>, ApiError> {
    let index = state
        .task_service
        .task_index()
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(index))
}

pub(in crate::api) async fn get_task_stats(
    State(state): State<AppState>,
) -> Result<Json<TaskStatsResponse>, ApiError> {
    let stats = state
        .task_service
        .task_stats()
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(stats))
}
