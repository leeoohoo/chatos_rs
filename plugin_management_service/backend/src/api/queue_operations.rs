// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub(super) struct ReplayCatalogSyncRequest {
    operation_id: String,
    marketplace_id: String,
    version: i64,
    reason: String,
}

#[derive(Debug, Serialize)]
pub(super) struct ReplayCatalogSyncResponse {
    operation_id: String,
    marketplace_id: String,
    version: i64,
    event_enqueued: bool,
    dead_letter_archived: bool,
}

pub(super) async fn replay_catalog_sync_dead_letter(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<ReplayCatalogSyncRequest>,
) -> Result<Json<ReplayCatalogSyncResponse>, ApiError> {
    ensure_super_admin(&user)?;
    let operation_id = validated_text(input.operation_id, "operation_id", 200)?;
    let marketplace_id = validated_text(input.marketplace_id, "marketplace_id", 200)?;
    let reason = input.reason.trim().to_string();
    if input.version <= 0 {
        return Err(ApiError::bad_request(
            "positive dead-letter version is required",
        ));
    }
    if !(8..=500).contains(&reason.len()) {
        return Err(ApiError::bad_request(
            "reason must contain between 8 and 500 characters",
        ));
    }

    tracing::warn!(
        operation_id = operation_id.as_str(),
        actor_user_id = user.user_id.as_str(),
        marketplace_id = marketplace_id.as_str(),
        version = input.version,
        reason = reason.as_str(),
        "Administrator requested Plugin Catalog dead-letter replay"
    );
    let Some(dead_letter_archived) = crate::catalog_sync_queue::replay_dead_lettered_marketplace(
        &state,
        marketplace_id.as_str(),
        input.version,
    )
    .await
    .map_err(ApiError::internal)?
    else {
        return Err(ApiError::bad_request(format!(
            "Plugin Catalog {marketplace_id} version {} is not an eligible dead-lettered sync",
            input.version
        )));
    };

    Ok(Json(ReplayCatalogSyncResponse {
        operation_id,
        marketplace_id,
        version: input.version,
        event_enqueued: true,
        dead_letter_archived,
    }))
}

fn validated_text(value: String, field: &str, max_len: usize) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty() || value.len() > max_len {
        return Err(ApiError::bad_request(format!(
            "{field} is required and must contain at most {max_len} characters"
        )));
    }
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::validated_text;

    #[test]
    fn replay_identity_rejects_blank_and_oversized_values() {
        assert!(validated_text("  ".to_string(), "operation_id", 10).is_err());
        assert!(validated_text("12345678901".to_string(), "operation_id", 10).is_err());
        assert_eq!(
            validated_text(" operation-1 ".to_string(), "operation_id", 20).unwrap(),
            "operation-1"
        );
    }
}
