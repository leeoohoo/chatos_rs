// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use jsonwebtoken::{
    decode, decode_header, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MEMORY_ENGINE_RUNTIME_CONTEXT_AUDIENCE: &str = "memory-engine";
pub const RUNTIME_CONTEXT_MAX_TTL_SECONDS: u64 = 300;

#[derive(Clone)]
pub struct RuntimeContextAssertionIssuer {
    secret: Arc<str>,
    key_id: Arc<str>,
    issuer: Arc<str>,
    runtime_identity_revision: Arc<str>,
    ttl_seconds: u64,
}

impl RuntimeContextAssertionIssuer {
    pub fn new(
        secret: impl Into<String>,
        key_id: impl Into<String>,
        issuer: impl Into<String>,
        runtime_identity_revision: impl Into<String>,
        ttl_seconds: u64,
    ) -> Result<Self, String> {
        let secret = secret.into();
        let key_id = key_id.into();
        let issuer = issuer.into();
        let runtime_identity_revision = runtime_identity_revision.into();
        validate_signing_text("secret", &secret)?;
        validate_signing_text("key_id", &key_id)?;
        validate_signing_text("issuer", &issuer)?;
        validate_signing_text("runtime_identity_revision", &runtime_identity_revision)?;
        if !(5..=RUNTIME_CONTEXT_MAX_TTL_SECONDS).contains(&ttl_seconds) {
            return Err(format!(
                "runtime context assertion ttl_seconds must be between 5 and {RUNTIME_CONTEXT_MAX_TTL_SECONDS}"
            ));
        }
        Ok(Self {
            secret: Arc::from(secret),
            key_id: Arc::from(key_id),
            issuer: Arc::from(issuer),
            runtime_identity_revision: Arc::from(runtime_identity_revision),
            ttl_seconds,
        })
    }

    pub fn issue(
        &self,
        principal_id: &str,
        tenant_id: &str,
        source_id: &str,
        canonical_agent_subject_id: &str,
        thread_id: &str,
        project_partition_key: &str,
        allowed_operations: &[&str],
    ) -> Result<String, String> {
        issue_runtime_context_assertion(IssueRuntimeContextAssertion {
            secret: &self.secret,
            key_id: &self.key_id,
            issuer: &self.issuer,
            principal_id,
            tenant_id,
            source_id,
            canonical_agent_subject_id,
            thread_id,
            project_partition_key,
            allowed_operations,
            runtime_identity_revision: &self.runtime_identity_revision,
            ttl_seconds: self.ttl_seconds,
        })
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    pub fn runtime_identity_revision(&self) -> &str {
        &self.runtime_identity_revision
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeContextAssertionClaims {
    pub iss: String,
    pub sub: String,
    pub aud: String,
    pub principal_id: String,
    pub tenant_id: String,
    pub source_id: String,
    pub canonical_agent_subject_id: String,
    pub thread_id: String,
    pub project_partition_key: String,
    pub allowed_operations: Vec<String>,
    pub runtime_identity_revision: String,
    pub iat: usize,
    pub exp: usize,
    pub jti: String,
}

#[derive(Debug, Clone)]
pub struct IssueRuntimeContextAssertion<'a> {
    pub secret: &'a str,
    pub key_id: &'a str,
    pub issuer: &'a str,
    pub principal_id: &'a str,
    pub tenant_id: &'a str,
    pub source_id: &'a str,
    pub canonical_agent_subject_id: &'a str,
    pub thread_id: &'a str,
    pub project_partition_key: &'a str,
    pub allowed_operations: &'a [&'a str],
    pub runtime_identity_revision: &'a str,
    pub ttl_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct ExpectedRuntimeContext<'a> {
    pub audience: &'a str,
    pub tenant_id: &'a str,
    pub source_id: &'a str,
    pub thread_id: &'a str,
    pub project_partition_key: &'a str,
    pub operation: &'a str,
}

pub fn issue_runtime_context_assertion(
    input: IssueRuntimeContextAssertion<'_>,
) -> Result<String, String> {
    ensure_crypto_provider();
    validate_signing_text("secret", input.secret)?;
    validate_signing_text("key_id", input.key_id)?;
    validate_signing_text("issuer", input.issuer)?;
    validate_signing_text("principal_id", input.principal_id)?;
    validate_signing_text("tenant_id", input.tenant_id)?;
    validate_signing_text("source_id", input.source_id)?;
    validate_agent_subject(input.canonical_agent_subject_id)?;
    validate_signing_text("thread_id", input.thread_id)?;
    validate_partition(input.project_partition_key)?;
    validate_signing_text("runtime_identity_revision", input.runtime_identity_revision)?;
    if input.allowed_operations.is_empty() {
        return Err("runtime context assertion requires allowed_operations".to_string());
    }
    let mut allowed_operations = input
        .allowed_operations
        .iter()
        .map(|operation| {
            validate_operation(operation)?;
            Ok((*operation).to_string())
        })
        .collect::<Result<Vec<_>, String>>()?;
    allowed_operations.sort();
    allowed_operations.dedup();
    let now = unix_timestamp()?;
    let ttl = input.ttl_seconds.clamp(5, RUNTIME_CONTEXT_MAX_TTL_SECONDS) as usize;
    let claims = RuntimeContextAssertionClaims {
        iss: input.issuer.to_string(),
        sub: input.principal_id.to_string(),
        aud: MEMORY_ENGINE_RUNTIME_CONTEXT_AUDIENCE.to_string(),
        principal_id: input.principal_id.to_string(),
        tenant_id: input.tenant_id.to_string(),
        source_id: input.source_id.to_string(),
        canonical_agent_subject_id: input.canonical_agent_subject_id.to_string(),
        thread_id: input.thread_id.to_string(),
        project_partition_key: input.project_partition_key.to_string(),
        allowed_operations,
        runtime_identity_revision: input.runtime_identity_revision.to_string(),
        iat: now,
        exp: now.saturating_add(ttl),
        jti: Uuid::new_v4().to_string(),
    };
    let mut header = Header::new(Algorithm::HS256);
    header.kid = Some(input.key_id.to_string());
    encode(
        &header,
        &claims,
        &EncodingKey::from_secret(input.secret.as_bytes()),
    )
    .map_err(|error| format!("issue runtime context assertion failed: {error}"))
}

pub fn runtime_context_assertion_key_id(token: &str) -> Result<String, String> {
    ensure_crypto_provider();
    decode_header(token)
        .map_err(|error| format!("decode runtime context assertion header failed: {error}"))?
        .kid
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "runtime context assertion key_id is missing".to_string())
}

pub fn verify_runtime_context_assertion(
    token: &str,
    secret: &str,
    expected_issuer: &str,
    expected: ExpectedRuntimeContext<'_>,
) -> Result<RuntimeContextAssertionClaims, String> {
    ensure_crypto_provider();
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&[expected_issuer]);
    validation.set_audience(&[expected.audience]);
    validation.leeway = 5;
    let claims = decode::<RuntimeContextAssertionClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(|error| format!("verify runtime context assertion failed: {error}"))?
    .claims;
    if claims.sub != claims.principal_id {
        return Err("runtime context assertion subject does not match principal".to_string());
    }
    if claims.tenant_id != expected.tenant_id
        || claims.source_id != expected.source_id
        || claims.thread_id != expected.thread_id
        || claims.project_partition_key != expected.project_partition_key
    {
        return Err("runtime context assertion scope does not match request".to_string());
    }
    if !claims
        .allowed_operations
        .iter()
        .any(|operation| operation == expected.operation)
    {
        return Err("runtime context assertion operation is not allowed".to_string());
    }
    validate_agent_subject(&claims.canonical_agent_subject_id)?;
    validate_partition(&claims.project_partition_key)?;
    validate_signing_text(
        "runtime_identity_revision",
        &claims.runtime_identity_revision,
    )?;
    Uuid::parse_str(&claims.jti)
        .map_err(|_| "runtime context assertion jti is invalid".to_string())?;
    let now = unix_timestamp()?;
    if claims.iat > now.saturating_add(5) {
        return Err("runtime context assertion was issued in the future".to_string());
    }
    if claims.exp <= claims.iat
        || claims.exp.saturating_sub(claims.iat) > RUNTIME_CONTEXT_MAX_TTL_SECONDS as usize
    {
        return Err("runtime context assertion lifetime is invalid".to_string());
    }
    Ok(claims)
}

fn validate_agent_subject(value: &str) -> Result<(), String> {
    if value
        .strip_prefix("agent:")
        .is_none_or(|agent| agent.trim().is_empty() || agent.chars().any(char::is_control))
    {
        return Err("runtime context assertion canonical agent subject is invalid".to_string());
    }
    Ok(())
}

fn validate_partition(value: &str) -> Result<(), String> {
    if value == "__none__" {
        return Ok(());
    }
    if value
        .strip_prefix("project:")
        .is_none_or(|project| project.trim().is_empty() || project.chars().any(char::is_control))
    {
        return Err("runtime context assertion project partition is invalid".to_string());
    }
    Ok(())
}

fn validate_operation(value: &str) -> Result<(), String> {
    if !matches!(value, "record_ingest" | "compose" | "summarize" | "repair") {
        return Err(format!("unsupported runtime context operation: {value}"));
    }
    Ok(())
}

fn validate_signing_text(field: &str, value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(format!("runtime context assertion {field} is invalid"));
    }
    Ok(())
}

fn unix_timestamp() -> Result<usize, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs() as usize)
        .map_err(|error| format!("system clock is before UNIX epoch: {error}"))
}

fn ensure_crypto_provider() {
    let _ = jsonwebtoken::crypto::aws_lc::DEFAULT_PROVIDER.install_default();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded_claims(audience: &str, issued_at: usize, expires_at: usize) -> String {
        ensure_crypto_provider();
        let claims = RuntimeContextAssertionClaims {
            iss: "local-agent-host".to_string(),
            sub: "user-1".to_string(),
            aud: audience.to_string(),
            principal_id: "user-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            source_id: "local-agent-host".to_string(),
            canonical_agent_subject_id: "agent:a-1".to_string(),
            thread_id: "thread-1".to_string(),
            project_partition_key: "project:p-1".to_string(),
            allowed_operations: vec!["compose".to_string()],
            runtime_identity_revision: "runtime-identity-v1".to_string(),
            iat: issued_at,
            exp: expires_at,
            jti: Uuid::new_v4().to_string(),
        };
        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some("local-host-2026-11".to_string());
        encode(
            &header,
            &claims,
            &EncodingKey::from_secret(b"a-long-runtime-context-test-secret"),
        )
        .expect("encode test assertion")
    }

    fn expected() -> ExpectedRuntimeContext<'static> {
        ExpectedRuntimeContext {
            audience: MEMORY_ENGINE_RUNTIME_CONTEXT_AUDIENCE,
            tenant_id: "tenant-1",
            source_id: "local-agent-host",
            thread_id: "thread-1",
            project_partition_key: "project:p-1",
            operation: "compose",
        }
    }

    #[test]
    fn assertion_binds_scope_operation_audience_and_key_id() {
        let token = issue_runtime_context_assertion(IssueRuntimeContextAssertion {
            secret: "a-long-runtime-context-test-secret",
            key_id: "local-host-2026-11",
            issuer: "local-agent-host",
            principal_id: "user-1",
            tenant_id: "tenant-1",
            source_id: "local-agent-host",
            canonical_agent_subject_id: "agent:a-1",
            thread_id: "thread-1",
            project_partition_key: "project:p-1",
            allowed_operations: &["record_ingest", "compose"],
            runtime_identity_revision: "runtime-identity-v1",
            ttl_seconds: 60,
        })
        .expect("issue assertion");
        assert_eq!(
            runtime_context_assertion_key_id(&token).expect("key id"),
            "local-host-2026-11"
        );
        let claims = verify_runtime_context_assertion(
            &token,
            "a-long-runtime-context-test-secret",
            "local-agent-host",
            ExpectedRuntimeContext {
                audience: MEMORY_ENGINE_RUNTIME_CONTEXT_AUDIENCE,
                tenant_id: "tenant-1",
                source_id: "local-agent-host",
                thread_id: "thread-1",
                project_partition_key: "project:p-1",
                operation: "compose",
            },
        )
        .expect("verify assertion");
        assert_eq!(claims.canonical_agent_subject_id, "agent:a-1");
        assert!(verify_runtime_context_assertion(
            &token,
            "a-long-runtime-context-test-secret",
            "local-agent-host",
            ExpectedRuntimeContext {
                audience: MEMORY_ENGINE_RUNTIME_CONTEXT_AUDIENCE,
                tenant_id: "tenant-1",
                source_id: "local-agent-host",
                thread_id: "thread-1",
                project_partition_key: "project:other",
                operation: "compose",
            },
        )
        .is_err());
    }

    #[test]
    fn assertion_rejects_expired_and_wrong_audience_tokens() {
        let now = unix_timestamp().expect("clock");
        let expired = encoded_claims(
            MEMORY_ENGINE_RUNTIME_CONTEXT_AUDIENCE,
            now.saturating_sub(30),
            now.saturating_sub(10),
        );
        assert!(verify_runtime_context_assertion(
            &expired,
            "a-long-runtime-context-test-secret",
            "local-agent-host",
            expected(),
        )
        .is_err());

        let wrong_audience = encoded_claims("other-service", now, now + 60);
        assert!(verify_runtime_context_assertion(
            &wrong_audience,
            "a-long-runtime-context-test-secret",
            "local-agent-host",
            expected(),
        )
        .is_err());
    }
}
