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
    Extension(current_user): Extension<CurrentUser>,
    Query(query): Query<PromptListQuery>,
) -> Result<Json<Vec<UiPromptRecord>>, ApiError> {
    let page = list_prompts_page_for_user(&state, &current_user, query.into_filters()).await?;
    Ok(Json(page.items))
}

pub(super) async fn list_prompts_page(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Query(query): Query<PromptListQuery>,
) -> Result<Json<PaginatedResponse<UiPromptRecord>>, ApiError> {
    let page = list_prompts_page_for_user(&state, &current_user, query.into_filters()).await?;
    Ok(Json(page))
}

pub(super) async fn list_prompt_task_counts(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Query(query): Query<PromptTaskCountQuery>,
) -> Result<Json<Vec<UiPromptTaskCountRecord>>, ApiError> {
    let counts = state
        .ui_prompt_service
        .list_prompt_task_counts(query.status)
        .await
        .map_err(ApiError::bad_request)?;
    if current_user.is_admin() {
        return Ok(Json(counts));
    }
    let allowed_task_ids = visible_task_ids_for_user(&state, &current_user).await?;
    let counts = match allowed_task_ids.as_ref() {
        Some(task_ids) => counts
            .into_iter()
            .filter(|item| task_ids.contains(item.task_id.as_str()))
            .collect(),
        None => counts,
    };
    Ok(Json(counts))
}

pub(super) async fn get_prompt(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<UiPromptRecord>, ApiError> {
    let prompt = state
        .ui_prompt_service
        .get_prompt(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("提示不存在: {id}")))?;
    ensure_prompt_access(&state, &prompt, &current_user).await?;
    Ok(Json(prompt))
}

pub(super) async fn submit_prompt(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<SubmitUiPromptRequest>,
) -> Result<Json<UiPromptRecord>, ApiError> {
    let existing = state
        .ui_prompt_service
        .get_prompt(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("提示不存在: {id}")))?;
    ensure_prompt_access(&state, &existing, &current_user).await?;
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
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<CancelUiPromptRequest>,
) -> Result<Json<UiPromptRecord>, ApiError> {
    let existing = state
        .ui_prompt_service
        .get_prompt(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("提示不存在: {id}")))?;
    ensure_prompt_access(&state, &existing, &current_user).await?;
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
    Extension(current_user): Extension<CurrentUser>,
    Query(query): Query<PromptListQuery>,
) -> Result<Json<Vec<UiPromptRecord>>, ApiError> {
    let run = state
        .run_service
        .get_run(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("运行记录不存在: {id}")))?;
    ensure_run_access(&state, &run, &current_user).await?;
    let page = list_prompts_page_for_user(
        &state,
        &current_user,
        PromptListFilters {
            task_id: query.task_id,
            run_id: Some(id),
            status: query.status,
            limit: query.limit,
            offset: query.offset,
        },
    )
    .await?;
    Ok(Json(page.items))
}

impl PromptListQuery {
    fn into_filters(self) -> PromptListFilters {
        PromptListFilters {
            task_id: self.task_id,
            run_id: self.run_id,
            status: self.status,
            limit: self.limit,
            offset: self.offset,
        }
    }
}

async fn list_prompts_page_for_user(
    state: &AppState,
    current_user: &CurrentUser,
    filters: PromptListFilters,
) -> Result<PaginatedResponse<UiPromptRecord>, ApiError> {
    if current_user.is_admin() {
        return state
            .ui_prompt_service
            .list_prompts_page(filters)
            .await
            .map_err(ApiError::bad_request);
    }
    if let Some(task_id) = filters.task_id.as_deref() {
        get_task_for_user(state, task_id, current_user)
            .await?
            .ok_or_else(|| ApiError::not_found(format!("任务不存在: {task_id}")))?;
    }
    let scoped_to_accessed_run = filters.run_id.is_some();
    if let Some(run_id) = filters.run_id.as_deref() {
        let run = state
            .run_service
            .get_run(run_id)
            .await
            .map_err(ApiError::bad_request)?
            .ok_or_else(|| ApiError::not_found(format!("运行记录不存在: {run_id}")))?;
        ensure_run_access(state, &run, current_user).await?;
    }
    if scoped_to_accessed_run {
        return state
            .ui_prompt_service
            .list_prompts_page(filters)
            .await
            .map_err(ApiError::bad_request);
    }

    let offset = filters.offset.unwrap_or(0);
    let limit = filters.limit.unwrap_or(20).clamp(1, 500);
    let mut unpaged = filters;
    unpaged.offset = None;
    unpaged.limit = Some(500);
    let page = state
        .ui_prompt_service
        .list_prompts_page(unpaged)
        .await
        .map_err(ApiError::bad_request)?;
    let allowed_task_ids = visible_task_ids_for_user(state, current_user).await?;
    let mut items = filter_prompts(page.items, allowed_task_ids.as_ref());
    let total = items.len();
    if offset >= items.len() {
        items.clear();
    } else {
        items = items.into_iter().skip(offset).take(limit).collect();
    }
    Ok(PaginatedResponse {
        has_more: offset.saturating_add(items.len()) < total,
        items,
        total,
        limit,
        offset,
    })
}

async fn ensure_prompt_access(
    state: &AppState,
    prompt: &UiPromptRecord,
    current_user: &CurrentUser,
) -> Result<(), ApiError> {
    if current_user.is_admin() {
        return Ok(());
    }
    if let Some(task_id) = prompt.task_id.as_deref() {
        get_task_for_user(state, task_id, current_user)
            .await?
            .map(|_| ())
            .ok_or_else(|| ApiError::not_found(format!("任务不存在: {task_id}")))
    } else if let Some(run_id) = prompt.run_id.as_deref() {
        let run = state
            .run_service
            .get_run(run_id)
            .await
            .map_err(ApiError::bad_request)?
            .ok_or_else(|| ApiError::not_found(format!("运行记录不存在: {run_id}")))?;
        ensure_run_access(state, &run, current_user).await
    } else {
        Err(ApiError::forbidden("无权访问该提示"))
    }
}

fn filter_prompts(
    prompts: Vec<UiPromptRecord>,
    allowed_task_ids: Option<&HashSet<String>>,
) -> Vec<UiPromptRecord> {
    match allowed_task_ids {
        Some(task_ids) => prompts
            .into_iter()
            .filter(|prompt| {
                prompt
                    .task_id
                    .as_deref()
                    .is_some_and(|task_id| task_ids.contains(task_id))
            })
            .collect(),
        None => prompts,
    }
}
