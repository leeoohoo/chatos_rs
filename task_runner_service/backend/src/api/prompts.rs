use super::*;

#[derive(Debug, Default, Deserialize)]
pub(super) struct PromptListQuery {
    task_id: Option<String>,
    run_id: Option<String>,
    status: Option<UiPromptStatus>,
    limit: Option<usize>,
    offset: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct PromptTaskCountQuery {
    status: Option<UiPromptStatus>,
}

pub(super) async fn list_prompts(
    State(state): State<AppState>,
    Query(query): Query<PromptListQuery>,
) -> Result<Json<Vec<UiPromptRecord>>, ApiError> {
    let page = state
        .ui_prompt_service
        .list_prompts_page(PromptListFilters {
            task_id: query.task_id,
            run_id: query.run_id,
            status: query.status,
            limit: query.limit,
            offset: query.offset,
        })
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(page.items))
}

pub(super) async fn list_prompts_page(
    State(state): State<AppState>,
    Query(query): Query<PromptListQuery>,
) -> Result<Json<PaginatedResponse<UiPromptRecord>>, ApiError> {
    let page = state
        .ui_prompt_service
        .list_prompts_page(PromptListFilters {
            task_id: query.task_id,
            run_id: query.run_id,
            status: query.status,
            limit: query.limit,
            offset: query.offset,
        })
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(page))
}

pub(super) async fn list_prompt_task_counts(
    State(state): State<AppState>,
    Query(query): Query<PromptTaskCountQuery>,
) -> Result<Json<Vec<UiPromptTaskCountRecord>>, ApiError> {
    let counts = state
        .ui_prompt_service
        .list_prompt_task_counts(query.status)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(counts))
}

pub(super) async fn get_prompt(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<UiPromptRecord>, ApiError> {
    state
        .ui_prompt_service
        .get_prompt(&id)
        .await
        .map_err(ApiError::bad_request)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("提示不存在: {id}")))
}

pub(super) async fn submit_prompt(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(input): Json<SubmitUiPromptRequest>,
) -> Result<Json<UiPromptRecord>, ApiError> {
    let prompt = state
        .ui_prompt_service
        .submit_prompt(&id, input)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("提示不存在: {id}")))?;
    Ok(Json(prompt))
}

pub(super) async fn cancel_prompt(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(input): Json<CancelUiPromptRequest>,
) -> Result<Json<UiPromptRecord>, ApiError> {
    let prompt = state
        .ui_prompt_service
        .cancel_prompt(&id, input)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("提示不存在: {id}")))?;
    Ok(Json(prompt))
}

pub(super) async fn list_run_prompts(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Query(query): Query<PromptListQuery>,
) -> Result<Json<Vec<UiPromptRecord>>, ApiError> {
    let page = state
        .ui_prompt_service
        .list_prompts_page(PromptListFilters {
            task_id: query.task_id,
            run_id: Some(id),
            status: query.status,
            limit: query.limit,
            offset: query.offset,
        })
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(page.items))
}
