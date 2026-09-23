// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::Arc;

use chatos_mcp_management_sdk::CloseRuntimeSessionResponse;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::types::Json;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeSessionCloseRecord {
    #[serde(rename = "_id")]
    session_id: String,
    caller_service: String,
    response: CloseRuntimeSessionResponse,
    expires_at: DateTime<Utc>,
    expires_at_unix: i64,
}

#[derive(Clone)]
pub struct RuntimeSessionCloseStore {
    backend: Arc<RuntimeSessionCloseStoreBackend>,
}

enum RuntimeSessionCloseStoreBackend {
    Memory(RwLock<HashMap<String, RuntimeSessionCloseRecord>>),
    Postgres(chatos_postgres::PgPool),
}

impl RuntimeSessionCloseStore {
    pub fn memory() -> Self {
        Self {
            backend: Arc::new(RuntimeSessionCloseStoreBackend::Memory(RwLock::new(
                HashMap::new(),
            ))),
        }
    }

    pub async fn connect(database_url: &str) -> Result<Self, String> {
        let pool = crate::postgres::connect(database_url).await?;
        Ok(Self::from_pool(pool))
    }

    pub(crate) fn from_pool(pool: chatos_postgres::PgPool) -> Self {
        Self {
            backend: Arc::new(RuntimeSessionCloseStoreBackend::Postgres(pool)),
        }
    }

    pub async fn get(
        &self,
        session_id: &str,
        caller_service: &str,
    ) -> Result<Option<CloseRuntimeSessionResponse>, String> {
        let now = chrono::Utc::now().timestamp();
        let record = match self.backend.as_ref() {
            RuntimeSessionCloseStoreBackend::Memory(records) => {
                let mut records = records.write().await;
                records.retain(|_, record| record.expires_at_unix > now);
                records.get(session_id).cloned()
            }
            RuntimeSessionCloseStoreBackend::Postgres(pool) => {
                sqlx::query_scalar::<_, Json<serde_json::Value>>(
                    "SELECT data FROM mcp_management_runtime_session_close_results \
                 WHERE session_id=$1 AND expires_at_unix>$2",
                )
                .bind(session_id)
                .bind(now)
                .fetch_optional(pool)
                .await
                .map_err(|error| format!("load Runtime Session close result failed: {error}"))?
                .map(|value| serde_json::from_value(value.0).map_err(|error| error.to_string()))
                .transpose()?
            }
        };
        let Some(record) = record else {
            return Ok(None);
        };
        if record.caller_service != caller_service {
            return Err(
                "runtime session close result belongs to another caller service".to_string(),
            );
        }
        Ok(Some(record.response))
    }

    pub async fn save(
        &self,
        caller_service: &str,
        response: CloseRuntimeSessionResponse,
        expires_at_unix: i64,
    ) -> Result<(), String> {
        let record = RuntimeSessionCloseRecord {
            session_id: response.session_id.clone(),
            caller_service: caller_service.to_string(),
            response,
            expires_at: DateTime::<Utc>::from_timestamp(expires_at_unix, 0).ok_or_else(|| {
                "Runtime Session close expiry is outside timestamp range".to_string()
            })?,
            expires_at_unix,
        };
        match self.backend.as_ref() {
            RuntimeSessionCloseStoreBackend::Memory(records) => {
                records
                    .write()
                    .await
                    .insert(record.session_id.clone(), record);
                Ok(())
            }
            RuntimeSessionCloseStoreBackend::Postgres(pool) => {
                let data = serde_json::to_value(&record)
                    .map(Json)
                    .map_err(|error| error.to_string())?;
                sqlx::query(
                    "INSERT INTO mcp_management_runtime_session_close_results \
                     (session_id,caller_service,expires_at,expires_at_unix,data,updated_at) \
                     VALUES($1,$2,$3,$4,$5,now()) ON CONFLICT(session_id) DO UPDATE SET \
                     caller_service=EXCLUDED.caller_service,expires_at=EXCLUDED.expires_at, \
                     expires_at_unix=EXCLUDED.expires_at_unix,data=EXCLUDED.data,updated_at=now()",
                )
                .bind(&record.session_id)
                .bind(&record.caller_service)
                .bind(record.expires_at)
                .bind(record.expires_at_unix)
                .bind(data)
                .execute(pool)
                .await
                .map(|_| ())
                .map_err(|error| format!("persist Runtime Session close result failed: {error}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn memory_store_replays_structured_close_response_for_same_caller() {
        let store = RuntimeSessionCloseStore::memory();
        let response = CloseRuntimeSessionResponse {
            session_id: "session-1".to_string(),
            closed: true,
            provider_finalization: None,
        };
        store
            .save(
                "task-runner",
                response.clone(),
                chrono::Utc::now().timestamp() + 300,
            )
            .await
            .unwrap();
        assert_eq!(
            store.get("session-1", "task-runner").await.unwrap(),
            Some(response)
        );
        assert!(store.get("session-1", "chatos").await.is_err());
    }
}
