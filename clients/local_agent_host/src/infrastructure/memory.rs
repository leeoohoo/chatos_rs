// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_ai_runtime::{
    AiRuntime, ContextualTurnRunner, MemoryContextComposer, MemoryEngineRecordWriter,
    MemoryRecordScope, MemoryRecordWriter, RuntimeContextAssertionProvider,
    RuntimeContextAssertionRequest,
};
use chatos_local_agent_ports::{
    AuthoritativeConversationPage, AuthoritativeConversationRecord,
    LocalConversationTranscriptSource, LocalMemoryOutboxStore,
};
use futures::future::try_join_all;
use memory_engine_sdk::{
    CompactTurnsResponse, MemoryEngineClient, SdkGetTurnProcessRecordsRequest,
    SdkListCompactTurnsRequest, TurnProcessRecordsResponse,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc, time::Duration};

use crate::{LocalMemoryOutboxWriter, LocalMemorySyncWorker};

pub struct LocalMemoryRuntimeServices {
    pub runner: ContextualTurnRunner,
    pub sync_worker: LocalMemorySyncWorker,
    pub transcript_source: Arc<dyn LocalConversationTranscriptSource>,
    pub source_id: String,
}

#[derive(Clone)]
struct LocalConnectorRuntimeContextAssertionProvider {
    client: MemoryEngineClient,
    endpoint: String,
}

#[derive(Serialize)]
struct LocalRuntimeContextAssertionRequest<'a> {
    runtime_kind: &'static str,
    project_id: Option<&'a str>,
    runtime_id: &'a str,
    operation: &'a str,
    source_id: &'a str,
    canonical_agent_subject_id: &'a str,
    thread_id: &'a str,
}

#[derive(Deserialize)]
struct LocalRuntimeContextAssertionResponse {
    runtime_context_assertion: String,
    source_id: String,
    canonical_agent_subject_id: String,
    thread_id: String,
    project_partition_key: String,
    runtime_identity_revision: String,
}

impl LocalConnectorRuntimeContextAssertionProvider {
    fn new(endpoint: String, client: MemoryEngineClient) -> Self {
        Self { client, endpoint }
    }
}

#[async_trait]
impl RuntimeContextAssertionProvider for LocalConnectorRuntimeContextAssertionProvider {
    async fn issue(&self, request: RuntimeContextAssertionRequest<'_>) -> Result<String, String> {
        let project_id = match request.project_partition_key {
            "__none__" => None,
            value => Some(
                value
                    .strip_prefix("project:")
                    .ok_or_else(|| "Memory project partition is not canonical".to_string())?,
            ),
        };
        let response: LocalRuntimeContextAssertionResponse = self
            .client
            .post_with_user_auth(
                self.endpoint.as_str(),
                &LocalRuntimeContextAssertionRequest {
                    runtime_kind: "local_agent",
                    project_id,
                    runtime_id: request.thread_id,
                    operation: request.operation,
                    source_id: request.source_id,
                    canonical_agent_subject_id: request.canonical_agent_subject_id,
                    thread_id: request.thread_id,
                },
            )
            .await
            .map_err(|error| format!("issue Memory runtime context assertion failed: {error}"))?;
        if response.runtime_context_assertion.trim().is_empty()
            || response.source_id != request.source_id
            || response.canonical_agent_subject_id != request.canonical_agent_subject_id
            || response.thread_id != request.thread_id
            || response.project_partition_key != request.project_partition_key
            || response.runtime_identity_revision.trim().is_empty()
        {
            return Err("Memory runtime context assertion scope mismatch".to_string());
        }
        Ok(response.runtime_context_assertion)
    }
}

#[derive(Clone)]
struct MemoryConversationTranscriptSource {
    client: Arc<dyn MemoryTranscriptClient>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TranscriptCursor {
    before_turn_id: String,
}

#[async_trait]
trait MemoryTranscriptClient: Send + Sync {
    async fn list_compact_turns(
        &self,
        thread_id: &str,
        request: SdkListCompactTurnsRequest,
    ) -> Result<CompactTurnsResponse, String>;

    async fn get_turn_process_records(
        &self,
        thread_id: &str,
        turn_id: &str,
        request: SdkGetTurnProcessRecordsRequest,
    ) -> Result<TurnProcessRecordsResponse, String>;
}

#[async_trait]
impl MemoryTranscriptClient for MemoryEngineClient {
    async fn list_compact_turns(
        &self,
        thread_id: &str,
        request: SdkListCompactTurnsRequest,
    ) -> Result<CompactTurnsResponse, String> {
        self.list_compact_turns(thread_id, &request).await
    }

    async fn get_turn_process_records(
        &self,
        thread_id: &str,
        turn_id: &str,
        request: SdkGetTurnProcessRecordsRequest,
    ) -> Result<TurnProcessRecordsResponse, String> {
        self.get_turn_process_records(thread_id, turn_id, &request)
            .await
    }
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
            .list_compact_turns(
                thread_id,
                SdkListCompactTurnsRequest {
                    tenant_id: tenant_id.to_string(),
                    conversation_id: Some(conversation_id.to_string()),
                    record_type: Some("message".to_string()),
                    limit: Some(i64::from(limit.clamp(1, 100))),
                    before_turn_id: cursor.map(|value| value.before_turn_id),
                },
            )
            .await?;
        let process_pages = try_join_all(page.items.iter().filter(|item| item.has_process).map(
            |item| async move {
                self.client
                    .get_turn_process_records(
                        thread_id,
                        item.turn_id.as_str(),
                        SdkGetTurnProcessRecordsRequest {
                            tenant_id: tenant_id.to_string(),
                            record_type: Some("message".to_string()),
                        },
                    )
                    .await
            },
        ))
        .await?;
        let mut process_by_turn = process_pages
            .into_iter()
            .map(|page| (page.turn_id, page.items))
            .collect::<HashMap<_, _>>();
        let mut records = Vec::new();
        for item in page.items {
            records.push(item.user_record);
            records.extend(process_by_turn.remove(&item.turn_id).unwrap_or_default());
            if let Some(final_assistant) = item.final_assistant_record {
                records.push(final_assistant);
            }
        }
        let next_before_cursor = page
            .next_before
            .map(|before_turn_id| serde_json::to_string(&TranscriptCursor { before_turn_id }))
            .transpose()
            .map_err(|error| format!("encode Memory transcript cursor failed: {error}"))?;
        Ok(AuthoritativeConversationPage {
            records: records
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
        self.access_token.as_ref().ok_or_else(|| {
            "Memory access token is required for runtime context assertion issuance".to_string()
        })?;
        let client = self.build_client()?;
        let assertion_provider = LocalConnectorRuntimeContextAssertionProvider::new(
            runtime_context_assertion_endpoint(&self.base_url)?,
            client.clone(),
        );
        let composer = MemoryContextComposer::from_client(client.clone())
            .with_runtime_context_assertion_provider(assertion_provider.clone());
        let remote_writer: Arc<dyn MemoryRecordWriter> = Arc::new(
            MemoryEngineRecordWriter::from_client(
                client.clone(),
                MemoryRecordScope::per_record_tenant("tenant_id"),
            )
            .with_runtime_context_assertion_provider(assertion_provider),
        );
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
                client: Arc::new(client.clone()),
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

fn runtime_context_assertion_endpoint(memory_base_url: &str) -> Result<String, String> {
    let mut url = reqwest::Url::parse(memory_base_url.trim())
        .map_err(|error| format!("Memory base_url is invalid: {error}"))?;
    let path = url.path().trim_end_matches('/');
    let prefix = path
        .find("/api/memory")
        .map(|index| &path[..index])
        .unwrap_or(path);
    url.set_path(&format!("{prefix}/api/local/runtime-context-assertions"));
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
#[path = "memory_auth_tests.rs"]
mod auth_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use memory_engine_sdk::{EngineRecord, TurnRecordSlice};
    use serde_json::json;
    use std::sync::Mutex;

    struct TranscriptClientFixture {
        compact_page: Mutex<Option<CompactTurnsResponse>>,
        process_pages: Mutex<HashMap<String, TurnProcessRecordsResponse>>,
        compact_requests: Mutex<Vec<SdkListCompactTurnsRequest>>,
    }

    #[async_trait]
    impl MemoryTranscriptClient for TranscriptClientFixture {
        async fn list_compact_turns(
            &self,
            _thread_id: &str,
            request: SdkListCompactTurnsRequest,
        ) -> Result<CompactTurnsResponse, String> {
            self.compact_requests.lock().unwrap().push(request);
            self.compact_page
                .lock()
                .unwrap()
                .take()
                .ok_or_else(|| "compact page already consumed".to_string())
        }

        async fn get_turn_process_records(
            &self,
            _thread_id: &str,
            turn_id: &str,
            _request: SdkGetTurnProcessRecordsRequest,
        ) -> Result<TurnProcessRecordsResponse, String> {
            self.process_pages
                .lock()
                .unwrap()
                .remove(turn_id)
                .ok_or_else(|| format!("missing process page for {turn_id}"))
        }
    }

    fn record(
        id: &str,
        turn_id: &str,
        conversation_id: &str,
        role: &str,
        content: &str,
        created_at: &str,
    ) -> EngineRecord {
        EngineRecord {
            id: id.to_string(),
            thread_id: "client-agent:agent:jiguli".to_string(),
            tenant_id: "tenant-1".to_string(),
            source_id: "chatos".to_string(),
            external_record_id: None,
            role: role.to_string(),
            record_type: "message".to_string(),
            content: content.to_string(),
            structured_payload: None,
            metadata: Some(json!({
                "conversation_id": conversation_id,
                "conversation_turn_id": turn_id,
            })),
            summary_status: "pending".to_string(),
            summary_id: None,
            summarized_at: None,
            created_at: created_at.to_string(),
        }
    }

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

    #[test]
    fn derives_public_runtime_context_assertion_endpoint_from_gateway_memory_url() {
        assert_eq!(
            runtime_context_assertion_endpoint("https://gateway.example/api/memory").expect("url"),
            "https://gateway.example/api/local/runtime-context-assertions"
        );
        assert_eq!(
            runtime_context_assertion_endpoint(
                "https://gateway.example/prefix/api/memory-engine/v1?ignored=true"
            )
            .expect("url"),
            "https://gateway.example/prefix/api/local/runtime-context-assertions"
        );
    }

    #[tokio::test]
    async fn transcript_pages_complete_interleaved_turns_with_their_user_messages() {
        let conversation_id = "conversation-1";
        let older_user = record(
            "older-user",
            "older-turn",
            conversation_id,
            "user",
            "older request",
            "2026-11-07T01:00:00Z",
        );
        let older_callback = record(
            "older-task-callback",
            "older-turn",
            conversation_id,
            "assistant",
            "older task cancelled",
            "2026-11-07T09:16:38Z",
        );
        let current_user = record(
            "current-user",
            "current-turn",
            conversation_id,
            "user",
            "current request",
            "2026-11-07T09:14:44Z",
        );
        let current_final = record(
            "current-final",
            "current-turn",
            conversation_id,
            "assistant",
            "current answer",
            "2026-11-07T09:17:16Z",
        );
        let fixture = Arc::new(TranscriptClientFixture {
            compact_page: Mutex::new(Some(CompactTurnsResponse {
                items: vec![
                    TurnRecordSlice {
                        turn_id: "older-turn".to_string(),
                        user_record: older_user,
                        final_assistant_record: None,
                        has_process: true,
                        tool_call_count: 0,
                        thinking_count: 0,
                        process_message_count: 1,
                    },
                    TurnRecordSlice {
                        turn_id: "current-turn".to_string(),
                        user_record: current_user,
                        final_assistant_record: Some(current_final),
                        has_process: false,
                        tool_call_count: 0,
                        thinking_count: 0,
                        process_message_count: 0,
                    },
                ],
                has_more: true,
                next_before: Some("older-turn".to_string()),
            })),
            process_pages: Mutex::new(HashMap::from([(
                "older-turn".to_string(),
                TurnProcessRecordsResponse {
                    turn_id: "older-turn".to_string(),
                    items: vec![older_callback],
                },
            )])),
            compact_requests: Mutex::new(Vec::new()),
        });
        let source = MemoryConversationTranscriptSource {
            client: fixture.clone(),
        };

        let page = source
            .list_conversation_records(
                "tenant-1",
                "client-agent:agent:jiguli",
                conversation_id,
                None,
                10,
            )
            .await
            .expect("complete transcript page");

        assert_eq!(
            page.records
                .iter()
                .filter(|record| record.role == "user")
                .map(|record| record.record_id.as_str())
                .collect::<Vec<_>>(),
            ["older-user", "current-user"]
        );
        assert!(page
            .records
            .iter()
            .any(|record| record.record_id == "older-task-callback"));
        let cursor: TranscriptCursor =
            serde_json::from_str(page.next_before_cursor.as_deref().expect("next cursor"))
                .expect("decode cursor");
        assert_eq!(cursor.before_turn_id, "older-turn");
        let requests = fixture.compact_requests.lock().unwrap();
        assert_eq!(
            requests[0].conversation_id.as_deref(),
            Some(conversation_id)
        );
        assert_eq!(requests[0].limit, Some(10));
    }
}
