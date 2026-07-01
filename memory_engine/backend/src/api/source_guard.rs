// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::StatusCode;

use crate::db::Db;
use crate::repositories::sources;

fn internal_error(message: String) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, message)
}

pub(crate) async fn ensure_write_source_allowed(
    db: &Db,
    source_id: &str,
) -> Result<(), (StatusCode, String)> {
    let normalized = source_id.trim();
    if normalized.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "source_id is required".to_string()));
    }
    if sources::is_retired_source_id(normalized) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("source_id {normalized} is retired"),
        ));
    }
    let registered = sources::is_source_active(db, normalized)
        .await
        .map_err(internal_error)?;
    if !registered {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("source_id {normalized} is not registered or active"),
        ));
    }
    Ok(())
}

pub(crate) async fn ensure_optional_write_source_allowed(
    db: &Db,
    source_id: Option<&str>,
) -> Result<(), (StatusCode, String)> {
    if let Some(source_id) = source_id {
        ensure_write_source_allowed(db, source_id).await?;
    }
    Ok(())
}

pub(crate) fn ensure_source_registration_allowed(
    source_id: &str,
) -> Result<(), (StatusCode, String)> {
    let normalized = source_id.trim();
    if normalized.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "source_id is required".to_string()));
    }
    if sources::is_retired_source_id(normalized) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("source_id {normalized} is retired"),
        ));
    }
    Ok(())
}
