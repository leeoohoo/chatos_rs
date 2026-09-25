// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
    Extension(current_user): Extension<CurrentUser>,
    Query(query): Query<RunListQuery>,
) -> Result<Json<Vec<TaskRunRecord>>, ApiError> {
    get_task_for_user(&state, &id, &current_user)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {id}")))?;
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
    Ok(Json(redact_workspace_paths(&state, runs)?))
}

pub(in crate::api) async fn list_runs(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Query(query): Query<RunListQuery>,
) -> Result<Json<Vec<TaskRunRecord>>, ApiError> {
    let runs = list_runs_for_user(&state, &current_user, query.into_filters()).await?;
    Ok(Json(redact_workspace_paths(&state, runs)?))
}

pub(in crate::api) async fn list_runs_page(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Query(query): Query<RunListQuery>,
) -> Result<Json<PaginatedResponse<TaskRunRecord>>, ApiError> {
    let filters = query.into_filters();
    ensure_run_list_task_access(&state, &current_user, &filters).await?;
    let owner_scope = run_owner_scope(&current_user)?;
    let page = state
        .run_service
        .list_runs_page_scoped(filters, owner_scope.as_deref())
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(redact_workspace_paths(&state, page)?))
}

pub(in crate::api) async fn list_run_index(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Query(query): Query<RunListQuery>,
) -> Result<Json<Vec<RunSummaryRecord>>, ApiError> {
    let runs = list_run_summaries_for_user(&state, &current_user, query.into_filters()).await?;
    Ok(Json(runs))
}

pub(in crate::api) async fn list_run_summaries(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Query(query): Query<RunSummaryQuery>,
) -> Result<Json<Vec<RunSummaryRecord>>, ApiError> {
    let has_ids = query.ids.is_some();
    let summaries = if let Some(ids) = query.ids {
        state
            .run_service
            .get_run_summaries_by_ids(parse_csv_ids(&ids))
            .await
            .map(|items| filter_run_summaries(items, None))
            .map_err(ApiError::bad_request)?
    } else {
        list_run_summaries_for_user(
            &state,
            &current_user,
            RunListFilters {
                task_id: query.task_id,
                status: query.status,
                model_config_id: query.model_config_id,
                keyword: query.keyword,
                limit: query.limit,
                offset: None,
            },
        )
        .await?
    };
    if current_user.is_admin() || !has_ids {
        return Ok(Json(summaries));
    }
    let allowed_task_ids = visible_task_ids_for_user(&state, &current_user).await?;
    Ok(Json(filter_run_summaries(
        summaries,
        allowed_task_ids.as_ref(),
    )))
}

impl RunListQuery {
    fn into_filters(self) -> RunListFilters {
        RunListFilters {
            task_id: self.task_id,
            status: self.status,
            model_config_id: self.model_config_id,
            keyword: self.keyword,
            limit: self.limit,
            offset: self.offset,
        }
    }
}

async fn list_runs_for_user(
    state: &AppState,
    current_user: &CurrentUser,
    filters: RunListFilters,
) -> Result<Vec<TaskRunRecord>, ApiError> {
    ensure_run_list_task_access(state, current_user, &filters).await?;
    let owner_scope = run_owner_scope(current_user)?;
    state
        .run_service
        .list_runs_filtered_scoped(filters, owner_scope.as_deref())
        .await
        .map_err(ApiError::bad_request)
}

async fn list_run_summaries_for_user(
    state: &AppState,
    current_user: &CurrentUser,
    filters: RunListFilters,
) -> Result<Vec<RunSummaryRecord>, ApiError> {
    ensure_run_list_task_access(state, current_user, &filters).await?;
    let owner_scope = run_owner_scope(current_user)?;
    state
        .run_service
        .run_index_scoped(filters, owner_scope.as_deref())
        .await
        .map_err(ApiError::bad_request)
}

async fn ensure_run_list_task_access(
    state: &AppState,
    current_user: &CurrentUser,
    filters: &RunListFilters,
) -> Result<(), ApiError> {
    if current_user.is_admin() {
        return Ok(());
    }
    if let Some(task_id) = filters.task_id.as_deref() {
        get_task_for_user(state, task_id, current_user)
            .await?
            .ok_or_else(|| ApiError::not_found(format!("任务不存在: {task_id}")))?;
    }
    Ok(())
}

fn run_owner_scope(current_user: &CurrentUser) -> Result<Option<String>, ApiError> {
    if current_user.is_admin() {
        Ok(None)
    } else {
        effective_owner_user_id(current_user).map(Some)
    }
}

fn filter_run_summaries(
    runs: Vec<RunSummaryRecord>,
    allowed_task_ids: Option<&HashSet<String>>,
) -> Vec<RunSummaryRecord> {
    match allowed_task_ids {
        Some(task_ids) => runs
            .into_iter()
            .filter(|run| task_ids.contains(run.task_id.as_str()))
            .collect(),
        None => runs,
    }
}
