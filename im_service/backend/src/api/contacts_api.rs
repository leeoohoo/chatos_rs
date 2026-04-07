use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::{json, Value};

use crate::models::CreateImContactRequest;
use crate::repositories::contacts;

use super::shared::{ensure_contact_access, require_auth};
use super::SharedState;

pub(super) async fn list_contacts(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };

    match contacts::list_contacts_by_owner(&state.pool, auth.user_id.as_str(), 200).await {
        Ok(items) => (StatusCode::OK, Json(json!(items))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list contacts failed", "detail": err})),
        ),
    }
}

pub(super) async fn create_contact(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(mut req): Json<CreateImContactRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    req.owner_user_id = auth.user_id.clone();

    match contacts::create_contact(&state.pool, req).await {
        Ok(contact) => (StatusCode::CREATED, Json(json!(contact))),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "create contact failed", "detail": err})),
        ),
    }
}

pub(super) async fn get_contact(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(contact_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };

    match ensure_contact_access(state.as_ref(), &auth, contact_id.as_str()).await {
        Ok(contact) => (StatusCode::OK, Json(json!(contact))),
        Err(err) => err,
    }
}
