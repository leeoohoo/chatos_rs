use crate::{
    domain::datasource::{
        DataSource, DataSourceCreateRequest, DataSourceHealthResponse, DataSourceListResponse,
        DataSourceMutationResponse, DataSourceUpdateRequest, DatabaseListResponse,
        DatabaseSummaryResponse,
    },
    domain::metadata::ObjectStatsResponse,
    error::AppResult,
    state::AppState,
};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct DataSourcePath {
    pub id: String,
}

#[derive(Deserialize)]
pub struct DatabaseStatsPath {
    pub id: String,
    pub database: String,
}

#[derive(Deserialize)]
pub struct DatabaseListQuery {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
    pub keyword: Option<String>,
}

pub async fn create_datasource(
    State(state): State<AppState>,
    Json(request): Json<DataSourceCreateRequest>,
) -> AppResult<Json<DataSourceMutationResponse>> {
    let result = state.datasource_service.create(request).await?;
    Ok(Json(result))
}

pub async fn list_datasources(
    State(state): State<AppState>,
) -> AppResult<Json<DataSourceListResponse>> {
    let result = state.datasource_service.list().await?;
    Ok(Json(result))
}

pub async fn get_datasource(
    State(state): State<AppState>,
    Path(path): Path<DataSourcePath>,
) -> AppResult<Json<DataSource>> {
    let result = state.datasource_service.detail(&path.id).await?;
    Ok(Json(result))
}

pub async fn update_datasource(
    State(state): State<AppState>,
    Path(path): Path<DataSourcePath>,
    Json(request): Json<DataSourceUpdateRequest>,
) -> AppResult<Json<DataSourceMutationResponse>> {
    let result = state.datasource_service.update(&path.id, request).await?;
    Ok(Json(result))
}

pub async fn delete_datasource(
    State(state): State<AppState>,
    Path(path): Path<DataSourcePath>,
) -> AppResult<Json<DataSourceMutationResponse>> {
    let result = state.datasource_service.delete(&path.id).await?;
    Ok(Json(result))
}

pub async fn test_datasource(
    State(state): State<AppState>,
    Path(path): Path<DataSourcePath>,
) -> AppResult<Json<crate::domain::datasource::ConnectionTestResult>> {
    let result = state.datasource_service.test_connection(&path.id).await?;
    Ok(Json(result))
}

pub async fn datasource_health(
    State(state): State<AppState>,
    Path(path): Path<DataSourcePath>,
) -> AppResult<Json<DataSourceHealthResponse>> {
    let result = state.datasource_service.health(&path.id).await?;
    Ok(Json(result))
}

pub async fn database_summary(
    State(state): State<AppState>,
    Path(path): Path<DataSourcePath>,
) -> AppResult<Json<DatabaseSummaryResponse>> {
    let result = state.datasource_service.database_summary(&path.id).await?;
    Ok(Json(result))
}

pub async fn list_databases(
    State(state): State<AppState>,
    Path(path): Path<DataSourcePath>,
    Query(query): Query<DatabaseListQuery>,
) -> AppResult<Json<DatabaseListResponse>> {
    let result = state
        .datasource_service
        .list_databases(
            &path.id,
            query.keyword.as_deref(),
            query.page.unwrap_or(1),
            query.page_size.unwrap_or(50),
        )
        .await?;
    Ok(Json(result))
}

pub async fn object_stats(
    State(state): State<AppState>,
    Path(path): Path<DatabaseStatsPath>,
) -> AppResult<Json<ObjectStatsResponse>> {
    let result = state
        .datasource_service
        .object_stats(&path.id, &path.database)
        .await?;
    Ok(Json(result))
}
