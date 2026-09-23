// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::body::{to_bytes, Body};
use axum::extract::State;
use axum::http::{HeaderMap, Request};
use axum::Json;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use chrono::Utc;
use ring::signature::{UnparsedPublicKey, ED25519};
use sha2::{Digest, Sha512};

use crate::auth::{
    bearer_token_from_headers, decode_any_user_service_token, AuthClaims, CurrentPrincipal,
};
use crate::models::{DeviceProofVerificationRequest, TokenVerifyResponse, VerifiedPrincipal};
use crate::state::AppState;
use crate::store::now_rfc3339;

use super::{internal_error, refresh_principal_identity, unauthorized, ApiResult};

const DEVICE_PROOF_MAX_SKEW_SECONDS: i64 = 60;
const DEVICE_PROOF_BODY_LIMIT_BYTES: usize = 4 * 1024 * 1024;

const HEADER_DEVICE_ID: &str = "x-chatos-device-id";
const HEADER_SESSION_ID: &str = "x-chatos-device-session-id";
const HEADER_TIMESTAMP: &str = "x-chatos-device-timestamp";
const HEADER_NONCE: &str = "x-chatos-device-nonce";
const HEADER_BODY_SHA512: &str = "x-chatos-device-body-sha512";
const HEADER_SIGNATURE_ALGORITHM: &str = "x-chatos-device-signature-alg";
const HEADER_SIGNATURE: &str = "x-chatos-device-signature";

pub(super) async fn verify_forwarded_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(proof): Json<DeviceProofVerificationRequest>,
) -> ApiResult<TokenVerifyResponse> {
    let token = bearer_token_from_headers(&headers).map_err(|error| unauthorized(&error))?;
    let claims = decode_any_user_service_token(token.as_str(), &state.config)
        .map_err(|_| unauthorized("invalid or expired token"))?;
    authenticate_claims(&state, &claims).await?;
    if claims
        .scopes
        .iter()
        .any(|scope| scope == "wechat_companion")
    {
        verify_device_proof(&state, &claims, &proof).await?;
    }
    let mut principal = CurrentPrincipal::from(claims);
    refresh_principal_identity(&state, &mut principal).await?;
    Ok(Json(TokenVerifyResponse {
        principal: verified_principal(principal),
    }))
}

pub(super) async fn authenticate_claims(
    state: &AppState,
    claims: &AuthClaims,
) -> Result<(), (axum::http::StatusCode, Json<serde_json::Value>)> {
    let is_companion = claims
        .scopes
        .iter()
        .any(|scope| scope == "wechat_companion");
    if state
        .store
        .is_token_revoked(claims.jti.as_str())
        .await
        .map_err(internal_error)?
    {
        return Err(unauthorized("token has been revoked"));
    }
    if state
        .store
        .is_client_session_invalid(claims.jti.as_str(), is_companion)
        .await
        .map_err(internal_error)?
    {
        return Err(unauthorized("client session has been revoked or expired"));
    }
    if is_companion {
        state
            .store
            .touch_client_session(claims.jti.as_str(), now_rfc3339().as_str())
            .await
            .map_err(internal_error)?;
    }
    Ok(())
}

pub(super) async fn verify_device_proof(
    state: &AppState,
    claims: &AuthClaims,
    proof: &DeviceProofVerificationRequest,
) -> Result<(), (axum::http::StatusCode, Json<serde_json::Value>)> {
    validate_proof_fields(proof)?;
    let session = state
        .store
        .find_client_session_by_jti(claims.jti.as_str())
        .await
        .map_err(internal_error)?
        .ok_or_else(|| unauthorized("device-bound client session was not found"))?;
    if session.id != proof.client_session_id
        || session.device_id.as_deref() != Some(proof.device_id.as_str())
    {
        return Err(unauthorized(
            "device proof does not match the bound client session",
        ));
    }
    let public_key_text = session
        .device_public_key
        .as_deref()
        .ok_or_else(|| unauthorized("client session is not bound to a device key"))?;
    let public_key = decode_public_key(public_key_text)
        .ok_or_else(|| unauthorized("bound device public key is invalid"))?;
    let signature = URL_SAFE_NO_PAD
        .decode(proof.signature.as_bytes())
        .map_err(|_| unauthorized("device proof signature is invalid"))?;
    let payload = signature_payload(proof);
    UnparsedPublicKey::new(&ED25519, public_key)
        .verify(payload.as_bytes(), signature.as_slice())
        .map_err(|_| unauthorized("device proof signature verification failed"))?;

    let consumed = state
        .store
        .consume_device_proof_nonce(
            session.id.as_str(),
            proof.nonce.as_str(),
            (proof.timestamp + DEVICE_PROOF_MAX_SKEW_SECONDS) * 1_000,
        )
        .await
        .map_err(internal_error)?;
    if !consumed {
        return Err(unauthorized("device proof nonce has already been used"));
    }
    Ok(())
}

pub(super) fn proof_from_request(
    surface: &str,
    request: &Request<Body>,
) -> Result<DeviceProofVerificationRequest, (axum::http::StatusCode, Json<serde_json::Value>)> {
    let headers = request.headers();
    let target = external_target(surface, request.uri());
    let timestamp = required_header(headers, HEADER_TIMESTAMP)?
        .parse::<i64>()
        .map_err(|_| unauthorized("device proof timestamp is invalid"))?;
    Ok(DeviceProofVerificationRequest {
        surface: surface.to_string(),
        method: request.method().as_str().to_string(),
        target,
        body_sha512: required_header(headers, HEADER_BODY_SHA512)?,
        client_session_id: required_header(headers, HEADER_SESSION_ID)?,
        device_id: required_header(headers, HEADER_DEVICE_ID)?,
        timestamp,
        nonce: required_header(headers, HEADER_NONCE)?,
        signature_algorithm: required_header(headers, HEADER_SIGNATURE_ALGORITHM)?,
        signature: required_header(headers, HEADER_SIGNATURE)?,
    })
}

pub(super) async fn verify_and_restore_body(
    request: &mut Request<Body>,
    expected_hash: &str,
) -> Result<(), (axum::http::StatusCode, Json<serde_json::Value>)> {
    let body = std::mem::replace(request.body_mut(), Body::empty());
    let bytes = to_bytes(body, DEVICE_PROOF_BODY_LIMIT_BYTES)
        .await
        .map_err(|_| unauthorized("device-bound request body is too large"))?;
    let actual = URL_SAFE_NO_PAD.encode(Sha512::digest(bytes.as_ref()));
    *request.body_mut() = Body::from(bytes);
    if actual != expected_hash {
        return Err(unauthorized(
            "device proof body digest does not match the request",
        ));
    }
    Ok(())
}

fn required_header(
    headers: &HeaderMap,
    name: &'static str,
) -> Result<String, (axum::http::StatusCode, Json<serde_json::Value>)> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            unauthorized(format!("required device proof header {name} is missing").as_str())
        })
}

fn external_target(surface: &str, uri: &axum::http::Uri) -> String {
    let internal_path = uri.path();
    let logical_path = match surface {
        "local" => internal_path
            .strip_prefix("/api/local-connectors")
            .unwrap_or(internal_path),
        _ => internal_path.strip_prefix("/api").unwrap_or(internal_path),
    };
    let mut target = format!("/api/{surface}{logical_path}");
    if let Some(query) = uri.query().filter(|query| !query.is_empty()) {
        target.push('?');
        target.push_str(query);
    }
    target
}

fn validate_proof_fields(
    proof: &DeviceProofVerificationRequest,
) -> Result<(), (axum::http::StatusCode, Json<serde_json::Value>)> {
    if !matches!(proof.surface.as_str(), "user" | "chatos" | "local")
        || !matches!(
            proof.method.as_str(),
            "GET" | "POST" | "PUT" | "PATCH" | "DELETE"
        )
        || proof.target.len() > 4096
        || !proof
            .target
            .starts_with(format!("/api/{}/", proof.surface).as_str())
        || contains_line_break(&proof.target)
        || proof.body_sha512.len() != 86
        || URL_SAFE_NO_PAD
            .decode(proof.body_sha512.as_bytes())
            .map_or(true, |value| value.len() != 64)
        || proof.client_session_id.trim().is_empty()
        || contains_line_break(&proof.client_session_id)
        || proof.device_id.trim().is_empty()
        || contains_line_break(&proof.device_id)
        || !(16..=128).contains(&proof.nonce.len())
        || contains_line_break(&proof.nonce)
        || proof.signature_algorithm != "ed25519"
        || (Utc::now().timestamp() - proof.timestamp).abs() > DEVICE_PROOF_MAX_SKEW_SECONDS
    {
        return Err(unauthorized("device proof metadata is invalid or expired"));
    }
    Ok(())
}

fn decode_public_key(value: &str) -> Option<Vec<u8>> {
    let encoded = value.strip_prefix("ed25519:")?;
    URL_SAFE_NO_PAD
        .decode(encoded.as_bytes())
        .ok()
        .filter(|value| value.len() == 32)
}

fn contains_line_break(value: &str) -> bool {
    value.contains('\n') || value.contains('\r')
}

pub(crate) fn signature_payload(proof: &DeviceProofVerificationRequest) -> String {
    format!(
        "chatos-device-proof-v1\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        proof.surface,
        proof.method,
        proof.target,
        proof.body_sha512,
        proof.client_session_id,
        proof.device_id,
        proof.timestamp,
        proof.nonce,
    )
}

fn verified_principal(principal: CurrentPrincipal) -> VerifiedPrincipal {
    VerifiedPrincipal {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    #[test]
    fn canonical_payload_binds_request_and_device() {
        let proof = DeviceProofVerificationRequest {
            surface: "chatos".to_string(),
            method: "POST".to_string(),
            target: "/api/chatos/agent/chat/send".to_string(),
            body_sha512: "a".repeat(86),
            client_session_id: "session-1".to_string(),
            device_id: "phone-1".to_string(),
            timestamp: 123,
            nonce: "nonce-1234567890".to_string(),
            signature_algorithm: "ed25519".to_string(),
            signature: "signature".to_string(),
        };
        assert_eq!(
            signature_payload(&proof),
            format!(
                "chatos-device-proof-v1\nchatos\nPOST\n/api/chatos/agent/chat/send\n{}\nsession-1\nphone-1\n123\nnonce-1234567890",
                "a".repeat(86)
            )
        );
    }

    #[test]
    fn signature_rejects_changed_request_content_or_target() {
        let key_pair = Ed25519KeyPair::from_seed_unchecked(&[7_u8; 32]).expect("test key");
        let proof = DeviceProofVerificationRequest {
            surface: "chatos".to_string(),
            method: "POST".to_string(),
            target: "/api/chatos/agent/chat/send".to_string(),
            body_sha512: URL_SAFE_NO_PAD.encode(Sha512::digest(br#"{"message":"hello"}"#)),
            client_session_id: "session-1".to_string(),
            device_id: "phone-1234567890".to_string(),
            timestamp: 123,
            nonce: "nonce-1234567890".to_string(),
            signature_algorithm: "ed25519".to_string(),
            signature: String::new(),
        };
        let signed_payload = signature_payload(&proof);
        let signature = key_pair.sign(signed_payload.as_bytes());
        let verifier = UnparsedPublicKey::new(&ED25519, key_pair.public_key().as_ref());
        verifier
            .verify(signed_payload.as_bytes(), signature.as_ref())
            .expect("original request verifies");

        let mut changed_body = proof.clone();
        changed_body.body_sha512 =
            URL_SAFE_NO_PAD.encode(Sha512::digest(br#"{"message":"run a different command"}"#));
        assert!(verifier
            .verify(
                signature_payload(&changed_body).as_bytes(),
                signature.as_ref()
            )
            .is_err());

        let mut changed_target = proof;
        changed_target.target = "/api/chatos/agent/chat/stop".to_string();
        assert!(verifier
            .verify(
                signature_payload(&changed_target).as_bytes(),
                signature.as_ref()
            )
            .is_err());
    }
}
