// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::{SystemTime, UNIX_EPOCH};

use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InternalServiceTokenClaims {
    pub iss: String,
    pub sub: String,
    pub caller: String,
    pub aud: String,
    pub scope: String,
    pub trace_id: String,
    pub iat: usize,
    pub exp: usize,
}

pub fn issue_internal_service_token(
    secret: &str,
    issuer: &str,
    audience: &str,
    scope: &str,
    ttl_seconds: u64,
) -> Result<String, String> {
    let trace_id = Uuid::new_v4().to_string();
    issue_internal_service_token_with_trace_id(
        secret,
        issuer,
        audience,
        scope,
        ttl_seconds,
        trace_id.as_str(),
    )
}

pub fn issue_internal_service_token_with_trace_id(
    secret: &str,
    issuer: &str,
    audience: &str,
    scope: &str,
    ttl_seconds: u64,
    trace_id: &str,
) -> Result<String, String> {
    ensure_crypto_provider();
    let trace_id = Uuid::parse_str(trace_id.trim())
        .map_err(|_| "internal service token trace id must be a UUID".to_string())?
        .to_string();
    let now = unix_timestamp()?;
    let ttl_seconds = ttl_seconds.clamp(5, 300) as usize;
    let claims = InternalServiceTokenClaims {
        iss: issuer.to_string(),
        sub: issuer.to_string(),
        caller: issuer.to_string(),
        aud: audience.to_string(),
        scope: scope.to_string(),
        trace_id,
        iat: now,
        exp: now.saturating_add(ttl_seconds),
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|err| format!("issue internal service token failed: {err}"))
}

pub fn verify_internal_service_token(
    token: &str,
    secret: &str,
    expected_issuer: &str,
    expected_audience: &str,
    expected_scope: &str,
) -> Result<InternalServiceTokenClaims, String> {
    ensure_crypto_provider();
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&[expected_issuer]);
    validation.set_audience(&[expected_audience]);
    validation.leeway = 5;
    let claims = decode::<InternalServiceTokenClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(|err| format!("verify internal service token failed: {err}"))?
    .claims;
    if claims.sub != expected_issuer || claims.caller != expected_issuer {
        return Err("internal service token subject does not match caller".to_string());
    }
    if claims.scope != expected_scope {
        return Err("internal service token scope is not allowed".to_string());
    }
    Uuid::parse_str(claims.trace_id.as_str())
        .map_err(|_| "internal service token trace id is invalid".to_string())?;
    let now = unix_timestamp()?;
    if claims.iat > now.saturating_add(5) {
        return Err("internal service token was issued in the future".to_string());
    }
    if claims.exp <= claims.iat || claims.exp.saturating_sub(claims.iat) > 300 {
        return Err("internal service token lifetime is invalid".to_string());
    }
    Ok(claims)
}

fn ensure_crypto_provider() {
    let _ = jsonwebtoken::crypto::rust_crypto::DEFAULT_PROVIDER.install_default();
}

fn unix_timestamp() -> Result<usize, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs() as usize)
        .map_err(|err| format!("system clock is before UNIX epoch: {err}"))
}

#[cfg(test)]
mod tests {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    use serde::Serialize;
    use uuid::Uuid;

    use super::{
        issue_internal_service_token, issue_internal_service_token_with_trace_id, unix_timestamp,
        verify_internal_service_token,
    };

    #[derive(Serialize)]
    struct LegacyInternalServiceTokenClaims {
        iss: String,
        sub: String,
        aud: String,
        scope: String,
        iat: usize,
        exp: usize,
    }

    #[test]
    fn signed_token_binds_issuer_audience_scope_and_expiry() {
        let token = issue_internal_service_token(
            "a-long-test-internal-secret",
            "task-runner",
            "plugin-management-service",
            "capabilities.resolve",
            60,
        )
        .expect("issue token");
        let claims = verify_internal_service_token(
            token.as_str(),
            "a-long-test-internal-secret",
            "task-runner",
            "plugin-management-service",
            "capabilities.resolve",
        )
        .expect("verify token");
        assert_eq!(claims.sub, "task-runner");
        assert_eq!(claims.caller, "task-runner");
        Uuid::parse_str(claims.trace_id.as_str()).expect("valid trace id");
        assert!(claims.exp > claims.iat);
        assert!(verify_internal_service_token(
            token.as_str(),
            "a-long-test-internal-secret",
            "task-runner",
            "another-service",
            "capabilities.resolve",
        )
        .is_err());
        assert!(verify_internal_service_token(
            token.as_str(),
            "a-long-test-internal-secret",
            "task-runner",
            "plugin-management-service",
            "local-connector.write",
        )
        .is_err());
    }

    #[test]
    fn legacy_token_without_caller_and_trace_id_is_rejected() {
        let now = unix_timestamp().expect("current timestamp");
        let token = encode(
            &Header::new(Algorithm::HS256),
            &LegacyInternalServiceTokenClaims {
                iss: "task-runner".to_string(),
                sub: "task-runner".to_string(),
                aud: "plugin-management-service".to_string(),
                scope: "capabilities.resolve".to_string(),
                iat: now,
                exp: now + 60,
            },
            &EncodingKey::from_secret(b"a-long-test-internal-secret"),
        )
        .expect("issue legacy token");

        assert!(verify_internal_service_token(
            token.as_str(),
            "a-long-test-internal-secret",
            "task-runner",
            "plugin-management-service",
            "capabilities.resolve",
        )
        .is_err());
    }

    #[test]
    fn explicit_trace_id_is_signed_and_invalid_trace_id_is_rejected() {
        let trace_id = Uuid::new_v4().to_string();
        let token = issue_internal_service_token_with_trace_id(
            "a-long-test-internal-secret",
            "configuration-center",
            "mcp-management-service",
            "queue.dead_letter.archive",
            60,
            trace_id.as_str(),
        )
        .expect("issue operation-bound token");
        let claims = verify_internal_service_token(
            token.as_str(),
            "a-long-test-internal-secret",
            "configuration-center",
            "mcp-management-service",
            "queue.dead_letter.archive",
        )
        .expect("verify operation-bound token");
        assert_eq!(claims.trace_id, trace_id);
        assert!(issue_internal_service_token_with_trace_id(
            "a-long-test-internal-secret",
            "configuration-center",
            "mcp-management-service",
            "queue.dead_letter.archive",
            60,
            "not-a-uuid",
        )
        .is_err());
    }
}
