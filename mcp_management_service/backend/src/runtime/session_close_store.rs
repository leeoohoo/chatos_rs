// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chatos_mcp_management_sdk::CloseRuntimeSessionResponse;
use mongodb::bson::{doc, DateTime};
use mongodb::options::{IndexOptions, ReplaceOptions};
use mongodb::{Client, Collection, IndexModel};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeSessionCloseRecord {
    #[serde(rename = "_id")]
    session_id: String,
    caller_service: String,
    response: CloseRuntimeSessionResponse,
    expires_at: DateTime,
    expires_at_unix: i64,
}

#[derive(Clone)]
pub struct RuntimeSessionCloseStore {
    backend: Arc<RuntimeSessionCloseStoreBackend>,
}

enum RuntimeSessionCloseStoreBackend {
    Memory(RwLock<HashMap<String, RuntimeSessionCloseRecord>>),
    Mongo(Collection<RuntimeSessionCloseRecord>),
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
        let client = Client::with_uri_str(database_url)
            .await
            .map_err(|error| format!("connect Runtime Session close MongoDB failed: {error}"))?;
        let database = client.default_database().ok_or_else(|| {
            "MCP_MANAGEMENT_DATABASE_URL must include a MongoDB database name".to_string()
        })?;
        let collection = database.collection::<RuntimeSessionCloseRecord>(
            "mcp_management_runtime_session_close_results",
        );
        collection
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "expires_at": 1 })
                    .options(
                        IndexOptions::builder()
                            .name("runtime_session_close_result_expiry_ttl".to_string())
                            .expire_after(Some(Duration::from_secs(0)))
                            .build(),
                    )
                    .build(),
                None,
            )
            .await
            .map_err(|error| {
                format!("initialize Runtime Session close result TTL index failed: {error}")
            })?;
        Ok(Self {
            backend: Arc::new(RuntimeSessionCloseStoreBackend::Mongo(collection)),
        })
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
            RuntimeSessionCloseStoreBackend::Mongo(collection) => collection
                .find_one(
                    doc! {
                        "_id": session_id,
                        "expires_at_unix": { "$gt": now },
                    },
                    None,
                )
                .await
                .map_err(|error| format!("load Runtime Session close result failed: {error}"))?,
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
            expires_at: DateTime::from_millis(expires_at_unix.saturating_mul(1_000)),
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
            RuntimeSessionCloseStoreBackend::Mongo(collection) => collection
                .replace_one(
                    doc! { "_id": record.session_id.as_str() },
                    record,
                    ReplaceOptions::builder().upsert(true).build(),
                )
                .await
                .map(|_| ())
                .map_err(|error| format!("persist Runtime Session close result failed: {error}")),
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
