// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::{Extension, Json};
use chrono::Utc;
use rand::{Rng, RngCore};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::auth::{
    encode_user_token, hash_password, normalize_display_name, normalize_username, verify_password,
    CurrentPrincipal,
};
use crate::email::send_registration_code;
use crate::integrations::{
    ensure_harness_user_public_register_on_login, provision_harness_user_public_register,
};
use crate::models::{
    CurrentUserResponse, ExchangeLocalConnectorTicketRequest, IssueLocalConnectorTicketResponse,
    LocalConnectorAuthTicketRecord, LoginRequest, LoginResponse, RegisterRequest,
    RegistrationEmailCodeRecord, SendRegisterEmailCodeRequest, SendRegisterEmailCodeResponse,
    TokenVerifyResponse, UserRecord, VerifiedPrincipal, USER_ROLE_USER,
};
use crate::state::AppState;
use crate::store::now_rfc3339;

use super::{bad_request, internal_error, not_found, ApiResult, ApiStatusResult};

const LOCAL_CONNECTOR_TICKET_AUDIENCE: &str = "local_connector_client";
const LOCAL_CONNECTOR_TICKET_SCOPE: &str = "local_connector_pair";
const LOCAL_CONNECTOR_TICKET_TTL_SECONDS: i64 = 60;

pub async fn login(
    State(state): State<AppState>,
    Json(input): Json<LoginRequest>,
) -> ApiResult<LoginResponse> {
    let username = normalize_username(input.username.as_str()).map_err(bad_request)?;
    if input.password.trim().is_empty() {
        return Err(bad_request("password is required"));
    }

    let Some(user) = state
        .store
        .find_user_by_username(username.as_str())
        .await
        .map_err(internal_error)?
    else {
        return Err(unauthorized("invalid username or password"));
    };
    if !user.enabled {
        return Err(unauthorized("account has been disabled"));
    }
    if !verify_password(input.password.as_str(), user.password_hash.as_str()) {
        return Err(unauthorized("invalid username or password"));
    }

    state
        .store
        .touch_user_last_login(user.id.as_str())
        .await
        .map_err(internal_error)?;
    let _ =
        ensure_harness_user_public_register_on_login(&state, &user, input.password.as_str()).await;
    let token = encode_user_token(&state.config, &user).map_err(internal_error)?;

    Ok(Json(LoginResponse {
        token,
        user: current_auth_user(
            user.id,
            user.username,
            user.display_name,
            user.role,
            String::new(),
            0,
        ),
    }))
}

pub async fn send_register_email_code(
    State(state): State<AppState>,
    Json(input): Json<SendRegisterEmailCodeRequest>,
) -> ApiResult<SendRegisterEmailCodeResponse> {
    let email = normalize_email(input.email.as_str()).map_err(bad_request)?;
    if state
        .store
        .find_user_by_username(email.as_str())
        .await
        .map_err(internal_error)?
        .is_some()
    {
        return Err(bad_request("email already registered"));
    }
    let invite_code_hash =
        invite_code_hash(input.invite_code.as_str(), state.config.jwt_secret.as_str())
            .map_err(bad_request)?;
    let invite = state
        .store
        .find_invite_code_by_hash(invite_code_hash.as_str())
        .await
        .map_err(internal_error)?
        .ok_or_else(|| bad_request("invite code is invalid"))?;
    validate_invite_code(&invite).map_err(bad_request)?;

    let now_unix = Utc::now().timestamp();
    let existing = state
        .store
        .find_registration_email_code(email.as_str())
        .await
        .map_err(internal_error)?;
    if let Some(existing) = existing.as_ref() {
        if existing.consumed_at.is_none() && existing.resend_after_unix > now_unix {
            return Err(bad_request(
                "verification code was sent recently; retry later",
            ));
        }
    }
    let (window_start_unix, send_count) = next_send_window(
        existing.as_ref(),
        now_unix,
        state.config.registration_code_hourly_limit,
    )
    .map_err(bad_request)?;
    let code = format!("{:06}", rand::thread_rng().gen_range(0..1_000_000));
    let record = RegistrationEmailCodeRecord {
        email: email.clone(),
        code_hash: registration_code_hash(
            email.as_str(),
            code.as_str(),
            state.config.jwt_secret.as_str(),
        ),
        invite_code_hash,
        expires_at_unix: now_unix + state.config.registration_code_ttl_seconds,
        resend_after_unix: now_unix + state.config.registration_code_resend_seconds,
        attempts: 0,
        send_count,
        window_start_unix,
        consumed_at: None,
        created_at: existing
            .map(|value| value.created_at)
            .unwrap_or_else(now_rfc3339),
        updated_at: now_rfc3339(),
    };
    send_registration_code(&state.config, email.as_str(), code.as_str())
        .await
        .map_err(internal_error)?;
    state
        .store
        .save_registration_email_code(&record)
        .await
        .map_err(internal_error)?;
    Ok(Json(SendRegisterEmailCodeResponse {
        ok: true,
        expires_in_seconds: state.config.registration_code_ttl_seconds,
        resend_after_seconds: state.config.registration_code_resend_seconds,
    }))
}

pub async fn register(
    State(state): State<AppState>,
    Json(input): Json<RegisterRequest>,
) -> ApiResult<LoginResponse> {
    let email = normalize_register_email(&input).map_err(bad_request)?;
    if input.password.trim().is_empty() {
        return Err(bad_request("password is required"));
    }
    if state
        .store
        .find_user_by_username(email.as_str())
        .await
        .map_err(internal_error)?
        .is_some()
    {
        return Err(bad_request("email already registered"));
    }
    let invite_code = input
        .invite_code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| bad_request("invite_code is required"))?;
    let verification_code = input
        .verification_code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| bad_request("verification_code is required"))?;
    let invite_hash =
        invite_code_hash(invite_code, state.config.jwt_secret.as_str()).map_err(bad_request)?;
    let invite = state
        .store
        .find_invite_code_by_hash(invite_hash.as_str())
        .await
        .map_err(internal_error)?
        .ok_or_else(|| bad_request("invite code is invalid"))?;
    validate_invite_code(&invite).map_err(bad_request)?;
    verify_registration_email_code(
        &state,
        email.as_str(),
        verification_code,
        invite_hash.as_str(),
    )
    .await?;
    let invite_used_at = now_rfc3339();
    let invite_consumed = state
        .store
        .consume_invite_code(
            invite.id.as_str(),
            Utc::now().timestamp(),
            invite_used_at.as_str(),
        )
        .await
        .map_err(internal_error)?;
    if !invite_consumed {
        return Err(bad_request("invite code is invalid or no longer available"));
    }

    let now = now_rfc3339();
    let user = UserRecord {
        id: uuid::Uuid::new_v4().to_string(),
        username: email.clone(),
        display_name: normalize_display_name(input.display_name.as_deref(), &email),
        password_hash: hash_password(input.password.as_str()).map_err(bad_request)?,
        role: USER_ROLE_USER.to_string(),
        enabled: true,
        created_at: now.clone(),
        updated_at: now,
        last_login_at: None,
    };
    state
        .store
        .insert_user_record(&user)
        .await
        .map_err(internal_error)?;
    state
        .store
        .mark_registration_email_code_consumed(email.as_str())
        .await
        .map_err(internal_error)?;
    let _ = provision_harness_user_public_register(&state, &user, input.password.as_str()).await;
    state
        .store
        .touch_user_last_login(user.id.as_str())
        .await
        .map_err(internal_error)?;
    let token = encode_user_token(&state.config, &user).map_err(internal_error)?;

    Ok(Json(LoginResponse {
        token,
        user: current_auth_user(
            user.id,
            user.username,
            user.display_name,
            user.role,
            String::new(),
            0,
        ),
    }))
}

pub async fn issue_local_connector_ticket(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
) -> ApiResult<IssueLocalConnectorTicketResponse> {
    let user_id = principal
        .user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| bad_request("human user is required"))?;
    let Some(user) = state
        .store
        .find_user_by_id(user_id)
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found("current user not found"));
    };
    if !user.enabled {
        return Err(bad_request("account has been disabled"));
    }

    let ticket = generate_local_connector_ticket();
    let now_unix = Utc::now().timestamp();
    let now = now_rfc3339();
    let record = LocalConnectorAuthTicketRecord {
        id: Uuid::new_v4().to_string(),
        ticket_hash: local_connector_ticket_hash(ticket.as_str(), state.config.jwt_secret.as_str()),
        user_id: user.id,
        audience: LOCAL_CONNECTOR_TICKET_AUDIENCE.to_string(),
        scope: LOCAL_CONNECTOR_TICKET_SCOPE.to_string(),
        expires_at_unix: now_unix + LOCAL_CONNECTOR_TICKET_TTL_SECONDS,
        consumed_at: None,
        created_at: now.clone(),
        updated_at: now,
    };
    state
        .store
        .insert_local_connector_auth_ticket(&record)
        .await
        .map_err(internal_error)?;
    Ok(Json(IssueLocalConnectorTicketResponse {
        ticket,
        expires_in_seconds: LOCAL_CONNECTOR_TICKET_TTL_SECONDS,
    }))
}

pub async fn exchange_local_connector_ticket(
    State(state): State<AppState>,
    Json(input): Json<ExchangeLocalConnectorTicketRequest>,
) -> ApiResult<LoginResponse> {
    let ticket = input.ticket.trim();
    if ticket.is_empty() || ticket.len() > 512 {
        return Err(bad_request("local connector ticket is invalid"));
    }
    let ticket_hash = local_connector_ticket_hash(ticket, state.config.jwt_secret.as_str());
    let now_unix = Utc::now().timestamp();
    let now = now_rfc3339();
    let record = state
        .store
        .consume_local_connector_auth_ticket(ticket_hash.as_str(), now_unix, now.as_str())
        .await
        .map_err(internal_error)?
        .ok_or_else(|| bad_request("local connector ticket is invalid or expired"))?;
    if record.audience != LOCAL_CONNECTOR_TICKET_AUDIENCE
        || record.scope != LOCAL_CONNECTOR_TICKET_SCOPE
    {
        return Err(bad_request("local connector ticket is invalid"));
    }
    let Some(user) = state
        .store
        .find_user_by_id(record.user_id.as_str())
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found("current user not found"));
    };
    if !user.enabled {
        return Err(bad_request("account has been disabled"));
    }

    state
        .store
        .touch_user_last_login(user.id.as_str())
        .await
        .map_err(internal_error)?;
    let token = encode_user_token(&state.config, &user).map_err(internal_error)?;
    Ok(Json(LoginResponse {
        token,
        user: current_auth_user(
            user.id,
            user.username,
            user.display_name,
            user.role,
            String::new(),
            0,
        ),
    }))
}

fn normalize_register_email(input: &RegisterRequest) -> Result<String, String> {
    input
        .email
        .as_deref()
        .or(input.username.as_deref())
        .ok_or_else(|| "email is required".to_string())
        .and_then(normalize_email)
}

fn normalize_email(value: &str) -> Result<String, String> {
    let email = normalize_username(value)?;
    let (local, domain) = email
        .split_once('@')
        .ok_or_else(|| "email format is invalid".to_string())?;
    if local.is_empty()
        || domain.is_empty()
        || !domain.contains('.')
        || email.len() > 254
        || email.contains(char::is_whitespace)
    {
        return Err("email format is invalid".to_string());
    }
    Ok(email)
}

fn next_send_window(
    existing: Option<&RegistrationEmailCodeRecord>,
    now_unix: i64,
    hourly_limit: i64,
) -> Result<(i64, i64), String> {
    let Some(existing) = existing else {
        return Ok((now_unix, 1));
    };
    if now_unix - existing.window_start_unix >= 3600 {
        return Ok((now_unix, 1));
    }
    if existing.send_count >= hourly_limit {
        return Err("too many verification emails; retry later".to_string());
    }
    Ok((existing.window_start_unix, existing.send_count + 1))
}

async fn verify_registration_email_code(
    state: &AppState,
    email: &str,
    code: &str,
    invite_code_hash: &str,
) -> ApiStatusResult {
    let mut record = state
        .store
        .find_registration_email_code(email)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| bad_request("verification code is invalid or expired"))?;
    let now_unix = Utc::now().timestamp();
    if record.consumed_at.is_some()
        || record.expires_at_unix < now_unix
        || record.invite_code_hash != invite_code_hash
    {
        return Err(bad_request("verification code is invalid or expired"));
    }
    if record.attempts >= state.config.registration_code_max_attempts {
        return Err(bad_request("verification code is invalid or expired"));
    }
    let expected = registration_code_hash(email, code, state.config.jwt_secret.as_str());
    if record.code_hash != expected {
        record.attempts += 1;
        record.updated_at = now_rfc3339();
        state
            .store
            .save_registration_email_code(&record)
            .await
            .map_err(internal_error)?;
        return Err(bad_request("verification code is invalid or expired"));
    }
    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub(crate) fn invite_code_hash(code: &str, secret: &str) -> Result<String, String> {
    let code = normalize_invite_code(code)?;
    Ok(hash_text(format!("invite:{secret}:{code}").as_str()))
}

pub(crate) fn normalize_invite_code(code: &str) -> Result<String, String> {
    let code = code.trim().to_ascii_uppercase();
    if code.len() < 8 || code.len() > 64 || code.contains(char::is_whitespace) {
        return Err("invite code is invalid".to_string());
    }
    Ok(code)
}

pub(crate) fn validate_invite_code(invite: &crate::models::InviteCodeRecord) -> Result<(), String> {
    if invite.revoked_at.is_some() {
        return Err("invite code is revoked".to_string());
    }
    if invite.used_count >= invite.max_uses {
        return Err("invite code has been used".to_string());
    }
    if invite
        .expires_at_unix
        .is_some_and(|expires_at| expires_at < Utc::now().timestamp())
    {
        return Err("invite code has expired".to_string());
    }
    Ok(())
}

fn registration_code_hash(email: &str, code: &str, secret: &str) -> String {
    hash_text(format!("register-code:{secret}:{email}:{code}").as_str())
}

fn hash_text(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn local_connector_ticket_hash(ticket: &str, secret: &str) -> String {
    hash_text(format!("local-connector-ticket:{secret}:{ticket}").as_str())
}

fn generate_local_connector_ticket() -> String {
    let mut bytes = [0_u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

pub(crate) fn generate_invite_code() -> String {
    let raw = Uuid::new_v4().simple().to_string().to_ascii_uppercase();
    format!("CHATOS-{}-{}-{}", &raw[0..4], &raw[4..8], &raw[8..12])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registration_code_record(
        window_start_unix: i64,
        send_count: i64,
    ) -> RegistrationEmailCodeRecord {
        RegistrationEmailCodeRecord {
            email: "user@example.com".to_string(),
            code_hash: "hash".to_string(),
            invite_code_hash: "invite".to_string(),
            expires_at_unix: window_start_unix + 600,
            resend_after_unix: window_start_unix + 60,
            attempts: 0,
            send_count,
            window_start_unix,
            consumed_at: None,
            created_at: now_rfc3339(),
            updated_at: now_rfc3339(),
        }
    }

    #[test]
    fn invite_code_normalization_trims_and_uppercases() {
        let normalized = normalize_invite_code("  chatos-abcd-ef12  ").unwrap();
        assert_eq!(normalized, "CHATOS-ABCD-EF12");
    }

    #[test]
    fn invite_code_normalization_rejects_whitespace_inside_code() {
        assert!(normalize_invite_code("CHATOS ABCD EF12").is_err());
    }

    #[test]
    fn next_send_window_enforces_hourly_limit() {
        let record = registration_code_record(1_000, 5);
        let err = next_send_window(Some(&record), 1_100, 5).unwrap_err();
        assert_eq!(err, "too many verification emails; retry later");
    }

    #[test]
    fn next_send_window_resets_after_hour_window() {
        let record = registration_code_record(1_000, 5);
        let next = next_send_window(Some(&record), 4_700, 5).unwrap();
        assert_eq!(next, (4_700, 1));
    }
}

pub async fn me(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
) -> ApiResult<CurrentUserResponse> {
    let Some(user_id) = principal.user_id.as_deref() else {
        return Err(not_found("current user not found"));
    };
    let Some(user) = state
        .store
        .find_user_by_id(user_id)
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found("current user not found"));
    };
    Ok(Json(CurrentUserResponse {
        user: current_auth_user(
            user.id,
            user.username,
            user.display_name,
            user.role,
            principal.jti,
            principal.exp,
        ),
    }))
}

pub async fn verify(
    Extension(principal): Extension<CurrentPrincipal>,
) -> ApiResult<TokenVerifyResponse> {
    Ok(Json(TokenVerifyResponse {
        principal: VerifiedPrincipal {
            sub: principal.sub,
            jti: principal.jti,
            exp: principal.exp,
            principal_type: principal.principal_type,
            user_id: principal.user_id,
            username: principal.username,
            display_name: principal.display_name,
            role: principal.role,
            agent_account_id: principal.agent_account_id,
            owner_user_id: principal.owner_user_id,
            owner_username: principal.owner_username,
            owner_display_name: principal.owner_display_name,
            scopes: principal.scopes,
        },
    }))
}

pub async fn logout(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
) -> ApiStatusResult {
    state
        .store
        .revoke_token(
            principal.jti.as_str(),
            principal.sub.as_str(),
            principal.exp as i64,
        )
        .await
        .map_err(internal_error)?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

fn unauthorized(message: &str) -> (axum::http::StatusCode, Json<serde_json::Value>) {
    (
        axum::http::StatusCode::UNAUTHORIZED,
        Json(json!({ "error": message })),
    )
}

fn current_auth_user(
    id: String,
    username: String,
    display_name: String,
    role: String,
    jti: String,
    exp: usize,
) -> crate::models::AuthUser {
    CurrentPrincipal {
        sub: format!("user:{id}"),
        jti,
        exp,
        principal_type: crate::models::PRINCIPAL_TYPE_HUMAN_USER.to_string(),
        user_id: Some(id),
        username: Some(username),
        display_name: Some(display_name),
        role: Some(role),
        agent_account_id: None,
        owner_user_id: None,
        owner_username: None,
        owner_display_name: None,
        scopes: vec!["user_service".to_string()],
    }
    .auth_user()
}
