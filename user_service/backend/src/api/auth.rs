// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::net::SocketAddr;

use axum::extract::{ConnectInfo, State};
use axum::http::HeaderMap;
use axum::{Extension, Json};
use chrono::Utc;
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
    SendRegisterEmailCodeRequest, SendRegisterEmailCodeResponse, TokenVerifyResponse, UserRecord,
    VerifiedPrincipal, USER_ROLE_USER,
};
use crate::state::AppState;
use crate::store::now_rfc3339;
use crate::store::{RegistrationEmailCodeReservationError, RegistrationTransactionError};

use super::{bad_request, internal_error, not_found, ApiResult, ApiStatusResult};

const LOCAL_CONNECTOR_TICKET_AUDIENCE: &str = "local_connector_client";
const LOCAL_CONNECTOR_TICKET_SCOPE: &str = "local_connector_pair";
const LOCAL_CONNECTOR_TICKET_TTL_SECONDS: i64 = 60;

pub async fn login(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(input): Json<LoginRequest>,
) -> ApiResult<LoginResponse> {
    let username = normalize_username(input.username.as_str()).map_err(bad_request)?;
    if input.password.trim().is_empty() {
        return Err(bad_request("password is required"));
    }

    let now_unix = Utc::now().timestamp();
    let source = crate::login_throttle::request_source(&headers, addr);
    let user = authenticate_password_user(
        &state,
        username.as_str(),
        input.password.as_str(),
        source.as_str(),
        now_unix,
    )
    .await?;
    let _ = ensure_harness_user_public_register_on_login(&state, &user).await;
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

pub(super) async fn authenticate_password_user(
    state: &AppState,
    username: &str,
    password: &str,
    source: &str,
    now_unix: i64,
) -> Result<UserRecord, (axum::http::StatusCode, Json<serde_json::Value>)> {
    if state
        .login_throttle
        .is_locked(username, Some(source), now_unix, &state.config)
        .await
        .map_err(internal_error)?
    {
        return Err(super::unauthorized("invalid username or password"));
    }

    let user = state
        .store
        .find_user_by_username(username)
        .await
        .map_err(internal_error)?;
    let valid = user
        .as_ref()
        .is_some_and(|user| user.enabled && verify_password(password, user.password_hash.as_str()));
    if !valid {
        state
            .login_throttle
            .record_failure(username, Some(source), now_unix, &state.config)
            .await
            .map_err(internal_error)?;
        return Err(super::unauthorized("invalid username or password"));
    }

    let user = user.expect("validated password user must exist");
    state
        .login_throttle
        .record_success(username, Some(source))
        .await
        .map_err(internal_error)?;
    state
        .store
        .touch_user_last_login(user.id.as_str())
        .await
        .map_err(internal_error)?;
    Ok(user)
}

pub async fn send_register_email_code(
    State(state): State<AppState>,
    Json(input): Json<SendRegisterEmailCodeRequest>,
) -> ApiResult<SendRegisterEmailCodeResponse> {
    let email = normalize_email(input.email.as_str()).map_err(bad_request)?;
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
    let code = format!("{:06}", rand::random_range(0..1_000_000));
    let reservation = state
        .store
        .reserve_registration_email_code_send(
            email.as_str(),
            registration_code_hash(
                email.as_str(),
                code.as_str(),
                state.config.jwt_secret.as_str(),
            ),
            invite_code_hash,
            now_unix,
            now_rfc3339(),
            state.config.registration_code_ttl_seconds,
            state.config.registration_code_resend_seconds,
            state.config.registration_code_hourly_limit,
        )
        .await
        .map_err(|error| match error {
            RegistrationEmailCodeReservationError::ResendTooSoon => {
                bad_request("verification code was sent recently; retry later")
            }
            RegistrationEmailCodeReservationError::HourlyLimitReached => {
                bad_request("too many verification emails; retry later")
            }
            RegistrationEmailCodeReservationError::Store(error) => internal_error(error),
        })?;
    let Some(reservation) = reservation else {
        return Ok(Json(SendRegisterEmailCodeResponse {
            ok: true,
            expires_in_seconds: state.config.registration_code_ttl_seconds,
            resend_after_seconds: state.config.registration_code_resend_seconds,
        }));
    };
    if let Err(error) = send_registration_code(&state.config, email.as_str(), code.as_str()).await {
        if let Err(restore_error) = state
            .store
            .restore_registration_email_code_reservation(&reservation)
            .await
        {
            tracing::error!(
                email = email.as_str(),
                error = restore_error.as_str(),
                "restore registration email quota after delivery failure failed"
            );
        }
        return Err(internal_error(error));
    }
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
    let now = now_rfc3339();
    let user = UserRecord {
        id: uuid::Uuid::new_v4().to_string(),
        username: email.clone(),
        display_name: normalize_display_name(input.display_name.as_deref(), &email),
        password_hash: hash_password(input.password.as_str()).map_err(bad_request)?,
        credential_version: 0,
        role: USER_ROLE_USER.to_string(),
        enabled: true,
        created_at: now.clone(),
        updated_at: now.clone(),
        last_login_at: None,
    };
    let expected_code_hash = registration_code_hash(
        email.as_str(),
        verification_code,
        state.config.jwt_secret.as_str(),
    );
    state
        .store
        .register_user_with_invite_and_email_code(
            &user,
            invite_hash.as_str(),
            expected_code_hash.as_str(),
            Utc::now().timestamp(),
            now.as_str(),
            state.config.registration_code_max_attempts,
        )
        .await
        .map_err(|error| match error {
            RegistrationTransactionError::InvalidVerificationCode => {
                bad_request("verification code is invalid or expired")
            }
            RegistrationTransactionError::InvalidInvite => {
                bad_request("invite code is invalid or no longer available")
            }
            RegistrationTransactionError::EmailAlreadyRegistered => {
                bad_request("email already registered")
            }
            RegistrationTransactionError::Store(error) => internal_error(error),
        })?;
    let _ = provision_harness_user_public_register(&state, &user).await;
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
    rand::fill(&mut bytes);
    hex::encode(bytes)
}

pub(crate) fn generate_invite_code() -> String {
    let raw = Uuid::new_v4().simple().to_string().to_ascii_uppercase();
    format!("CHATOS-{}-{}-{}", &raw[0..4], &raw[4..8], &raw[8..12])
}

pub async fn me(
    Extension(principal): Extension<CurrentPrincipal>,
) -> ApiResult<CurrentUserResponse> {
    if principal.principal_type != crate::models::PRINCIPAL_TYPE_HUMAN_USER {
        return Err(not_found("current user not found"));
    }
    Ok(Json(CurrentUserResponse {
        user: principal.auth_user(),
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
    state
        .store
        .revoke_client_session_by_jti(
            principal.jti.as_str(),
            principal.sub.as_str(),
            now_rfc3339().as_str(),
        )
        .await
        .map_err(internal_error)?;
    Ok(axum::http::StatusCode::NO_CONTENT)
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
        credential_version: None,
        agent_account_id: None,
        owner_user_id: None,
        owner_username: None,
        owner_display_name: None,
        scopes: vec!["user_service".to_string()],
    }
    .auth_user()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invite_code_normalization_trims_and_uppercases() {
        let normalized = normalize_invite_code("  chatos-abcd-ef12  ").unwrap();
        assert_eq!(normalized, "CHATOS-ABCD-EF12");
    }

    #[test]
    fn invite_code_normalization_rejects_whitespace_inside_code() {
        assert!(normalize_invite_code("CHATOS ABCD EF12").is_err());
    }
}
