// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::net::SocketAddr;

use axum::extract::{ConnectInfo, Path, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use base64::Engine;
use chrono::Utc;
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::auth::{issue_user_token_with_scopes, CurrentPrincipal};
#[cfg(debug_assertions)]
use crate::auth::{normalize_username, verify_password};
#[cfg(debug_assertions)]
use crate::models::WeChatMiniProgramDevelopmentLoginRequest;
use crate::models::{
    AuthUser, ClaimWeChatBindTicketRequest, ClaimWeChatBindTicketResponse, ClientSessionRecord,
    ClientSessionSummary, ConfirmWeChatBindTicketResponse, IssueWeChatBindTicketResponse,
    UserExternalIdentityRecord, UserRecord, WeChatBindClaimResultRequest,
    WeChatBindClaimResultResponse, WeChatBindTicketRecord, WeChatBindTicketStatusResponse,
    WeChatBindingStatusResponse, WeChatMiniProgramLoginRequest, WeChatMiniProgramLoginResponse,
    CLIENT_TYPE_WECHAT_MINI_PROGRAM, EXTERNAL_IDENTITY_PROVIDER_WECHAT_MINI_PROGRAM,
    PRINCIPAL_TYPE_HUMAN_USER, WECHAT_BIND_STATUS_CLAIMED, WECHAT_BIND_STATUS_ISSUED,
};
use crate::state::AppState;
use crate::store::now_rfc3339;
use crate::store::wechat_auth::BindExternalIdentityResult;
use crate::wechat::{WeChatExchangeError, WeChatMiniProgramClient, WeChatMiniProgramIdentity};

#[cfg(debug_assertions)]
use super::unauthorized;
use super::{
    bad_request, conflict, forbidden, internal_error, not_found, service_unavailable, ApiResult,
    ApiStatusResult,
};

const WECHAT_COMPANION_SCOPE: &str = "wechat_companion";
const SECRET_MAX_BYTES: usize = 512;

pub async fn login(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(input): Json<WeChatMiniProgramLoginRequest>,
) -> ApiResult<WeChatMiniProgramLoginResponse> {
    let source = wechat_source(addr);
    reject_locked(&state, "wechat-code-exchange", source.as_str())?;
    let provider_identity = match exchange_code(&state, input.code.as_str()).await {
        Ok(identity) => identity,
        Err(error) => {
            record_failure(&state, "wechat-code-exchange", source.as_str());
            return Err(map_exchange_error(error));
        }
    };
    let (open_id_hash, _) = hash_provider_identity(&state, &provider_identity)?;
    reject_locked(&state, open_id_hash.as_str(), source.as_str())?;
    let app_id = configured_app_id(&state)?;
    let Some(identity) = state
        .store
        .find_active_external_identity_by_subject(
            EXTERNAL_IDENTITY_PROVIDER_WECHAT_MINI_PROGRAM,
            app_id,
            open_id_hash.as_str(),
        )
        .await
        .map_err(internal_error)?
    else {
        state
            .login_throttle
            .record_success(open_id_hash.as_str(), Some(source.as_str()));
        return Ok(Json(WeChatMiniProgramLoginResponse::BindingRequired));
    };
    let user = load_enabled_user(&state, identity.user_id.as_str())
        .await
        .map_err(|error| {
            record_failure(&state, open_id_hash.as_str(), source.as_str());
            error
        })?;
    let (token, client_session_id) =
        issue_client_session(&state, &user, Some(identity.id.as_str())).await?;
    let now = now_rfc3339();
    state
        .store
        .touch_external_identity_login(identity.id.as_str(), now.as_str())
        .await
        .map_err(internal_error)?;
    state
        .store
        .touch_user_last_login(user.id.as_str())
        .await
        .map_err(internal_error)?;
    state
        .login_throttle
        .record_success(open_id_hash.as_str(), Some(source.as_str()));
    Ok(Json(WeChatMiniProgramLoginResponse::Authenticated {
        token,
        user: auth_user(&user),
        client_session_id,
    }))
}

/// Password-authenticated test entry for the local Mini Program simulator.
/// Release binaries do not register this handler.
#[cfg(debug_assertions)]
pub async fn development_login(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(input): Json<WeChatMiniProgramDevelopmentLoginRequest>,
) -> ApiResult<WeChatMiniProgramLoginResponse> {
    if state.config.wechat_mini_program_env_version != "develop" {
        return Err(not_found("development login is not enabled"));
    }

    let username = normalize_username(input.username.as_str()).map_err(bad_request)?;
    if input.password.trim().is_empty() {
        return Err(bad_request("password is required"));
    }

    let now_unix = Utc::now().timestamp();
    let source = format!("wechat-development:{}", addr.ip());
    if state.login_throttle.is_locked(
        username.as_str(),
        Some(source.as_str()),
        now_unix,
        &state.config,
    ) {
        return Err(unauthorized("invalid username or password"));
    }

    let Some(user) = state
        .store
        .find_user_by_username(username.as_str())
        .await
        .map_err(internal_error)?
    else {
        state.login_throttle.record_failure(
            username.as_str(),
            Some(source.as_str()),
            now_unix,
            &state.config,
        );
        return Err(unauthorized("invalid username or password"));
    };
    if !user.enabled || !verify_password(input.password.as_str(), user.password_hash.as_str()) {
        state.login_throttle.record_failure(
            username.as_str(),
            Some(source.as_str()),
            now_unix,
            &state.config,
        );
        return Err(unauthorized("invalid username or password"));
    }

    state
        .login_throttle
        .record_success(username.as_str(), Some(source.as_str()));
    state
        .store
        .touch_user_last_login(user.id.as_str())
        .await
        .map_err(internal_error)?;
    let (token, client_session_id) = issue_client_session(&state, &user, None).await?;
    tracing::warn!(
        user_id = %user.id,
        client_session_id = %client_session_id,
        "issued a development-only WeChat Companion session"
    );

    Ok(Json(WeChatMiniProgramLoginResponse::Authenticated {
        token,
        user: auth_user(&user),
        client_session_id,
    }))
}

pub async fn issue_bind_ticket(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
) -> ApiResult<IssueWeChatBindTicketResponse> {
    let user_id = human_user_id(&principal)?;
    let app_id = configured_app_id(&state)?;
    if state
        .store
        .find_active_external_identity_for_user(
            user_id,
            EXTERNAL_IDENTITY_PROVIDER_WECHAT_MINI_PROGRAM,
            app_id,
        )
        .await
        .map_err(internal_error)?
        .is_some()
    {
        return Err(conflict("a WeChat account is already bound"));
    }
    let bind_ticket = generate_secret(16);
    let qr_code = state
        .wechat_mini_program
        .as_ref()
        .ok_or_else(|| service_unavailable("WeChat Mini Program login is not configured"))?
        .generate_bind_code(bind_ticket.as_str())
        .await
        .map_err(map_exchange_error)?;
    let now_unix = Utc::now().timestamp();
    let expires_at_unix = now_unix + state.config.wechat_mini_program_bind_ticket_ttl_seconds;
    let now = now_rfc3339();
    state
        .store
        .expire_open_wechat_bind_tickets_for_user(user_id, app_id, now.as_str())
        .await
        .map_err(internal_error)?;
    let record = WeChatBindTicketRecord {
        id: Uuid::new_v4().to_string(),
        ticket_hash: hash_secret(&state, "bind-ticket", bind_ticket.as_str())?,
        user_id: user_id.to_string(),
        app_id: app_id.to_string(),
        status: WECHAT_BIND_STATUS_ISSUED.to_string(),
        claimed_open_id_hash: None,
        claimed_union_id_hash: None,
        claim_id: None,
        claim_secret_hash: None,
        confirmed_external_identity_id: None,
        expires_at_unix,
        claimed_at: None,
        confirmed_at: None,
        consumed_at: None,
        created_at: now.clone(),
        updated_at: now,
    };
    state
        .store
        .insert_wechat_bind_ticket(&record)
        .await
        .map_err(internal_error)?;
    Ok(Json(IssueWeChatBindTicketResponse {
        ticket_id: record.id,
        scene: bind_ticket.clone(),
        bind_ticket,
        expires_in_seconds: state.config.wechat_mini_program_bind_ticket_ttl_seconds,
        expires_at_unix,
        qr_code_data_url: format!(
            "data:{};base64,{}",
            if qr_code.starts_with(&[0xff, 0xd8, 0xff]) {
                "image/jpeg"
            } else {
                "image/png"
            },
            base64::engine::general_purpose::STANDARD.encode(qr_code)
        ),
    }))
}

pub async fn claim_bind_ticket(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(input): Json<ClaimWeChatBindTicketRequest>,
) -> ApiResult<ClaimWeChatBindTicketResponse> {
    validate_secret(input.bind_ticket.as_str(), "bind_ticket")?;
    let source = wechat_source(addr);
    reject_locked(&state, "wechat-bind-claim", source.as_str())?;
    let provider_identity = match exchange_code(&state, input.code.as_str()).await {
        Ok(identity) => identity,
        Err(error) => {
            record_failure(&state, "wechat-bind-claim", source.as_str());
            return Err(map_exchange_error(error));
        }
    };
    let (open_id_hash, union_id_hash) = hash_provider_identity(&state, &provider_identity)?;
    reject_locked(&state, open_id_hash.as_str(), source.as_str())?;
    let claim_id = Uuid::new_v4().to_string();
    let claim_secret = generate_secret(32);
    let claim_secret_hash = hash_secret(&state, "claim-secret", claim_secret.as_str())?;
    let ticket_hash = hash_secret(&state, "bind-ticket", input.bind_ticket.trim())?;
    let now_unix = Utc::now().timestamp();
    let now = now_rfc3339();
    let record = state
        .store
        .claim_wechat_bind_ticket(
            ticket_hash.as_str(),
            configured_app_id(&state)?,
            open_id_hash.as_str(),
            union_id_hash.as_deref(),
            claim_id.as_str(),
            claim_secret_hash.as_str(),
            now_unix,
            now.as_str(),
        )
        .await
        .map_err(internal_error)?
        .ok_or_else(|| bad_request("bind ticket is invalid, expired, or already claimed"))?;
    state
        .login_throttle
        .record_success(open_id_hash.as_str(), Some(source.as_str()));
    Ok(Json(ClaimWeChatBindTicketResponse {
        status: WECHAT_BIND_STATUS_CLAIMED.to_string(),
        claim_id,
        claim_secret,
        expires_at_unix: record.expires_at_unix,
    }))
}

pub async fn get_bind_ticket(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
    Path(ticket_id): Path<String>,
) -> ApiResult<WeChatBindTicketStatusResponse> {
    let user_id = human_user_id(&principal)?;
    let record = state
        .store
        .find_wechat_bind_ticket_for_user(ticket_id.as_str(), user_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("bind ticket not found"))?;
    let status = if record.expires_at_unix <= Utc::now().timestamp()
        && record.status != crate::models::WECHAT_BIND_STATUS_CONSUMED
    {
        "expired".to_string()
    } else {
        record.status
    };
    Ok(Json(WeChatBindTicketStatusResponse {
        ticket_id: record.id,
        status,
        expires_at_unix: record.expires_at_unix,
        claimed_at: record.claimed_at,
    }))
}

pub async fn confirm_bind_ticket(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
    Path(ticket_id): Path<String>,
) -> ApiResult<ConfirmWeChatBindTicketResponse> {
    let user_id = human_user_id(&principal)?;
    let ticket = state
        .store
        .find_wechat_bind_ticket_for_user(ticket_id.as_str(), user_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("bind ticket not found"))?;
    if ticket.status != WECHAT_BIND_STATUS_CLAIMED
        || ticket.expires_at_unix <= Utc::now().timestamp()
    {
        return Err(conflict("bind ticket is not awaiting confirmation"));
    }
    let open_id_hash = ticket
        .claimed_open_id_hash
        .as_deref()
        .ok_or_else(|| internal_error("bind ticket claim is incomplete"))?;
    let now = now_rfc3339();
    let identity = UserExternalIdentityRecord {
        id: Uuid::new_v4().to_string(),
        user_id: user_id.to_string(),
        provider: EXTERNAL_IDENTITY_PROVIDER_WECHAT_MINI_PROGRAM.to_string(),
        app_id: ticket.app_id.clone(),
        open_id_hash: open_id_hash.to_string(),
        union_id_hash: ticket.claimed_union_id_hash.clone(),
        created_at: now.clone(),
        updated_at: now.clone(),
        last_login_at: None,
        revoked_at: None,
    };
    let identity = match state
        .store
        .bind_external_identity(&identity)
        .await
        .map_err(internal_error)?
    {
        BindExternalIdentityResult::Bound(identity) => identity,
        BindExternalIdentityResult::Conflict => {
            return Err(conflict(
                "the WeChat account or ChatOS account is already bound elsewhere",
            ));
        }
    };
    state
        .store
        .confirm_wechat_bind_ticket(
            ticket.id.as_str(),
            user_id,
            identity.id.as_str(),
            Utc::now().timestamp(),
            now.as_str(),
        )
        .await
        .map_err(internal_error)?
        .ok_or_else(|| conflict("bind ticket was already confirmed or expired"))?;
    Ok(Json(ConfirmWeChatBindTicketResponse {
        status: crate::models::WECHAT_BIND_STATUS_CONFIRMED.to_string(),
        external_identity_id: identity.id,
    }))
}

pub async fn claim_result(
    State(state): State<AppState>,
    Path(claim_id): Path<String>,
    Json(input): Json<WeChatBindClaimResultRequest>,
) -> ApiResult<WeChatBindClaimResultResponse> {
    validate_secret(claim_id.as_str(), "claim_id")?;
    validate_secret(input.claim_secret.as_str(), "claim_secret")?;
    let secret_hash = hash_secret(&state, "claim-secret", input.claim_secret.trim())?;
    let now_unix = Utc::now().timestamp();
    let now = now_rfc3339();
    let consumed = state
        .store
        .consume_confirmed_wechat_claim(
            claim_id.as_str(),
            secret_hash.as_str(),
            now_unix,
            now.as_str(),
        )
        .await
        .map_err(internal_error)?;
    let Some(ticket) = consumed else {
        let pending = state
            .store
            .find_wechat_claim(claim_id.as_str(), secret_hash.as_str(), now_unix)
            .await
            .map_err(internal_error)?
            .ok_or_else(|| bad_request("bind claim is invalid or expired"))?;
        if pending.status == WECHAT_BIND_STATUS_CLAIMED {
            return Ok(Json(
                WeChatBindClaimResultResponse::PendingDesktopConfirmation,
            ));
        }
        return Err(conflict("bind claim has already been consumed"));
    };
    let identity_id = ticket
        .confirmed_external_identity_id
        .as_deref()
        .ok_or_else(|| internal_error("confirmed binding is incomplete"))?;
    let identity = state
        .store
        .find_active_external_identity_by_id(identity_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| conflict("WeChat binding is no longer active"))?;
    let user = load_enabled_user(&state, ticket.user_id.as_str()).await?;
    let (token, client_session_id) =
        issue_client_session(&state, &user, Some(identity.id.as_str())).await?;
    state
        .store
        .touch_external_identity_login(identity.id.as_str(), now.as_str())
        .await
        .map_err(internal_error)?;
    state
        .store
        .touch_user_last_login(user.id.as_str())
        .await
        .map_err(internal_error)?;
    Ok(Json(WeChatBindClaimResultResponse::Authenticated {
        token,
        user: auth_user(&user),
        client_session_id,
    }))
}

pub async fn list_client_sessions(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
) -> ApiResult<Vec<ClientSessionSummary>> {
    let sessions = state
        .store
        .list_client_sessions(human_user_id(&principal)?)
        .await
        .map_err(internal_error)?
        .into_iter()
        .map(ClientSessionSummary::from)
        .collect();
    Ok(Json(sessions))
}

pub async fn get_binding(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
) -> ApiResult<WeChatBindingStatusResponse> {
    let user_id = human_user_id(&principal)?;
    let identity = state
        .store
        .find_active_external_identity_for_user(
            user_id,
            EXTERNAL_IDENTITY_PROVIDER_WECHAT_MINI_PROGRAM,
            configured_app_id(&state)?,
        )
        .await
        .map_err(internal_error)?;
    Ok(Json(match identity {
        Some(identity) => WeChatBindingStatusResponse {
            bound: true,
            created_at: Some(identity.created_at),
            last_login_at: identity.last_login_at,
        },
        None => WeChatBindingStatusResponse {
            bound: false,
            created_at: None,
            last_login_at: None,
        },
    }))
}

pub async fn revoke_client_session(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
    Path(session_id): Path<String>,
) -> ApiStatusResult {
    let user_id = human_user_id(&principal)?;
    let now = now_rfc3339();
    let session = state
        .store
        .revoke_client_session(
            session_id.as_str(),
            user_id,
            principal.sub.as_str(),
            now.as_str(),
        )
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("active client session not found"))?;
    state
        .store
        .revoke_token(
            session.token_jti.as_str(),
            format!("user:{}", session.user_id).as_str(),
            session.expires_at_unix,
        )
        .await
        .map_err(internal_error)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn unbind(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
) -> ApiStatusResult {
    let user_id = human_user_id(&principal)?;
    let app_id = configured_app_id(&state)?;
    let now = now_rfc3339();
    let Some(identity) = state
        .store
        .revoke_external_identity(
            user_id,
            EXTERNAL_IDENTITY_PROVIDER_WECHAT_MINI_PROGRAM,
            app_id,
            now.as_str(),
        )
        .await
        .map_err(internal_error)?
    else {
        return Ok(StatusCode::NO_CONTENT);
    };
    let sessions = state
        .store
        .revoke_client_sessions_for_identity(
            identity.id.as_str(),
            principal.sub.as_str(),
            now.as_str(),
        )
        .await
        .map_err(internal_error)?;
    for session in sessions {
        state
            .store
            .revoke_token(
                session.token_jti.as_str(),
                format!("user:{}", session.user_id).as_str(),
                session.expires_at_unix,
            )
            .await
            .map_err(internal_error)?;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn issue_client_session(
    state: &AppState,
    user: &UserRecord,
    external_identity_id: Option<&str>,
) -> Result<(String, String), (StatusCode, Json<Value>)> {
    let issued = issue_user_token_with_scopes(
        &state.config,
        user,
        state.config.wechat_mini_program_client_session_ttl_seconds,
        vec![WECHAT_COMPANION_SCOPE.to_string()],
    )
    .map_err(internal_error)?;
    let now = now_rfc3339();
    let session = ClientSessionRecord {
        id: Uuid::new_v4().to_string(),
        user_id: user.id.clone(),
        client_type: CLIENT_TYPE_WECHAT_MINI_PROGRAM.to_string(),
        external_identity_id: external_identity_id.map(ToOwned::to_owned),
        token_jti: issued.jti,
        created_at: now.clone(),
        updated_at: now.clone(),
        last_seen_at: now,
        expires_at_unix: issued.expires_at_unix,
        revoked_at: None,
        revoked_by: None,
    };
    state
        .store
        .insert_client_session(&session)
        .await
        .map_err(internal_error)?;
    Ok((issued.token, session.id))
}

async fn exchange_code(
    state: &AppState,
    code: &str,
) -> Result<WeChatMiniProgramIdentity, WeChatExchangeError> {
    let client = state
        .wechat_mini_program
        .as_ref()
        .ok_or(WeChatExchangeError::NotConfigured)?;
    client.exchange_code(code).await
}

fn configured_client(
    state: &AppState,
) -> Result<&WeChatMiniProgramClient, (StatusCode, Json<Value>)> {
    state
        .wechat_mini_program
        .as_ref()
        .ok_or_else(|| service_unavailable("WeChat Mini Program login is not configured"))
}

fn configured_app_id(state: &AppState) -> Result<&str, (StatusCode, Json<Value>)> {
    Ok(configured_client(state)?.app_id())
}

fn hash_provider_identity(
    state: &AppState,
    identity: &WeChatMiniProgramIdentity,
) -> Result<(String, Option<String>), (StatusCode, Json<Value>)> {
    Ok((
        hash_secret(state, "openid", identity.open_id.as_str())?,
        identity
            .union_id
            .as_deref()
            .map(|value| hash_secret(state, "unionid", value))
            .transpose()?,
    ))
}

fn hash_secret(
    state: &AppState,
    domain: &str,
    value: &str,
) -> Result<String, (StatusCode, Json<Value>)> {
    let secret = state
        .config
        .wechat_mini_program_identity_hash_secret
        .as_deref()
        .ok_or_else(|| service_unavailable("WeChat Mini Program login is not configured"))?;
    let app_id = state
        .config
        .wechat_mini_program_app_id
        .as_deref()
        .ok_or_else(|| service_unavailable("WeChat Mini Program login is not configured"))?;
    Ok(hex::encode(Sha256::digest(
        format!("chatos-wechat-v1:{domain}:{secret}:{app_id}:{value}").as_bytes(),
    )))
}

fn validate_secret(value: &str, field: &str) -> Result<(), (StatusCode, Json<Value>)> {
    let value = value.trim();
    if value.is_empty() || value.len() > SECRET_MAX_BYTES || value.contains(char::is_whitespace) {
        return Err(bad_request(format!("{field} is invalid")));
    }
    Ok(())
}

fn generate_secret(bytes: usize) -> String {
    let mut value = vec![0_u8; bytes];
    rand::fill(value.as_mut_slice());
    hex::encode(value)
}

fn human_user_id(principal: &CurrentPrincipal) -> Result<&str, (StatusCode, Json<Value>)> {
    if principal.principal_type != PRINCIPAL_TYPE_HUMAN_USER {
        return Err(forbidden("human user session required"));
    }
    principal
        .user_id
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| forbidden("human user session required"))
}

async fn load_enabled_user(
    state: &AppState,
    user_id: &str,
) -> Result<UserRecord, (StatusCode, Json<Value>)> {
    let user = state
        .store
        .find_user_by_id(user_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| forbidden("account is unavailable"))?;
    if !user.enabled {
        return Err(forbidden("account is unavailable"));
    }
    Ok(user)
}

fn auth_user(user: &UserRecord) -> AuthUser {
    AuthUser {
        id: user.id.clone(),
        username: user.username.clone(),
        display_name: user.display_name.clone(),
        role: user.role.clone(),
        principal_type: PRINCIPAL_TYPE_HUMAN_USER.to_string(),
    }
}

fn reject_locked(
    state: &AppState,
    identity: &str,
    source: &str,
) -> Result<(), (StatusCode, Json<Value>)> {
    if state.login_throttle.is_locked(
        identity,
        Some(source),
        Utc::now().timestamp(),
        &state.config,
    ) {
        Err(bad_request("WeChat authentication is temporarily locked"))
    } else {
        Ok(())
    }
}

fn record_failure(state: &AppState, identity: &str, source: &str) {
    state.login_throttle.record_failure(
        identity,
        Some(source),
        Utc::now().timestamp(),
        &state.config,
    );
}

fn wechat_source(addr: SocketAddr) -> String {
    format!("wechat:{}", addr.ip())
}

fn map_exchange_error(error: WeChatExchangeError) -> (StatusCode, Json<Value>) {
    match error {
        WeChatExchangeError::NotConfigured => service_unavailable(error.public_message()),
        WeChatExchangeError::InvalidCode | WeChatExchangeError::ProviderRejected { .. } => {
            bad_request(error.public_message())
        }
        WeChatExchangeError::Transport | WeChatExchangeError::InvalidResponse => {
            service_unavailable(error.public_message())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_bind_ticket_fits_wechat_scene_limit() {
        let ticket = generate_secret(16);
        assert_eq!(ticket.len(), 32);
        assert!(!ticket.contains(char::is_whitespace));
    }

    #[test]
    fn secret_validation_rejects_values_that_could_break_protocol_boundaries() {
        assert!(validate_secret("", "ticket").is_err());
        assert!(validate_secret("has spaces", "ticket").is_err());
        assert!(validate_secret(&"x".repeat(513), "ticket").is_err());
        assert!(validate_secret("safe-value", "ticket").is_ok());
    }

    #[test]
    fn companion_scope_is_distinct_from_full_user_service_scope() {
        assert_ne!(WECHAT_COMPANION_SCOPE, "user_service");
    }
}
