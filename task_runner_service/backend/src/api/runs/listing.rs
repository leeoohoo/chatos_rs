use super::*;

#[derive(Debug, Default, Deserialize)]
pub(in crate::api) struct RunListQuery {
    task_id: Option<String>,
    status: Option<TaskRunStatus>,
    model_config_id: Option<String>,
    keyword: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
pub(in crate::api) struct RunSummaryQuery {
    ids: Option<String>,
    task_id: Option<String>,
    status: Option<TaskRunStatus>,
    model_config_id: Option<String>,
    keyword: Option<String>,
    limit: Option<usize>,
}

pub(in crate::api) async fn list_task_runs(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Query(query): Query<RunListQuery>,
) -> Result<Json<Vec<TaskRunRecord>>, ApiError> {
    let runs = state
        .run_service
        .list_runs_filtered(RunListFilters {
            task_id: Some(id),
            status: query.status,
            model_config_id: query.model_config_id,
            keyword: query.keyword,
            limit: query.limit,
            offset: query.offset,
        })
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(runs))
}

pub(in crate::api) async fn list_runs(
    State(state): State<AppState>,
    Query(query): Query<RunListQuery>,
) -> Result<Json<Vec<TaskRunRecord>>, ApiError> {
    let runs = state
        .run_service
        .list_runs_filtered(RunListFilters {
            task_id: query.task_id,
            status: query.status,
            model_config_id: query.model_config_id,
            keyword: query.keyword,
            limit: query.limit,
            offset: query.offset,
        })
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(runs))
}

pub(in crate::api) async fn list_runs_page(
    State(state): State<AppState>,
    Query(query): Query<RunListQuery>,
) -> Result<Json<PaginatedResponse<TaskRunRecord>>, ApiError> {
    let page = state
        .run_service
        .list_runs_page(RunListFilters {
            task_id: query.task_id,
            status: query.status,
            model_config_id: query.model_config_id,
            keyword: query.keyword,
            limit: query.limit,
            offset: query.offset,
        })
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(page))
}

pub(in crate::api) async fn list_run_index(
    State(state): State<AppState>,
    Query(query): Query<RunListQuery>,
) -> Result<Json<Vec<RunSummaryRecord>>, ApiError> {
    let runs = state
        .run_service
        .run_index(RunListFilters {
            task_id: query.task_id,
            status: query.status,
            model_config_id: query.model_config_id,
            keyword: query.keyword,
            limit: query.limit,
            offset: query.offset,
        })
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(runs))
}

pub(in crate::api) async fn list_run_summaries(
    State(state): State<AppState>,
    Query(query): Query<RunSummaryQuery>,
) -> Result<Json<Vec<RunSummaryRecord>>, ApiError> {
    let summaries = if let Some(ids) = query.ids {
        state
            .run_service
            .get_run_summaries_by_ids(parse_csv_ids(&ids))
            .await
    } else {
        state
            .run_service
            .run_index(RunListFilters {
                task_id: query.task_id,
                status: query.status,
                model_config_id: query.model_config_id,
                keyword: query.keyword,
                limit: query.limit,
                offset: None,
            })
            .await
    }
    .map_err(ApiError::bad_request)?;
    Ok(Json(summaries))
}
