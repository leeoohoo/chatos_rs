// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_ai_runtime::{
    AiRuntime, ContextualTurnRunner, MemoryContextComposer, MemoryEngineRecordWriter,
    MemoryRecordScope, MemoryRecordWriter,
};
use chatos_local_agent_ports::{
    AuthoritativeConversationPage, AuthoritativeConversationRecord,
    LocalConversationTranscriptSource, LocalMemoryOutboxStore,
};
use memory_engine_sdk::{MemoryEngineClient, SdkListThreadRecordsRequest};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};

use crate::{LocalMemoryOutboxWriter, LocalMemorySyncWorker};

pub struct LocalMemoryRuntimeServices {
    pub runner: ContextualTurnRunner,
    pub sync_worker: LocalMemorySyncWorker,
    pub transcript_source: Arc<dyn LocalConversationTranscriptSource>,
    pub source_id: String,
}

#[derive(Clone)]
struct MemoryConversationTranscriptSource {
    client: MemoryEngineClient,
}

#[derive(Debug, Serialize, Deserialize)]
struct TranscriptCursor {
    created_at: String,
    record_id: String,
}

#[async_trait]
impl LocalConversationTranscriptSource for MemoryConversationTranscriptSource {
    async fn list_conversation_records(
        &self,
        tenant_id: &str,
        thread_id: &str,
        conversation_id: &str,
        before_cursor: Option<&str>,
        limit: u32,
    ) -> Result<AuthoritativeConversationPage, String> {
        let cursor = before_cursor
            .map(serde_json::from_str::<TranscriptCursor>)
            .transpose()
            .map_err(|error| format!("Memory transcript cursor is invalid: {error}"))?;
        let page = self
            .client
            .list_thread_records_page(
                thread_id,
                &SdkListThreadRecordsRequest {
                    tenant_id: tenant_id.to_string(),
                    conversation_id: Some(conversation_id.to_string()),
                    role: None,
                    record_type: Some("message".to_string()),
                    summary_status: None,
                    after_created_at: cursor.as_ref().map(|value| value.created_at.clone()),
                    after_id: cursor.as_ref().map(|value| value.record_id.clone()),
                    limit: Some(i64::from(limit.clamp(1, 100))),
                    offset: None,
                    order: Some("desc".to_string()),
                },
            )
            .await?;
        let mut items = page.items;
        let mut has_more = page.has_more;
        let boundary_turn_id = items
            .last()
            .and_then(|record| record.metadata.as_ref())
            .and_then(|metadata| metadata.get("conversation_turn_id"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        while has_more {
            let Some(boundary_turn_id) = boundary_turn_id.as_deref() else {
                break;
            };
            let Some(last) = items.last() else { break };
            let continuation = self
                .client
                .list_thread_records_page(
                    thread_id,
                    &SdkListThreadRecordsRequest {
                        tenant_id: tenant_id.to_string(),
                        conversation_id: Some(conversation_id.to_string()),
                        role: None,
                        record_type: Some("message".to_string()),
                        summary_status: None,
                        after_created_at: Some(last.created_at.clone()),
                        after_id: Some(last.id.clone()),
                        limit: Some(100),
                        offset: None,
                        order: Some("desc".to_string()),
                    },
                )
                .await?;
            let same_turn_count = continuation
                .items
                .iter()
                .take_while(|record| {
                    record
                        .metadata
                        .as_ref()
                        .and_then(|metadata| metadata.get("conversation_turn_id"))
                        .and_then(serde_json::Value::as_str)
                        == Some(boundary_turn_id)
                })
                .count();
            if same_turn_count == 0 {
                has_more = !continuation.items.is_empty() || continuation.has_more;
                break;
            }
            let continuation_len = continuation.items.len();
            items.extend(continuation.items.into_iter().take(same_turn_count));
            if same_turn_count < continuation_len {
                break;
            }
            has_more = continuation.has_more;
        }
        let next_before_cursor = if has_more {
            items
                .last()
                .map(|record| TranscriptCursor {
                    created_at: record.created_at.clone(),
                    record_id: record.id.clone(),
                })
                .map(|cursor| serde_json::to_string(&cursor))
                .transpose()
                .map_err(|error| format!("encode Memory transcript cursor failed: {error}"))?
        } else {
            None
        };
        Ok(AuthoritativeConversationPage {
            records: items
                .into_iter()
                .map(|record| AuthoritativeConversationRecord {
                    record_id: record.id,
                    role: record.role,
                    content: record.content,
                    structured_payload: record.structured_payload,
                    metadata: record.metadata,
                    created_at: record.created_at,
                })
                .collect(),
            next_before_cursor,
        })
    }
}

/// Process-local retained Memory adapter. Authentication values are accepted
/// only as transient strings and this type deliberately implements neither
/// serialization nor debug formatting.
pub struct LocalMemoryRuntimeConfig {
    base_url: String,
    source_id: String,
    timeout: Duration,
    access_token: Option<String>,
}

impl LocalMemoryRuntimeConfig {
    pub fn new(
        base_url: impl Into<String>,
        source_id: impl Into<String>,
        timeout: Duration,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            source_id: source_id.into(),
            timeout,
            access_token: None,
        }
    }

    pub fn with_access_token(mut self, access_token: Option<String>) -> Self {
        self.access_token = normalized(access_token);
        self
    }

    pub fn source_id(&self) -> &str {
        self.source_id.trim()
    }

    pub fn build_services<S>(
        &self,
        store: Arc<S>,
        tenant_id: impl Into<String>,
    ) -> Result<LocalMemoryRuntimeServices, String>
    where
        S: LocalMemoryOutboxStore + 'static,
    {
        let source_id = required("Memory source_id", &self.source_id)?;
        let client = self.build_client()?;
        let composer = MemoryContextComposer::from_client(client.clone());
        let remote_writer: Arc<dyn MemoryRecordWriter> =
            Arc::new(MemoryEngineRecordWriter::from_client(
                client.clone(),
                MemoryRecordScope::per_record_tenant("tenant_id"),
            ));
        let outbox_store: Arc<dyn LocalMemoryOutboxStore> = store;
        let outbox_writer =
            LocalMemoryOutboxWriter::new(Arc::clone(&outbox_store), source_id.clone())?;
        Ok(LocalMemoryRuntimeServices {
            runner: AiRuntime::builder()
                .with_memory_composer(composer)
                .with_record_writer(outbox_writer)
                .build_contextual_turn_runner(),
            sync_worker: LocalMemorySyncWorker::new(outbox_store, remote_writer, tenant_id)?
                .with_lease_duration(
                    self.timeout
                        .saturating_add(Duration::from_secs(5))
                        .max(Duration::from_secs(30)),
                )?,
            transcript_source: Arc::new(MemoryConversationTranscriptSource {
                client: client.clone(),
            }),
            source_id,
        })
    }

    fn build_client(&self) -> Result<MemoryEngineClient, String> {
        let base_url = required("Memory base_url", &self.base_url)?;
        let source_id = required("Memory source_id", &self.source_id)?;
        if self.timeout.is_zero() {
            return Err("Memory timeout must be greater than zero".to_string());
        }
        let http = reqwest::Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|error| format!("build Memory HTTP client failed: {error}"))?;
        let mut client = MemoryEngineClient::new_direct_with_http_client(base_url, source_id, http);
        if let Some(access_token) = self.access_token.as_deref() {
            client = client
                .with_bearer_token(access_token)
                .with_bearer_token_refresh_endpoint(user_service_refresh_endpoint(&self.base_url)?);
        }
        Ok(client)
    }
}

fn user_service_refresh_endpoint(memory_base_url: &str) -> Result<String, String> {
    let mut url = reqwest::Url::parse(memory_base_url.trim())
        .map_err(|error| format!("Memory base_url is invalid: {error}"))?;
    let path = url.path().trim_end_matches('/');
    let prefix = path
        .find("/api/memory")
        .map(|index| &path[..index])
        .unwrap_or(path);
    url.set_path(&format!("{prefix}/api/user/auth/refresh"));
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.to_string())
}

fn required(label: &str, value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        Err(format!("{label} must not be empty"))
    } else {
        Ok(value.to_string())
    }
}

fn normalized(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_string();
        (!value.is_empty()).then_some(value)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_non_secret_memory_configuration() {
        let config = LocalMemoryRuntimeConfig::new(
            "http://127.0.0.1:8080",
            "local_agent",
            Duration::from_secs(1),
        );
        assert_eq!(config.source_id(), "local_agent");
        config.build_client().expect("client");

        let invalid = LocalMemoryRuntimeConfig::new(" ", "local_agent", Duration::from_secs(1));
        assert!(invalid.build_client().is_err());
    }

    #[test]
    fn derives_user_service_refresh_endpoint_from_gateway_memory_url() {
        assert_eq!(
            user_service_refresh_endpoint("https://gateway.example/api/memory").expect("url"),
            "https://gateway.example/api/user/auth/refresh"
        );
        assert_eq!(
            user_service_refresh_endpoint("https://gateway.example/prefix/api/memory-engine/v1")
                .expect("url"),
            "https://gateway.example/prefix/api/user/auth/refresh"
        );
    }
}
