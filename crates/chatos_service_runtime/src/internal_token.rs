// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::{SystemTime, UNIX_EPOCH};

use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InternalServiceTokenClaims {
    pub iss: String,
    pub sub: String,
    pub aud: String,
    pub scope: String,
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
    let now = unix_timestamp()?;
    let ttl_seconds = ttl_seconds.clamp(5, 300) as usize;
    let claims = InternalServiceTokenClaims {
        iss: issuer.to_string(),
        sub: issuer.to_string(),
        aud: audience.to_string(),
        scope: scope.to_string(),
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
    if claims.sub != expected_issuer {
        return Err("internal service token subject does not match caller".to_string());
    }
    if claims.scope != expected_scope {
        return Err("internal service token scope is not allowed".to_string());
    }
    let now = unix_timestamp()?;
    if claims.iat > now.saturating_add(5) {
        return Err("internal service token was issued in the future".to_string());
    }
    if claims.exp <= claims.iat || claims.exp.saturating_sub(claims.iat) > 300 {
        return Err("internal service token lifetime is invalid".to_string());
    }
    Ok(claims)
}

fn unix_timestamp() -> Result<usize, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs() as usize)
        .map_err(|err| format!("system clock is before UNIX epoch: {err}"))
}

#[cfg(test)]
mod tests {
    use super::{issue_internal_service_token, verify_internal_service_token};

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
}
