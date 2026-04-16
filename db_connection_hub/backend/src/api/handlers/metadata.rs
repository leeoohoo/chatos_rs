use crate::{
    domain::metadata::{
        MetadataNodesQuery, MetadataNodesResponse, ObjectDetailQuery, ObjectDetailResponse,
    },
    error::AppResult,
    state::AppState,
};
use axum::{
    extract::{Query, State},
    Json,
};

pub async fn list_nodes(
    State(state): State<AppState>,
    Query(query): Query<MetadataNodesQuery>,
) -> AppResult<Json<MetadataNodesResponse>> {
    let result = state
        .metadata_service
        .list_nodes(
            &query.datasource_id,
            query.parent_id.as_deref(),
            query.page.unwrap_or(1),
            query.page_size.unwrap_or(100),
        )
        .await?;

    Ok(Json(result))
}

pub async fn object_detail(
    State(state): State<AppState>,
    Query(query): Query<ObjectDetailQuery>,
) -> AppResult<Json<ObjectDetailResponse>> {
    let result = state
        .metadata_service
        .object_detail(&query.datasource_id, &query.node_id)
        .await?;

    Ok(Json(result))
}
