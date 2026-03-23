use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::{json, Value};

use crate::models::Contact;

use super::super::{ensure_contact_access, require_auth, SharedState};

pub(super) async fn resolve_contact(
    state: &SharedState,
    headers: &HeaderMap,
    contact_id: &str,
) -> Result<Contact, (StatusCode, Json<Value>)> {
    let auth = require_auth(state, headers)?;
    ensure_contact_access(state.as_ref(), &auth, contact_id).await
}

pub(super) fn internal_error(message: &str, detail: String) -> (StatusCode, Json<Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": message, "detail": detail})),
    )
}
