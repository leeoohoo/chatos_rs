// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sha2::{Digest, Sha256};

use crate::config::AppConfig;
use crate::db::Db;

#[derive(Debug, Clone, Copy)]
pub struct ExpectedRuntimeContextScope<'a> {
    pub tenant_id: &'a str,
    pub source_id: &'a str,
    pub thread_id: &'a str,
    pub project_partition_key: &'a str,
    pub operation: &'a str,
}

pub async fn verify_and_consume(
    db: &Db,
    config: &AppConfig,
    token: &str,
    expected: ExpectedRuntimeContextScope<'_>,
) -> Result<memory_engine_sdk::RuntimeContextAssertionClaims, String> {
    let token = token.trim();
    if token.is_empty() {
        return Err("runtime_context_assertion is required".to_string());
    }
    let key_id = memory_engine_sdk::runtime_context_assertion_key_id(token)?;
    let key = config
        .runtime_context_assertion_keys
        .get(&key_id)
        .ok_or_else(|| "runtime context assertion key_id is not trusted".to_string())?;
    let claims = memory_engine_sdk::verify_runtime_context_assertion(
        token,
        key.secret.as_str(),
        key.issuer.as_str(),
        memory_engine_sdk::ExpectedRuntimeContext {
            audience: memory_engine_sdk::MEMORY_ENGINE_RUNTIME_CONTEXT_AUDIENCE,
            tenant_id: expected.tenant_id,
            source_id: expected.source_id,
            thread_id: expected.thread_id,
            project_partition_key: expected.project_partition_key,
            operation: expected.operation,
        },
    )?;
    let thread_subject_id = sqlx::query_scalar::<_, String>(
        "SELECT subject_id FROM engine_threads WHERE tenant_id=$1 AND source_id=$2 AND id=$3 \
         AND recall_eligible",
    )
    .bind(expected.tenant_id)
    .bind(expected.source_id)
    .bind(expected.thread_id)
    .fetch_optional(db)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "runtime context assertion thread does not exist".to_string())?;
    if thread_subject_id != claims.canonical_agent_subject_id {
        return Err("runtime context assertion agent does not match thread identity".to_string());
    }
    consume_replay_guard(db, &key_id, &claims, expected).await?;
    Ok(claims)
}

async fn consume_replay_guard(
    db: &Db,
    key_id: &str,
    claims: &memory_engine_sdk::RuntimeContextAssertionClaims,
    expected: ExpectedRuntimeContextScope<'_>,
) -> Result<(), String> {
    let expires_at = chrono::DateTime::from_timestamp(claims.exp as i64, 0)
        .ok_or_else(|| "runtime context assertion expiry is invalid".to_string())?;
    let jti_hash = sha256(claims.jti.as_bytes());
    let scope_hash = sha256(
        format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
            expected.tenant_id,
            expected.source_id,
            expected.thread_id,
            expected.project_partition_key,
            expected.operation
        )
        .as_bytes(),
    );
    let mut tx = db.begin().await.map_err(|error| error.to_string())?;
    sqlx::query("DELETE FROM engine_runtime_context_assertion_replays WHERE expires_at<now()")
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    let inserted = sqlx::query(
        "INSERT INTO engine_runtime_context_assertion_replays( \
           jti_hash,scope_hash,issuer,key_id,operation,expires_at) \
         VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(jti_hash) DO NOTHING",
    )
    .bind(jti_hash)
    .bind(scope_hash)
    .bind(&claims.iss)
    .bind(key_id)
    .bind(expected.operation)
    .bind(expires_at)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?
    .rows_affected();
    if inserted != 1 {
        tx.rollback().await.map_err(|error| error.to_string())?;
        return Err("runtime context assertion replay was rejected".to_string());
    }
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(())
}

fn sha256(value: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(value)))
}

#[cfg(test)]
#[path = "../repositories/postgres_contract_tests/runtime_context_assertion_contracts.rs"]
mod contract_tests;
