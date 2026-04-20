use crate::{
    domain::query::{QueryCancelResponse, QueryExecuteRequest, QueryExecuteResponse},
    error::AppResult,
    state::AppState,
};
use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct QueryPath {
    pub id: String,
}

pub async fn execute_query(
    State(state): State<AppState>,
    Json(request): Json<QueryExecuteRequest>,
) -> AppResult<Json<QueryExecuteResponse>> {
    let result = state.query_service.execute(request).await?;
    Ok(Json(result))
}

pub async fn cancel_query(
    State(state): State<AppState>,
    Path(path): Path<QueryPath>,
) -> AppResult<Json<QueryCancelResponse>> {
    let result = state.query_service.cancel(&path.id).await?;
    Ok(Json(result))
}
