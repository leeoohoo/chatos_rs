// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::Arc;

use redis::aio::ConnectionManager;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use super::RuntimeInvocationRecord;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RuntimeInvocationQuotaLimits {
    pub tenant: u32,
    pub user: u32,
    pub project: u32,
    pub device: u32,
}

impl RuntimeInvocationQuotaLimits {
    pub fn new(tenant: u32, user: u32, project: u32, device: u32) -> Result<Self, String> {
        let limits = Self {
            tenant,
            user,
            project,
            device,
        };
        for (dimension, limit) in limits.dimensions() {
            if !(1..=1_000_000).contains(&limit) {
                return Err(format!(
                    "Runtime Invocation {dimension} quota must be between 1 and 1000000"
                ));
            }
        }
        Ok(limits)
    }

    fn dimensions(self) -> [(&'static str, u32); 4] {
        [
            ("tenant", self.tenant),
            ("user", self.user),
            ("project", self.project),
            ("device", self.device),
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeInvocationQuotaReserveError {
    CapacityExhausted { dimension: &'static str, limit: u32 },
    Infrastructure(String),
}

#[derive(Clone)]
pub struct RuntimeInvocationQuota {
    backend: Arc<RuntimeInvocationQuotaBackend>,
    limits: RuntimeInvocationQuotaLimits,
    key_prefix: String,
}

#[cfg_attr(not(test), allow(dead_code))]
enum RuntimeInvocationQuotaBackend {
    Valkey(ConnectionManager),
    Memory(Mutex<HashMap<String, HashMap<String, i64>>>),
}

struct QuotaScope {
    dimension: &'static str,
    key: String,
    limit: u32,
}

impl RuntimeInvocationQuota {
    pub fn limits(&self) -> RuntimeInvocationQuotaLimits {
        self.limits
    }

    pub async fn connect(
        valkey_url: &str,
        key_prefix: &str,
        limits: RuntimeInvocationQuotaLimits,
    ) -> Result<Self, String> {
        let key_prefix = normalize_key_prefix(key_prefix)?;
        let client = redis::Client::open(valkey_url)
            .map_err(|error| format!("parse MCP invocation quota Valkey URL failed: {error}"))?;
        let connection = client
            .get_connection_manager()
            .await
            .map_err(|error| format!("connect MCP invocation quota Valkey failed: {error}"))?;
        Ok(Self {
            backend: Arc::new(RuntimeInvocationQuotaBackend::Valkey(connection)),
            limits,
            key_prefix,
        })
    }

    #[cfg(test)]
    pub fn memory(limits: RuntimeInvocationQuotaLimits) -> Self {
        Self {
            backend: Arc::new(RuntimeInvocationQuotaBackend::Memory(Mutex::new(
                HashMap::new(),
            ))),
            limits,
            key_prefix: "test:mcp-invocation-quota".to_string(),
        }
    }

    pub async fn reserve(
        &self,
        record: &RuntimeInvocationRecord,
    ) -> Result<(), RuntimeInvocationQuotaReserveError> {
        let scopes = self
            .scopes(record)
            .map_err(RuntimeInvocationQuotaReserveError::Infrastructure)?;
        let now_ms = chrono::Utc::now().timestamp_millis();
        let expires_at_ms = record.expires_at_unix.saturating_mul(1_000);
        match self.backend.as_ref() {
            RuntimeInvocationQuotaBackend::Valkey(connection) => {
                reserve_valkey(
                    connection.clone(),
                    scopes.as_slice(),
                    record.invocation_id.as_str(),
                    now_ms,
                    expires_at_ms,
                )
                .await
            }
            RuntimeInvocationQuotaBackend::Memory(entries) => {
                let mut entries = entries.lock().await;
                for scope in &scopes {
                    entries
                        .entry(scope.key.clone())
                        .or_default()
                        .retain(|_, expiry| *expiry > now_ms);
                }
                for scope in &scopes {
                    let active = entries.get(scope.key.as_str()).map_or(0, HashMap::len);
                    let already_reserved = entries
                        .get(scope.key.as_str())
                        .is_some_and(|values| values.contains_key(record.invocation_id.as_str()));
                    if !already_reserved && active >= scope.limit as usize {
                        return Err(RuntimeInvocationQuotaReserveError::CapacityExhausted {
                            dimension: scope.dimension,
                            limit: scope.limit,
                        });
                    }
                }
                for scope in scopes {
                    entries
                        .entry(scope.key)
                        .or_default()
                        .insert(record.invocation_id.clone(), expires_at_ms);
                }
                Ok(())
            }
        }
    }

    pub async fn release(&self, record: &RuntimeInvocationRecord) -> Result<(), String> {
        let scopes = self.scopes(record)?;
        match self.backend.as_ref() {
            RuntimeInvocationQuotaBackend::Valkey(connection) => {
                let script = redis::Script::new(
                    "for i = 1, #KEYS do redis.call('ZREM', KEYS[i], ARGV[1]) end return 1",
                );
                let mut invocation = script.prepare_invoke();
                for scope in &scopes {
                    invocation.key(scope.key.as_str());
                }
                invocation.arg(record.invocation_id.as_str());
                let mut connection = connection.clone();
                invocation
                    .invoke_async::<i64>(&mut connection)
                    .await
                    .map(|_| ())
                    .map_err(|error| format!("release MCP invocation quota failed: {error}"))
            }
            RuntimeInvocationQuotaBackend::Memory(entries) => {
                let mut entries = entries.lock().await;
                for scope in scopes {
                    let remove_scope = if let Some(values) = entries.get_mut(scope.key.as_str()) {
                        values.remove(record.invocation_id.as_str());
                        values.is_empty()
                    } else {
                        false
                    };
                    if remove_scope {
                        entries.remove(scope.key.as_str());
                    }
                }
                Ok(())
            }
        }
    }

    fn scopes(&self, record: &RuntimeInvocationRecord) -> Result<Vec<QuotaScope>, String> {
        let mut scopes = vec![
            self.scope("tenant", record.tenant_id.as_str(), self.limits.tenant)?,
            self.scope("user", record.owner_user_id.as_str(), self.limits.user)?,
        ];
        if let Some(project_id) = record.project_id.as_deref() {
            scopes.push(self.scope("project", project_id, self.limits.project)?);
        }
        if let Some(device_id) = record.device_id.as_deref() {
            scopes.push(self.scope("device", device_id, self.limits.device)?);
        }
        Ok(scopes)
    }

    fn scope(&self, dimension: &'static str, id: &str, limit: u32) -> Result<QuotaScope, String> {
        let id = id.trim();
        if id.is_empty() {
            return Err(format!(
                "Runtime Invocation {dimension} identity is required"
            ));
        }
        let digest = Sha256::digest(id.as_bytes());
        Ok(QuotaScope {
            dimension,
            key: format!(
                "{}:{{runtime-invocation-quota}}:{dimension}:{}",
                self.key_prefix,
                hex::encode(digest)
            ),
            limit,
        })
    }
}

async fn reserve_valkey(
    mut connection: ConnectionManager,
    scopes: &[QuotaScope],
    invocation_id: &str,
    now_ms: i64,
    expires_at_ms: i64,
) -> Result<(), RuntimeInvocationQuotaReserveError> {
    let script = redis::Script::new(
        "for i = 1, #KEYS do redis.call('ZREMRANGEBYSCORE', KEYS[i], '-inf', ARGV[1]) end for i = 1, #KEYS do if not redis.call('ZSCORE', KEYS[i], ARGV[3]) and redis.call('ZCARD', KEYS[i]) >= tonumber(ARGV[3 + i]) then return i end end for i = 1, #KEYS do redis.call('ZADD', KEYS[i], ARGV[2], ARGV[3]); redis.call('PEXPIREAT', KEYS[i], ARGV[4 + #KEYS]) end return 0",
    );
    let mut invocation = script.prepare_invoke();
    for scope in scopes {
        invocation.key(scope.key.as_str());
    }
    invocation.arg(now_ms).arg(expires_at_ms).arg(invocation_id);
    for scope in scopes {
        invocation.arg(scope.limit);
    }
    invocation.arg(expires_at_ms.saturating_add(60_000));
    let rejected_dimension = invocation
        .invoke_async::<i64>(&mut connection)
        .await
        .map_err(|error| {
            RuntimeInvocationQuotaReserveError::Infrastructure(format!(
                "reserve MCP invocation quota failed: {error}"
            ))
        })?;
    if rejected_dimension == 0 {
        return Ok(());
    }
    let scope = scopes
        .get(rejected_dimension.saturating_sub(1) as usize)
        .ok_or_else(|| {
            RuntimeInvocationQuotaReserveError::Infrastructure(
                "MCP invocation quota script returned an invalid dimension".to_string(),
            )
        })?;
    Err(RuntimeInvocationQuotaReserveError::CapacityExhausted {
        dimension: scope.dimension,
        limit: scope.limit,
    })
}

fn normalize_key_prefix(value: &str) -> Result<String, String> {
    let value = value.trim().trim_end_matches(':');
    if value.is_empty() {
        return Err("MCP invocation quota key prefix is required".to_string());
    }
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use mongodb::bson::DateTime;

    use super::*;
    use crate::runtime::RuntimeInvocationStatus;

    fn record(id: &str) -> RuntimeInvocationRecord {
        let expires_at_unix = chrono::Utc::now().timestamp() + 300;
        RuntimeInvocationRecord {
            invocation_id: id.to_string(),
            session_id: format!("session-{id}"),
            request_id_key: format!("\"request-{id}\""),
            caller_service: "task-runner".to_string(),
            tenant_id: "tenant-1".to_string(),
            owner_user_id: "user-1".to_string(),
            project_id: Some("project-1".to_string()),
            device_id: Some("device-1".to_string()),
            resource_id: "mcp-1".to_string(),
            exposed_tool_name: "demo".to_string(),
            original_tool_name: "demo".to_string(),
            mutation_may_have_started: false,
            cancel_supported: true,
            status: RuntimeInvocationStatus::Queued,
            created_at_unix_ms: chrono::Utc::now().timestamp_millis(),
            started_at_unix_ms: None,
            completed_at_unix_ms: None,
            terminal_result: None,
            terminal_error_code: None,
            terminal_error_message: None,
            file_modification_outcome: None,
            expires_at: DateTime::from_millis(expires_at_unix * 1_000),
            expires_at_unix,
        }
    }

    #[tokio::test]
    async fn memory_quota_reserves_all_dimensions_and_releases_together() {
        let quota =
            RuntimeInvocationQuota::memory(RuntimeInvocationQuotaLimits::new(1, 1, 1, 1).unwrap());
        let first = record("one");
        quota.reserve(&first).await.unwrap();
        let error = quota.reserve(&record("two")).await.unwrap_err();
        assert_eq!(
            error,
            RuntimeInvocationQuotaReserveError::CapacityExhausted {
                dimension: "tenant",
                limit: 1,
            }
        );
        quota.release(&first).await.unwrap();
        quota.reserve(&record("two")).await.unwrap();
    }
}
