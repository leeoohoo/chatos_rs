// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::net::SocketAddr;

use axum::extract::{ConnectInfo, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::{Extension, Json};
use base64::Engine;
use chrono::Utc;
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::auth::{issue_user_token_with_scopes, CurrentPrincipal};
use crate::auth::{normalize_username, verify_password};
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
    headers: HeaderMap,
    Json(input): Json<WeChatMiniProgramLoginRequest>,
) -> ApiResult<WeChatMiniProgramLoginResponse> {
    let (device_id, device_public_key) =
        validate_device_identity(input.device_id.as_str(), input.device_public_key.as_str())?;
    let source = wechat_source(&headers, addr);
    reject_locked(&state, "wechat-code-exchange", source.as_str()).await?;
    let provider_identity = match exchange_code(&state, input.code.as_str()).await {
        Ok(identity) => identity,
        Err(error) => {
            record_failure(&state, "wechat-code-exchange", source.as_str()).await?;
            return Err(map_exchange_error(error));
        }
    };
    let (open_id_hash, _) = hash_provider_identity(&state, &provider_identity)?;
    reject_locked(&state, open_id_hash.as_str(), source.as_str()).await?;
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
            .record_success(open_id_hash.as_str(), Some(source.as_str()))
            .await
            .map_err(internal_error)?;
        return Ok(Json(WeChatMiniProgramLoginResponse::BindingRequired));
    };
    if identity.companion_device_id.as_deref() != Some(device_id.as_str())
        || identity.companion_device_public_key.as_deref() != Some(device_public_key.as_str())
    {
        return Ok(Json(WeChatMiniProgramLoginResponse::BindingRequired));
    }
    let user = match load_enabled_user(&state, identity.user_id.as_str()).await {
        Ok(user) => user,
        Err(error) => {
            record_failure(&state, open_id_hash.as_str(), source.as_str()).await?;
            return Err(error);
        }
    };
    let (token, client_session_id) = issue_client_session(
        &state,
        &user,
        Some(identity.id.as_str()),
        device_id.as_str(),
        device_public_key.as_str(),
    )
    .await?;
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
        .record_success(open_id_hash.as_str(), Some(source.as_str()))
        .await
        .map_err(internal_error)?;
    Ok(Json(WeChatMiniProgramLoginResponse::Authenticated {
        token,
        user: auth_user(&user),
        client_session_id,
    }))
}

/// Password-authenticated test entry for the Mini Program developer simulator.
/// Release binaries keep the route disabled unless the operator explicitly enables it.
pub async fn development_login(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(input): Json<WeChatMiniProgramDevelopmentLoginRequest>,
) -> ApiResult<WeChatMiniProgramLoginResponse> {
    if !state.config.wechat_mini_program_development_login_enabled {
        return Err(not_found("development login is not enabled"));
    }

    let username = normalize_username(input.username.as_str()).map_err(bad_request)?;
    if input.password.trim().is_empty() {
        return Err(bad_request("password is required"));
    }
    let (device_id, device_public_key) =
        validate_device_identity(input.device_id.as_str(), input.device_public_key.as_str())?;

    let now_unix = Utc::now().timestamp();
    let source = format!(
        "wechat-development:{}",
        crate::login_throttle::request_source(&headers, addr)
    );
    if state
        .login_throttle
        .is_locked(
            username.as_str(),
            Some(source.as_str()),
            now_unix,
            &state.config,
        )
        .await
        .map_err(internal_error)?
    {
        return Err(unauthorized("invalid username or password"));
    }

    let Some(user) = state
        .store
        .find_user_by_username(username.as_str())
        .await
        .map_err(internal_error)?
    else {
        state
            .login_throttle
            .record_failure(
                username.as_str(),
                Some(source.as_str()),
                now_unix,
                &state.config,
            )
            .await
            .map_err(internal_error)?;
        return Err(unauthorized("invalid username or password"));
    };
    if !user.enabled || !verify_password(input.password.as_str(), user.password_hash.as_str()) {
        state
            .login_throttle
            .record_failure(
                username.as_str(),
                Some(source.as_str()),
                now_unix,
                &state.config,
            )
            .await
            .map_err(internal_error)?;
        return Err(unauthorized("invalid username or password"));
    }

    state
        .login_throttle
        .record_success(username.as_str(), Some(source.as_str()))
        .await
        .map_err(internal_error)?;
    state
        .store
        .touch_user_last_login(user.id.as_str())
        .await
        .map_err(internal_error)?;
    let (token, client_session_id) = issue_client_session(
        &state,
        &user,
        None,
        device_id.as_str(),
        device_public_key.as_str(),
    )
    .await?;
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
    // An already-bound account may intentionally replace its phone. The new device
    // still has to scan this short-lived ticket and receive explicit desktop approval.
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
        claimed_device_id: None,
        claimed_device_public_key: None,
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
    headers: HeaderMap,
    Json(input): Json<ClaimWeChatBindTicketRequest>,
) -> ApiResult<ClaimWeChatBindTicketResponse> {
    validate_secret(input.bind_ticket.as_str(), "bind_ticket")?;
    let (device_id, device_public_key) =
        validate_device_identity(input.device_id.as_str(), input.device_public_key.as_str())?;
    let source = wechat_source(&headers, addr);
    reject_locked(&state, "wechat-bind-claim", source.as_str()).await?;
    let provider_identity = match exchange_code(&state, input.code.as_str()).await {
        Ok(identity) => identity,
        Err(error) => {
            record_failure(&state, "wechat-bind-claim", source.as_str()).await?;
            return Err(map_exchange_error(error));
        }
    };
    let (open_id_hash, union_id_hash) = hash_provider_identity(&state, &provider_identity)?;
    reject_locked(&state, open_id_hash.as_str(), source.as_str()).await?;
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
            device_id.as_str(),
            device_public_key.as_str(),
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
        .record_success(open_id_hash.as_str(), Some(source.as_str()))
        .await
        .map_err(internal_error)?;
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
    let device_id = ticket
        .claimed_device_id
        .as_deref()
        .ok_or_else(|| internal_error("bind ticket device identity is incomplete"))?;
    let device_public_key = ticket
        .claimed_device_public_key
        .as_deref()
        .ok_or_else(|| internal_error("bind ticket device key is incomplete"))?;
    let now = now_rfc3339();
    let identity = UserExternalIdentityRecord {
        id: Uuid::new_v4().to_string(),
        user_id: user_id.to_string(),
        provider: EXTERNAL_IDENTITY_PROVIDER_WECHAT_MINI_PROGRAM.to_string(),
        app_id: ticket.app_id.clone(),
        open_id_hash: open_id_hash.to_string(),
        union_id_hash: ticket.claimed_union_id_hash.clone(),
        companion_device_id: Some(device_id.to_string()),
        companion_device_public_key: Some(device_public_key.to_string()),
        created_at: now.clone(),
        updated_at: now.clone(),
        last_login_at: None,
        revoked_at: None,
    };
    let previous_identity = state
        .store
        .find_active_external_identity_by_subject(
            EXTERNAL_IDENTITY_PROVIDER_WECHAT_MINI_PROGRAM,
            ticket.app_id.as_str(),
            open_id_hash,
        )
        .await
        .map_err(internal_error)?;
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
    if previous_identity.as_ref().is_some_and(|previous| {
        previous.companion_device_id.as_deref() != Some(device_id)
            || previous.companion_device_public_key.as_deref() != Some(device_public_key)
    }) {
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
    }
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
    let device_id = identity
        .companion_device_id
        .as_deref()
        .ok_or_else(|| conflict("confirmed binding is missing its device identity"))?;
    let device_public_key = identity
        .companion_device_public_key
        .as_deref()
        .ok_or_else(|| conflict("confirmed binding is missing its device key"))?;
    let (token, client_session_id) = issue_client_session(
        &state,
        &user,
        Some(identity.id.as_str()),
        device_id,
        device_public_key,
    )
    .await?;
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

include!("wechat_auth_part01.rs");
