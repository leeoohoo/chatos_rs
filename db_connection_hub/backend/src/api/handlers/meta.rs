use crate::{
    domain::datasource::{ConnectionTestResult, DataSourceCreateRequest, DatabaseListResponse},
    error::AppResult,
    state::AppState,
};
use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct DiscoverDatabasesQuery {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
    pub keyword: Option<String>,
}

pub async fn list_db_types(
    State(state): State<AppState>,
) -> AppResult<Json<crate::domain::meta::DbTypeListResponse>> {
    Ok(Json(state.meta_service.list_db_types()))
}

pub async fn discover_databases(
    State(state): State<AppState>,
    Query(query): Query<DiscoverDatabasesQuery>,
    Json(request): Json<DataSourceCreateRequest>,
) -> AppResult<Json<DatabaseListResponse>> {
    let result = state
        .datasource_service
        .discover_databases(
            request,
            query.keyword.as_deref(),
            query.page.unwrap_or(1),
            query.page_size.unwrap_or(200),
        )
        .await?;
    Ok(Json(result))
}

pub async fn test_connection(
    State(state): State<AppState>,
    Json(request): Json<DataSourceCreateRequest>,
) -> AppResult<Json<ConnectionTestResult>> {
    let result = state.datasource_service.test_connection_preview(request).await?;
    Ok(Json(result))
}
