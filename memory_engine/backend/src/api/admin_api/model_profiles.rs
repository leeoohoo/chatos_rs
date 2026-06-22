use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use serde_json::json;

use super::error::internal_error;
use crate::api::model_profile_auth::ModelProfileAuthContext;
use crate::models::UpsertEngineModelProfileRequest;
use crate::repositories::control_plane;
use crate::state::AppState;

#[derive(Debug, Default, serde::Deserialize)]
pub struct ModelProfileScopeQuery {
    pub owner_user_id: Option<String>,
}

pub async fn list_model_profiles(
    State(state): State<Arc<AppState>>,
    auth: ModelProfileAuthContext,
    Query(query): Query<ModelProfileScopeQuery>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let owner_scope = auth.resolve_owner_scope(query.owner_user_id.as_deref())?;
    let items = match (&auth, owner_scope.as_deref()) {
        (ModelProfileAuthContext::Operator, None) => {
            control_plane::list_model_profiles(&state.pool).await
        }
        (_, Some(owner_user_id)) => {
            control_plane::list_model_profiles_by_owner(&state.pool, owner_user_id).await
        }
        (_, None) => control_plane::list_model_profiles(&state.pool).await,
    }
    .map_err(internal_error)?;
    Ok(Json(json!({ "items": items })))
}

pub async fn get_model_profile(
    State(state): State<Arc<AppState>>,
    auth: ModelProfileAuthContext,
    Path(model_id): Path<String>,
) -> Result<Json<crate::models::EngineModelProfile>, (axum::http::StatusCode, String)> {
    let item = match auth.resolve_owner_scope(None)? {
        Some(owner_user_id) => {
            control_plane::get_model_profile_by_id_for_owner(
                &state.pool,
                model_id.as_str(),
                owner_user_id.as_str(),
            )
            .await
        }
        None => control_plane::get_model_profile_by_id(&state.pool, model_id.as_str()).await,
    }
    .map_err(internal_error)?;
    match item {
        Some(item) => Ok(Json(item)),
        None => Err((
            axum::http::StatusCode::NOT_FOUND,
            "model profile not found".to_string(),
        )),
    }
}

pub async fn create_model_profile(
    State(state): State<Arc<AppState>>,
    auth: ModelProfileAuthContext,
    Query(query): Query<ModelProfileScopeQuery>,
    Json(req): Json<UpsertEngineModelProfileRequest>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let owner_user_id = auth.resolve_owner_scope(query.owner_user_id.as_deref())?;
    control_plane::create_model_profile(
        &state.pool,
        owner_user_id.as_deref(),
        auth.owner_username_for_create().as_deref(),
        req,
    )
    .await
    .map(|item| Json(json!(item)))
    .map_err(internal_error)
}

pub async fn update_model_profile(
    State(state): State<Arc<AppState>>,
    auth: ModelProfileAuthContext,
    Path(model_id): Path<String>,
    Json(req): Json<UpsertEngineModelProfileRequest>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let owner_scope = auth.resolve_owner_scope(None)?;
    let existing = match owner_scope.as_deref() {
        Some(owner_user_id) => {
            control_plane::get_model_profile_by_id_for_owner(
                &state.pool,
                model_id.as_str(),
                owner_user_id,
            )
            .await
        }
        None => control_plane::get_model_profile_by_id(&state.pool, model_id.as_str()).await,
    }
    .map_err(internal_error)?;
    if existing.is_none() {
        return Err((
            axum::http::StatusCode::NOT_FOUND,
            "model profile not found".to_string(),
        ));
    }
    match control_plane::update_model_profile(&state.pool, model_id.as_str(), req).await {
        Ok(Some(item)) => Ok(Json(json!(item))),
        Ok(None) => Err((
            axum::http::StatusCode::NOT_FOUND,
            "model profile not found".to_string(),
        )),
        Err(err) => Err(internal_error(err)),
    }
}

pub async fn delete_model_profile(
    State(state): State<Arc<AppState>>,
    auth: ModelProfileAuthContext,
    Path(model_id): Path<String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let owner_scope = auth.resolve_owner_scope(None)?;
    let existing = match owner_scope.as_deref() {
        Some(owner_user_id) => {
            control_plane::get_model_profile_by_id_for_owner(
                &state.pool,
                model_id.as_str(),
                owner_user_id,
            )
            .await
        }
        None => control_plane::get_model_profile_by_id(&state.pool, model_id.as_str()).await,
    }
    .map_err(internal_error)?;
    if existing.is_none() {
        return Err((
            axum::http::StatusCode::NOT_FOUND,
            "model profile not found".to_string(),
        ));
    }
    match control_plane::delete_model_profile(&state.pool, model_id.as_str()).await {
        Ok(true) => Ok(Json(json!({"success": true}))),
        Ok(false) => Err((
            axum::http::StatusCode::NOT_FOUND,
            "model profile not found".to_string(),
        )),
        Err(err) => Err(internal_error(err)),
    }
}
