// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chrono::{SecondsFormat, TimeZone, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

const RUNTIME_GRANT_ISSUER: &str = "mcp-management-service";
const RUNTIME_GRANT_AUDIENCE: &str = "mcp-management-runtime";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeGrantClaims {
    pub iss: String,
    pub sub: String,
    pub aud: String,
    pub session_id: String,
    pub owner_user_id: String,
    pub agent_key: String,
    pub project_id: String,
    pub run_id: Option<String>,
    pub turn_id: Option<String>,
    pub task_id: Option<String>,
    pub source_session_id: Option<String>,
    pub source_user_message_id: Option<String>,
    pub default_model_config_id: Option<String>,
    pub expected_project_task_ids: Vec<String>,
    pub policy_revision: String,
    pub route_revision: String,
    pub allowed_resource_ids: Vec<String>,
    pub iat: usize,
    pub exp: usize,
}

#[derive(Debug, Clone)]
pub struct IssuedRuntimeGrant {
    pub token: String,
    pub expires_at: String,
    pub expires_at_unix: i64,
}

#[derive(Clone)]
pub struct RuntimeGrantService {
    secret: String,
    ttl: Duration,
}

impl RuntimeGrantService {
    pub fn new(secret: impl Into<String>, ttl: Duration) -> Self {
        Self {
            secret: secret.into(),
            ttl,
        }
    }

    pub fn issue(&self, mut claims: RuntimeGrantClaims) -> Result<IssuedRuntimeGrant, String> {
        let now = Utc::now().timestamp();
        let expires_at_unix = now.saturating_add(self.ttl.as_secs() as i64);
        claims.iss = RUNTIME_GRANT_ISSUER.to_string();
        claims.aud = RUNTIME_GRANT_AUDIENCE.to_string();
        claims.iat = usize::try_from(now).map_err(|_| "system clock is invalid".to_string())?;
        claims.exp = usize::try_from(expires_at_unix)
            .map_err(|_| "runtime grant expiry is invalid".to_string())?;
        let token = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(self.secret.as_bytes()),
        )
        .map_err(|err| format!("issue runtime grant failed: {err}"))?;
        let expires_at = Utc
            .timestamp_opt(expires_at_unix, 0)
            .single()
            .ok_or_else(|| "runtime grant expiry is invalid".to_string())?
            .to_rfc3339_opts(SecondsFormat::Secs, true);
        Ok(IssuedRuntimeGrant {
            token,
            expires_at,
            expires_at_unix,
        })
    }

    pub fn verify(&self, token: &str) -> Result<RuntimeGrantClaims, String> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_issuer(&[RUNTIME_GRANT_ISSUER]);
        validation.set_audience(&[RUNTIME_GRANT_AUDIENCE]);
        validation.leeway = 5;
        let claims = decode::<RuntimeGrantClaims>(
            token,
            &DecodingKey::from_secret(self.secret.as_bytes()),
            &validation,
        )
        .map_err(|err| format!("verify runtime grant failed: {err}"))?
        .claims;
        if claims.sub.trim().is_empty() {
            return Err("runtime grant caller is missing".to_string());
        }
        if claims.exp <= claims.iat {
            return Err("runtime grant lifetime is invalid".to_string());
        }
        Ok(claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claims() -> RuntimeGrantClaims {
        RuntimeGrantClaims {
            iss: String::new(),
            sub: "task-runner".to_string(),
            aud: String::new(),
            session_id: "session-1".to_string(),
            owner_user_id: "user-1".to_string(),
            agent_key: "task_runner_run_phase".to_string(),
            project_id: "project-1".to_string(),
            run_id: Some("run-1".to_string()),
            turn_id: None,
            task_id: Some("task-1".to_string()),
            source_session_id: None,
            source_user_message_id: None,
            default_model_config_id: None,
            expected_project_task_ids: Vec::new(),
            policy_revision: "policy-1".to_string(),
            route_revision: "route-1".to_string(),
            allowed_resource_ids: vec!["mcp-1".to_string()],
            iat: 0,
            exp: 0,
        }
    }

    #[test]
    fn grant_binds_session_owner_policy_and_routes() {
        let service =
            RuntimeGrantService::new("a-long-runtime-grant-secret", Duration::from_secs(900));
        let issued = service.issue(claims()).unwrap();
        let verified = service.verify(issued.token.as_str()).unwrap();
        assert_eq!(verified.session_id, "session-1");
        assert_eq!(verified.owner_user_id, "user-1");
        assert_eq!(verified.route_revision, "route-1");
        assert_eq!(verified.allowed_resource_ids, vec!["mcp-1"]);
    }
}
