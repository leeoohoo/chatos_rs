// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Minimal Memory Engine boundary owned by the native clients.
//!
//! This crate intentionally supports only end-user bearer authentication and
//! the four operations required by the Local Agent Runtime. Server-side
//! administration, internal-service authentication, discovery, and implicit
//! retry behavior do not belong in a native client.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use chatos_client_http::{
    classify_http_request_error, read_response_json_limited,
    read_response_preview_text_limited_or_message, ERROR_BODY_PREVIEW_LIMIT_BYTES,
};
use reqwest::{Method, RequestBuilder};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use zeroize::Zeroizing;

const RESPONSE_BODY_LIMIT_BYTES: usize = 16 * 1024 * 1024;
const MAXIMUM_BEARER_TOKEN_BYTES: usize = 64 * 1024;

/// Process-local bearer credential shared by every client that belongs to one
/// authenticated native account. Each request obtains a short-lived,
/// zeroizing snapshot before its first await. Rotating the source therefore
/// never cancels an in-flight request, while every later request observes the
/// replacement credential.
#[derive(Clone)]
pub struct RotatingBearerToken {
    value: Arc<RwLock<Zeroizing<String>>>,
}

impl RotatingBearerToken {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        Ok(Self {
            value: Arc::new(RwLock::new(validate_bearer_token(value.into())?)),
        })
    }

    pub fn snapshot(&self) -> Result<Zeroizing<String>, String> {
        let value = self
            .value
            .read()
            .map_err(|_| "bearer token lock is unavailable".to_string())?;
        Ok(Zeroizing::new(value.to_string()))
    }

    pub fn rotate(&self, value: impl Into<String>) -> Result<(), String> {
        self.rotate_zeroizing(Zeroizing::new(value.into()))
    }

    pub fn rotate_zeroizing(&self, value: Zeroizing<String>) -> Result<(), String> {
        let mut replacement = validate_bearer_token(value.to_string())?;
        let mut current = self
            .value
            .write()
            .map_err(|_| "bearer token lock is unavailable".to_string())?;
        std::mem::swap(&mut *current, &mut replacement);
        // replacement now owns the superseded token and zeroizes it at the
        // end of this scope. Existing request snapshots remain valid until
        // those requests complete, then zeroize themselves independently.
        Ok(())
    }
}

impl std::fmt::Debug for RotatingBearerToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("RotatingBearerToken([REDACTED])")
    }
}

fn validate_bearer_token(value: String) -> Result<Zeroizing<String>, String> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > MAXIMUM_BEARER_TOKEN_BYTES
        || value.chars().any(char::is_control)
    {
        return Err("bearer_token is invalid".to_string());
    }
    Ok(Zeroizing::new(value))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComposeContextPolicy {
    pub include_recent_records: Option<bool>,
    pub include_thread_summary: Option<bool>,
    pub include_subject_memory: Option<bool>,
    pub recent_record_limit: Option<usize>,
    pub summary_limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComposeContextRequest {
    pub tenant_id: String,
    pub subject_id: Option<String>,
    pub related_subject_ids: Option<Vec<String>>,
    pub thread_id: String,
    pub policy: Option<ComposeContextPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComposeContextBlock {
    pub block_type: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComposeContextMeta {
    pub summary_count: usize,
    pub recent_record_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComposeContextResponse {
    pub thread_id: String,
    pub blocks: Vec<ComposeContextBlock>,
    pub recent_records: Vec<EngineRecord>,
    pub meta: ComposeContextMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EngineRecord {
    pub id: String,
    pub thread_id: String,
    pub tenant_id: String,
    pub source_id: String,
    pub external_record_id: Option<String>,
    pub role: String,
    pub record_type: String,
    pub content: String,
    pub structured_payload: Option<Value>,
    pub metadata: Option<Value>,
    #[serde(default = "default_pending")]
    pub summary_status: String,
    pub summary_id: Option<String>,
    pub summarized_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpsertRecordInput {
    pub id: String,
    pub external_record_id: Option<String>,
    pub role: String,
    pub record_type: String,
    pub content: String,
    pub structured_payload: Option<Value>,
    pub metadata: Option<Value>,
    pub summary_status: Option<String>,
    pub summary_id: Option<String>,
    pub summarized_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BatchSyncRecordsRequest {
    pub tenant_id: String,
    pub records: Vec<UpsertRecordInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BatchSyncRecordsResponse {
    pub thread_id: String,
    pub received_count: usize,
    pub upserted_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunThreadActiveSummaryResponse {
    pub thread_id: String,
    #[serde(default)]
    pub accepted: bool,
    #[serde(default)]
    pub running: bool,
    #[serde(default)]
    pub completed: bool,
    #[serde(default)]
    pub failed: bool,
    pub job_run_id: Option<String>,
    #[serde(default)]
    pub generated: bool,
    pub summary_id: Option<String>,
    pub source_record_count: usize,
    pub pending_before_count: Option<i64>,
    pub pending_after_count: Option<i64>,
    #[serde(default)]
    pub compacted: bool,
    pub error_message: Option<String>,
}

#[derive(Clone)]
pub struct MemoryEngineClient {
    http: reqwest::Client,
    base_url: String,
    source_id: String,
    bearer_token: RotatingBearerToken,
}

impl std::fmt::Debug for MemoryEngineClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MemoryEngineClient")
            .field("base_url", &self.base_url)
            .field("source_id", &self.source_id)
            .field("bearer_token", &"[REDACTED]")
            .finish()
    }
}

impl MemoryEngineClient {
    pub fn new(
        base_url: impl Into<String>,
        timeout: Duration,
        source_id: impl Into<String>,
        bearer_token: impl Into<String>,
    ) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|error| format!("Memory Engine HTTP client could not be created: {error}"))?;
        let bearer_token = RotatingBearerToken::new(bearer_token)?;
        Self::new_with_http_client_and_token(base_url, source_id, bearer_token, http)
    }

    pub fn new_with_http_client(
        base_url: impl Into<String>,
        source_id: impl Into<String>,
        bearer_token: impl Into<String>,
        http: reqwest::Client,
    ) -> Result<Self, String> {
        let bearer_token = RotatingBearerToken::new(bearer_token)?;
        Self::new_with_http_client_and_token(base_url, source_id, bearer_token, http)
    }

    pub fn new_with_token(
        base_url: impl Into<String>,
        timeout: Duration,
        source_id: impl Into<String>,
        bearer_token: RotatingBearerToken,
    ) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|error| format!("Memory Engine HTTP client could not be created: {error}"))?;
        Self::new_with_http_client_and_token(base_url, source_id, bearer_token, http)
    }

    pub fn new_with_http_client_and_token(
        base_url: impl Into<String>,
        source_id: impl Into<String>,
        bearer_token: RotatingBearerToken,
        http: reqwest::Client,
    ) -> Result<Self, String> {
        let base_url = normalize_base_url(base_url.into())?;
        let source_id = require_credential("source_id", source_id.into())?;
        // Fail construction if the shared source was poisoned or invalidated.
        drop(bearer_token.snapshot()?);
        Ok(Self {
            http,
            base_url,
            source_id,
            bearer_token,
        })
    }

    pub async fn compose_context(
        &self,
        request: &ComposeContextRequest,
    ) -> Result<ComposeContextResponse, String> {
        #[derive(Serialize)]
        struct DirectRequest<'a> {
            tenant_id: &'a str,
            source_id: &'a str,
            subject_id: Option<&'a str>,
            related_subject_ids: Option<&'a [String]>,
            thread_id: &'a str,
            policy: Option<&'a ComposeContextPolicy>,
        }
        let body = DirectRequest {
            tenant_id: request.tenant_id.as_str(),
            source_id: self.source_id.as_str(),
            subject_id: request.subject_id.as_deref(),
            related_subject_ids: request.related_subject_ids.as_deref(),
            thread_id: request.thread_id.as_str(),
            policy: request.policy.as_ref(),
        };
        self.send_json(Method::POST, "/context/compose", Some(&body))
            .await
    }

    pub async fn run_thread_active_summary(
        &self,
        thread_id: &str,
        tenant_id: &str,
        trigger_reason: Option<&str>,
    ) -> Result<RunThreadActiveSummaryResponse, String> {
        #[derive(Serialize)]
        struct DirectRequest<'a> {
            tenant_id: &'a str,
            source_id: &'a str,
            trigger_reason: Option<&'a str>,
        }
        let body = DirectRequest {
            tenant_id,
            source_id: self.source_id.as_str(),
            trigger_reason,
        };
        self.send_json(
            Method::POST,
            &format!(
                "/threads/{}/active-summary/run",
                urlencoding::encode(thread_id)
            ),
            Some(&body),
        )
        .await
    }

    pub async fn get_thread_active_summary_status(
        &self,
        thread_id: &str,
        tenant_id: &str,
        job_run_id: Option<&str>,
    ) -> Result<RunThreadActiveSummaryResponse, String> {
        let mut query = vec![
            ("tenant_id", tenant_id),
            ("source_id", self.source_id.as_str()),
        ];
        if let Some(job_run_id) = job_run_id.filter(|value| !value.trim().is_empty()) {
            query.push(("job_run_id", job_run_id));
        }
        let query = query
            .into_iter()
            .map(|(key, value)| {
                format!(
                    "{}={}",
                    urlencoding::encode(key),
                    urlencoding::encode(value)
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        self.send_json::<RunThreadActiveSummaryResponse, ()>(
            Method::GET,
            &format!(
                "/threads/{}/active-summary/status?{query}",
                urlencoding::encode(thread_id)
            ),
            None,
        )
        .await
    }

    pub async fn batch_sync_records(
        &self,
        thread_id: &str,
        request: &BatchSyncRecordsRequest,
    ) -> Result<BatchSyncRecordsResponse, String> {
        #[derive(Serialize)]
        struct DirectRequest<'a> {
            tenant_id: &'a str,
            source_id: &'a str,
            records: &'a [UpsertRecordInput],
        }
        let body = DirectRequest {
            tenant_id: request.tenant_id.as_str(),
            source_id: self.source_id.as_str(),
            records: request.records.as_slice(),
        };
        self.send_json(
            Method::PUT,
            &format!(
                "/threads/{}/records/batch-sync",
                urlencoding::encode(thread_id)
            ),
            Some(&body),
        )
        .await
    }

    async fn send_json<T, B>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<T, String>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let url = format!("{}{}", self.base_url, path);
        let bearer_token = self.bearer_token.snapshot()?;
        let request = self
            .http
            .request(method.clone(), url.as_str())
            .bearer_auth(bearer_token.as_str());
        let request = apply_json_body(request, body);
        let response = request.send().await.map_err(|error| {
            format!(
                "Memory Engine request failed: kind={} method={} url={} detail={error}",
                classify_http_request_error(&error).as_str(),
                method.as_str(),
                url
            )
        })?;
        let status = response.status();
        if !status.is_success() {
            let detail = read_response_preview_text_limited_or_message(
                response,
                ERROR_BODY_PREVIEW_LIMIT_BYTES,
            )
            .await;
            return Err(format!(
                "Memory Engine request failed: kind=status status={} method={} url={} detail={detail}",
                status,
                method.as_str(),
                url
            ));
        }
        read_response_json_limited(response, RESPONSE_BODY_LIMIT_BYTES)
            .await
            .map_err(|error| {
                format!(
                    "Memory Engine response is invalid: method={} url={} detail={error}",
                    method.as_str(),
                    url
                )
            })
    }
}

fn apply_json_body<B>(request: RequestBuilder, body: Option<&B>) -> RequestBuilder
where
    B: Serialize + ?Sized,
{
    match body {
        Some(body) => request.json(body),
        None => request,
    }
}

fn normalize_base_url(mut base_url: String) -> Result<String, String> {
    if base_url.trim() != base_url || base_url.is_empty() {
        return Err("Memory Engine base URL is invalid".to_string());
    }
    while base_url.ends_with('/') {
        base_url.pop();
    }
    let parsed = reqwest::Url::parse(base_url.as_str())
        .map_err(|error| format!("Memory Engine base URL is invalid: {error}"))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err("Memory Engine base URL must be an absolute HTTP(S) URL".to_string());
    }
    if base_url.ends_with("/api/memory-engine/v1") || base_url.contains("/api/memory-engine/") {
        Ok(base_url)
    } else {
        Ok(format!("{base_url}/api/memory-engine/v1"))
    }
}

fn require_credential(name: &str, value: String) -> Result<String, String> {
    if value.is_empty() || value.trim() != value {
        Err(format!("Memory Engine {name} is invalid"))
    } else {
        Ok(value)
    }
}

fn default_pending() -> String {
    "pending".to_string()
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use axum::{extract::State, http::HeaderMap, routing::post, Json, Router};
    use serde_json::json;
    use tokio::sync::Semaphore;

    use super::{
        normalize_base_url, ComposeContextRequest, MemoryEngineClient, RotatingBearerToken,
    };

    #[test]
    fn base_url_has_one_memory_api_prefix() {
        assert_eq!(
            normalize_base_url("http://localhost:3000/".to_string()).unwrap(),
            "http://localhost:3000/api/memory-engine/v1"
        );
        assert_eq!(
            normalize_base_url("https://memory.example/api/memory-engine/v1/".to_string()).unwrap(),
            "https://memory.example/api/memory-engine/v1"
        );
    }

    #[test]
    fn rotating_token_preserves_in_flight_snapshots_and_updates_every_clone() {
        let source = RotatingBearerToken::new("old-token").unwrap();
        let shared_clone = source.clone();
        let in_flight = source.snapshot().unwrap();

        source.rotate("new-token").unwrap();

        assert_eq!(in_flight.as_str(), "old-token");
        assert_eq!(shared_clone.snapshot().unwrap().as_str(), "new-token");
        assert!(!format!("{source:?}").contains("new-token"));
        assert!(format!("{source:?}").contains("[REDACTED]"));
    }

    #[test]
    fn rotating_token_rejects_invalid_replacements_without_changing_the_source() {
        let source = RotatingBearerToken::new("valid-token").unwrap();

        assert!(source.rotate(" invalid").is_err());
        assert_eq!(source.snapshot().unwrap().as_str(), "valid-token");
        assert!(RotatingBearerToken::new("").is_err());
        assert!(RotatingBearerToken::new("x".repeat(64 * 1024 + 1)).is_err());
    }

    #[derive(Clone)]
    struct AuthorizationProbe {
        observed: Arc<Mutex<Vec<String>>>,
        first_seen: Arc<Semaphore>,
        release_first: Arc<Semaphore>,
    }

    async fn compose_probe(
        State(probe): State<AuthorizationProbe>,
        headers: HeaderMap,
    ) -> Json<serde_json::Value> {
        let authorization = headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let request_index = {
            let mut observed = probe.observed.lock().unwrap();
            let index = observed.len();
            observed.push(authorization);
            index
        };
        if request_index == 0 {
            probe.first_seen.add_permits(1);
            probe.release_first.acquire().await.unwrap().forget();
        }
        Json(json!({
            "thread_id": "thread-1",
            "blocks": [],
            "recent_records": [],
            "meta": {"summary_count": 0, "recent_record_count": 0}
        }))
    }

    #[tokio::test]
    async fn in_flight_memory_request_finishes_with_old_token_and_next_request_uses_new_token() {
        let probe = AuthorizationProbe {
            observed: Arc::new(Mutex::new(Vec::new())),
            first_seen: Arc::new(Semaphore::new(0)),
            release_first: Arc::new(Semaphore::new(0)),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router = Router::new()
            .route("/api/memory-engine/v1/context/compose", post(compose_probe))
            .with_state(probe.clone());
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let token = RotatingBearerToken::new("old-token").unwrap();
        let client = MemoryEngineClient::new_with_token(
            format!("http://{address}"),
            Duration::from_secs(5),
            "native-client",
            token.clone(),
        )
        .unwrap();
        let request = ComposeContextRequest {
            tenant_id: "user-1".to_string(),
            subject_id: None,
            related_subject_ids: None,
            thread_id: "thread-1".to_string(),
            policy: None,
        };

        let first_client = client.clone();
        let first_request = request.clone();
        let first = tokio::spawn(async move { first_client.compose_context(&first_request).await });
        probe.first_seen.acquire().await.unwrap().forget();
        token.rotate("new-token").unwrap();

        client.compose_context(&request).await.unwrap();
        probe.release_first.add_permits(1);
        first.await.unwrap().unwrap();

        assert_eq!(
            *probe.observed.lock().unwrap(),
            [
                "Bearer old-token".to_string(),
                "Bearer new-token".to_string()
            ]
        );
        server.abort();
    }
}
