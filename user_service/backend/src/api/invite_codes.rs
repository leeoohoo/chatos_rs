// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::{Extension, Json};
use chrono::Utc;

use crate::auth::CurrentPrincipal;
use crate::models::{
    CreateInviteCodeRequest, CreateInviteCodeResponse, InviteCodePublicRecord, InviteCodeRecord,
};
use crate::state::AppState;
use crate::store::now_rfc3339;

use super::auth::{generate_invite_code, invite_code_hash};
use super::{bad_request, internal_error, not_found, require_super_admin, ApiResult};

pub async fn list_invite_codes(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
) -> ApiResult<Vec<InviteCodePublicRecord>> {
    require_super_admin(&principal)?;
    state
        .store
        .list_invite_codes()
        .await
        .map(Json)
        .map_err(internal_error)
}

pub async fn create_invite_code(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
    Json(input): Json<CreateInviteCodeRequest>,
) -> ApiResult<CreateInviteCodeResponse> {
    require_super_admin(&principal)?;
    let code = generate_invite_code();
    let code_hash = invite_code_hash(code.as_str(), state.config.jwt_secret.as_str())
        .map_err(internal_error)?;
    let max_uses = input.max_uses.unwrap_or(1).clamp(1, 10_000);
    let expires_at_unix = input
        .expires_in_days
        .filter(|days| *days > 0)
        .map(|days| Utc::now().timestamp() + days.min(3650) * 24 * 60 * 60);
    let now = now_rfc3339();
    let record = InviteCodeRecord {
        id: uuid::Uuid::new_v4().to_string(),
        code_hash,
        label: input
            .label
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        created_by_user_id: principal.user_id.unwrap_or_default(),
        max_uses,
        used_count: 0,
        expires_at_unix,
        revoked_at: None,
        last_used_at: None,
        created_at: now.clone(),
        updated_at: now,
    };
    let invite = state
        .store
        .insert_invite_code(&record)
        .await
        .map_err(internal_error)?;
    Ok(Json(CreateInviteCodeResponse { code, invite }))
}

pub async fn revoke_invite_code(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
    Path(id): Path<String>,
) -> ApiResult<InviteCodePublicRecord> {
    require_super_admin(&principal)?;
    let mut record = state
        .store
        .find_invite_code_by_id(id.as_str())
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("invite code not found"))?;
    if record.revoked_at.is_some() {
        return Err(bad_request("invite code is already revoked"));
    }

    record.revoked_at = Some(now_rfc3339());
    record.updated_at = now_rfc3339();
    state
        .store
        .update_invite_code(&record)
        .await
        .map_err(internal_error)?;
    Ok(Json(record.into()))
}
