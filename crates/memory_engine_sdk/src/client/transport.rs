// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::error::Error;
use std::time::Duration;

use reqwest::{header::HeaderMap, Method, RequestBuilder, StatusCode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::{AuthMode, MemoryEngineClient};

const TOKEN_AUDIENCE: &str = "memory-engine";
const DATA_SCOPE: &str = "memory.data";
const SOURCE_SCOPE: &str = "memory.source";
const ADMIN_SCOPE: &str = "memory.admin";
const MODEL_PROFILE_SYNC_SCOPE: &str = "model-profile.sync";
const CONNECT_RETRY_DELAYS: [Duration; 3] = [
    Duration::from_millis(200),
    Duration::from_millis(500),
    Duration::from_secs(1),
];
const REFRESH_RESPONSE_LIMIT_BYTES: usize = 64 * 1024;

#[derive(Deserialize)]
struct RefreshAccessTokenResponse {
    #[serde(alias = "access_token")]
    token: String,
}

fn should_retry_connect_error(error: &reqwest::Error, completed_retries: usize) -> bool {
    error.is_connect() && completed_retries < CONNECT_RETRY_DELAYS.len()
}

impl MemoryEngineClient {
    pub(super) async fn send_json<T, B>(
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
        let method_label = method.as_str().to_string();
        let mut completed_auth_retries = 0usize;
        let resp = loop {
            let request_access_token = self.current_access_token();
            let mut connect_retry_index = 0usize;
            let resp = loop {
                let req = self.http.request(method.clone(), url.clone());
                let req = self.apply_auth(req, path, request_access_token.as_deref())?;
                let req = if let Some(body) = body {
                    req.json(body)
                } else {
                    req
                };
                match req.send().await {
                    Ok(resp) => break resp,
                    Err(err) if should_retry_connect_error(&err, connect_retry_index) => {
                        let delay = CONNECT_RETRY_DELAYS[connect_retry_index];
                        connect_retry_index += 1;
                        tokio::time::sleep(delay).await;
                    }
                    Err(err) => {
                        return Err(format_reqwest_error(
                            "send",
                            method_label.as_str(),
                            url.as_str(),
                            err,
                        ));
                    }
                }
            };
            if resp.status() == StatusCode::UNAUTHORIZED
                && completed_auth_retries == 0
                && self.access_token_refresh_endpoint.is_some()
                && request_access_token.is_some()
            {
                self.refresh_access_token(request_access_token.as_deref().unwrap_or_default())
                    .await?;
                completed_auth_retries += 1;
                continue;
            }
            self.adopt_response_access_token(request_access_token.as_deref(), resp.headers());
            break resp;
        };
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!(
                "status={} method={} url={} detail={}",
                status,
                method_label,
                url,
                truncate_detail(body.as_str(), 4096)
            ));
        }
        resp.json::<T>().await.map_err(|err| {
            format_reqwest_error("decode_json", method_label.as_str(), url.as_str(), err)
        })
    }

    pub(super) async fn delete_with_query(
        &self,
        path: &str,
        query_pairs: &[(&str, &str)],
    ) -> Result<bool, String> {
        #[derive(serde::Deserialize)]
        struct DeleteResponse {
            deleted: bool,
        }

        let query = query_pairs
            .iter()
            .map(|(key, value)| {
                format!(
                    "{}={}",
                    urlencoding::encode(key),
                    urlencoding::encode(value)
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        let full_path = if query.is_empty() {
            path.to_string()
        } else {
            format!("{path}?{query}")
        };
        let resp: DeleteResponse = self
            .send_json(Method::DELETE, full_path.as_str(), Option::<&()>::None)
            .await?;
        Ok(resp.deleted)
    }

    fn apply_auth(
        &self,
        req: RequestBuilder,
        path: &str,
        access_token: Option<&str>,
    ) -> Result<RequestBuilder, String> {
        if let Some(auth) = self.internal_service_auth.as_ref() {
            let token = chatos_service_runtime::issue_internal_service_token(
                auth.secret.as_str(),
                auth.caller.as_str(),
                TOKEN_AUDIENCE,
                internal_scope_for_path(path),
                60,
            )?;
            return Ok(req
                .header("x-memory-caller", auth.caller.as_str())
                .header("x-memory-internal-token", token));
        }
        let req = if let Some(access_token) = access_token {
            req.bearer_auth(access_token)
        } else {
            req
        };

        Ok(match &self.auth {
            AuthMode::Direct { .. } => req,
            AuthMode::SystemKey {
                system_id,
                secret_key,
            } => req
                .header("x-memory-system-id", system_id)
                .header("x-memory-system-key", secret_key),
        })
    }

    fn current_access_token(&self) -> Option<String> {
        self.access_token
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn adopt_response_access_token(&self, request_access_token: Option<&str>, headers: &HeaderMap) {
        let Some(refreshed) = headers
            .get("x-access-token")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| super::normalize_token(value.to_string()))
        else {
            return;
        };
        let mut current = self
            .access_token
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if current.as_deref() == request_access_token {
            *current = Some(refreshed);
        }
    }

    async fn refresh_access_token(&self, rejected_access_token: &str) -> Result<(), String> {
        let _guard = self.access_token_refresh_gate.lock().await;
        if self.current_access_token().as_deref() != Some(rejected_access_token) {
            return Ok(());
        }
        let endpoint = self
            .access_token_refresh_endpoint
            .as_deref()
            .ok_or_else(|| "Memory access-token refresh endpoint is unavailable".to_string())?;
        let response = self
            .http
            .post(endpoint)
            .bearer_auth(rejected_access_token)
            .header("accept", "application/json")
            .send()
            .await
            .map_err(|error| {
                format_reqwest_error("refresh_access_token", "POST", endpoint, error)
            })?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(format!(
                "Memory access-token refresh failed: status={} detail={}",
                status,
                truncate_detail(body.as_str(), 1024)
            ));
        }
        if response.content_length().is_some_and(|length| {
            length > u64::try_from(REFRESH_RESPONSE_LIMIT_BYTES).unwrap_or(u64::MAX)
        }) {
            return Err("Memory access-token refresh response is too large".to_string());
        }
        let bytes = response.bytes().await.map_err(|error| {
            format!("read Memory access-token refresh response failed: {error}")
        })?;
        if bytes.len() > REFRESH_RESPONSE_LIMIT_BYTES {
            return Err("Memory access-token refresh response is too large".to_string());
        }
        let payload: RefreshAccessTokenResponse =
            serde_json::from_slice(bytes.as_ref()).map_err(|error| {
                format!("decode Memory access-token refresh response failed: {error}")
            })?;
        let token = super::normalize_token(payload.token)
            .ok_or_else(|| "Memory access-token refresh returned an empty token".to_string())?;
        let mut current = self
            .access_token
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if current.as_deref() == Some(rejected_access_token) {
            *current = Some(token);
        }
        Ok(())
    }
}

fn internal_scope_for_path(path: &str) -> &'static str {
    let path = path.split('?').next().unwrap_or(path);
    if path.starts_with("/admin/model-profiles") {
        MODEL_PROFILE_SYNC_SCOPE
    } else if path.starts_with("/admin/sources") {
        SOURCE_SCOPE
    } else if path.starts_with("/admin/") {
        ADMIN_SCOPE
    } else {
        DATA_SCOPE
    }
}

fn format_reqwest_error(stage: &str, method: &str, url: &str, err: reqwest::Error) -> String {
    let mut details = vec![
        format!("stage={stage}"),
        format!("method={method}"),
        format!("url={url}"),
        format!("error={err}"),
        format!("is_timeout={}", err.is_timeout()),
        format!("is_connect={}", err.is_connect()),
        format!("is_request={}", err.is_request()),
        format!("is_body={}", err.is_body()),
        format!("is_decode={}", err.is_decode()),
        format!("is_status={}", err.is_status()),
    ];

    let mut source = err.source();
    let mut index = 0usize;
    while let Some(cause) = source {
        details.push(format!("source[{index}]={cause}"));
        source = cause.source();
        index += 1;
        if index >= 8 {
            details.push("source_chain_truncated=true".to_string());
            break;
        }
    }

    details.join(" ")
}

fn truncate_detail(value: &str, max_chars: usize) -> String {
    let mut output = String::new();
    for (index, ch) in value.chars().enumerate() {
        if index >= max_chars {
            output.push_str("...<truncated>");
            break;
        }
        output.push(ch);
    }
    output
}

pub(super) fn normalize_base_url(mut base_url: String) -> String {
    while base_url.ends_with('/') {
        base_url.pop();
    }
    if base_url.ends_with("/api/memory-engine/v1") {
        return base_url;
    }
    if base_url.ends_with("/api/memory") {
        return base_url;
    }
    if base_url.contains("/api/memory-engine/") {
        return base_url;
    }
    format!("{base_url}/api/memory-engine/v1")
}

pub(super) fn append_optional_query(query: &mut String, key: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        if !query.is_empty() {
            query.push('&');
        }
        query.push_str(urlencoding::encode(key).as_ref());
        query.push('=');
        query.push_str(urlencoding::encode(value).as_ref());
    }
}

pub(super) fn append_optional_i64_query(query: &mut String, key: &str, value: Option<i64>) {
    if let Some(value) = value {
        if !query.is_empty() {
            query.push('&');
        }
        query.push_str(urlencoding::encode(key).as_ref());
        query.push('=');
        query.push_str(value.to_string().as_str());
    }
}

pub(super) fn append_optional_bool_query(query: &mut String, key: &str, value: Option<bool>) {
    if let Some(value) = value {
        append_optional_query(query, key, Some(if value { "true" } else { "false" }));
    }
}

#[cfg(test)]
mod tests {
    use super::{
        append_optional_bool_query, append_optional_i64_query, append_optional_query,
        normalize_base_url, should_retry_connect_error, CONNECT_RETRY_DELAYS, SOURCE_SCOPE,
        TOKEN_AUDIENCE,
    };
    use crate::MemoryEngineClient;
    use std::time::Duration;

    #[test]
    fn normalize_base_url_appends_default_api_prefix() {
        assert_eq!(
            normalize_base_url("http://localhost:3000/".to_string()),
            "http://localhost:3000/api/memory-engine/v1"
        );
    }

    #[test]
    fn normalize_base_url_preserves_existing_memory_engine_path() {
        assert_eq!(
            normalize_base_url("http://localhost:3000/api/memory-engine/v1///".to_string()),
            "http://localhost:3000/api/memory-engine/v1"
        );
        assert_eq!(
            normalize_base_url("http://localhost:3000/custom/api/memory-engine/v2".to_string()),
            "http://localhost:3000/custom/api/memory-engine/v2"
        );
        assert_eq!(
            normalize_base_url("https://gateway.example/api/memory/".to_string()),
            "https://gateway.example/api/memory"
        );
    }

    #[test]
    fn append_optional_query_helpers_build_expected_query_string() {
        let mut query = "tenant_id=tenant-1".to_string();
        append_optional_query(&mut query, "record_type", Some("summary rollup"));
        append_optional_i64_query(&mut query, "limit", Some(20));
        append_optional_bool_query(&mut query, "active", Some(true));

        assert_eq!(
            query,
            "tenant_id=tenant-1&record_type=summary%20rollup&limit=20&active=true"
        );
    }

    #[test]
    fn append_optional_query_helpers_ignore_empty_inputs() {
        let mut query = String::new();
        append_optional_query(&mut query, "record_type", Some("   "));
        append_optional_query(&mut query, "role", None);
        append_optional_i64_query(&mut query, "limit", None);
        append_optional_bool_query(&mut query, "active", None);

        assert!(query.is_empty());
    }

    #[tokio::test]
    async fn connect_errors_are_retried_only_within_the_bounded_budget() {
        let error = reqwest::Client::new()
            .get("http://127.0.0.1:0/connect-retry-test")
            .send()
            .await
            .expect_err("unused local port should refuse the connection");

        assert!(should_retry_connect_error(&error, 0));
        assert!(should_retry_connect_error(
            &error,
            CONNECT_RETRY_DELAYS.len() - 1
        ));
        assert!(!should_retry_connect_error(
            &error,
            CONNECT_RETRY_DELAYS.len()
        ));
    }

    #[tokio::test]
    async fn send_json_recovers_when_the_service_starts_during_connect_backoff() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let reservation = TcpListener::bind("127.0.0.1:0").expect("reserve port");
        let address = reservation.local_addr().expect("reserved address");
        drop(reservation);
        let server = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            let listener = TcpListener::bind(address).expect("bind delayed server");
            let (mut stream, _) = listener.accept().expect("accept retried request");
            let mut request = [0_u8; 2048];
            let _ = stream.read(&mut request).expect("read request");
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
                )
                .expect("write response");
        });
        let client = MemoryEngineClient::new_platform(
            format!("http://{address}/api/memory-engine/v1"),
            Duration::from_secs(2),
        )
        .expect("client");

        let response: serde_json::Value = client
            .send_json(reqwest::Method::GET, "/retry-probe", Option::<&()>::None)
            .await
            .expect("connect retry should recover");

        assert_eq!(response, serde_json::json!({}));
        server.join().expect("delayed server");
    }

    #[test]
    fn with_bearer_token_stores_authorization_header_value() {
        let client =
            MemoryEngineClient::new_platform("http://localhost:3000", Duration::from_secs(5))
                .expect("client")
                .with_bearer_token(" user-token-1 ");

        let request = client
            .apply_auth(
                client.http.get("http://localhost:3000/test"),
                "/test",
                client.current_access_token().as_deref(),
            )
            .expect("apply auth")
            .build()
            .expect("request");

        assert_eq!(
            request
                .headers()
                .get(reqwest::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer user-token-1")
        );
    }

    #[test]
    fn internal_service_auth_issues_scoped_token_without_static_secret() {
        let client =
            MemoryEngineClient::new_platform("http://localhost:3000", Duration::from_secs(5))
                .expect("client")
                .with_internal_service_auth("task-runner", "a-long-task-runner-memory-secret");
        let request = client
            .apply_auth(
                client.http.get("http://localhost:3000/admin/sources"),
                "/admin/sources",
                None,
            )
            .expect("apply auth")
            .build()
            .expect("request");
        assert!(!request.headers().contains_key("x-memory-operator-token"));
        assert_eq!(
            request
                .headers()
                .get("x-memory-caller")
                .and_then(|value| value.to_str().ok()),
            Some("task-runner")
        );
        let token = request
            .headers()
            .get("x-memory-internal-token")
            .and_then(|value| value.to_str().ok())
            .expect("internal token");
        chatos_service_runtime::verify_internal_service_token(
            token,
            "a-long-task-runner-memory-secret",
            "task-runner",
            TOKEN_AUDIENCE,
            SOURCE_SCOPE,
        )
        .expect("valid token");
    }

    #[tokio::test]
    async fn unauthorized_memory_request_refreshes_once_and_reuses_the_rotated_token() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::{Arc, Mutex};

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind server");
        let address = listener.local_addr().expect("server address");
        let captured = Arc::new(Mutex::new(Vec::<String>::new()));
        let server_captured = Arc::clone(&captured);
        let server = std::thread::spawn(move || {
            for _ in 0..4 {
                let (mut stream, _) = listener.accept().expect("accept request");
                let mut buffer = [0_u8; 4096];
                let count = stream.read(&mut buffer).expect("read request");
                let request = String::from_utf8_lossy(&buffer[..count]).to_string();
                server_captured
                    .lock()
                    .expect("captured")
                    .push(request.clone());
                let first_line = request.lines().next().unwrap_or_default();
                let authorization = request
                    .lines()
                    .find(|line| line.to_ascii_lowercase().starts_with("authorization:"))
                    .unwrap_or_default();
                let (status, body) = if first_line.contains("/api/user/auth/refresh") {
                    assert!(authorization.contains("Bearer expired-token"));
                    ("200 OK", r#"{"token":"renewed-token"}"#)
                } else if authorization.contains("Bearer renewed-token") {
                    ("200 OK", r#"{"ok":true}"#)
                } else {
                    ("401 Unauthorized", r#"{"error":"expired"}"#)
                };
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .expect("write response");
            }
        });
        let client = MemoryEngineClient::new_direct(
            format!("http://{address}/api/memory"),
            Duration::from_secs(2),
            "chatos",
        )
        .expect("client")
        .with_bearer_token("expired-token")
        .with_bearer_token_refresh_endpoint(format!("http://{address}/api/user/auth/refresh"));

        for _ in 0..2 {
            let response: serde_json::Value = client
                .send_json(reqwest::Method::GET, "/probe", Option::<&()>::None)
                .await
                .expect("request should recover");
            assert_eq!(response, serde_json::json!({"ok": true}));
        }

        server.join().expect("server");
        let requests = captured.lock().expect("captured");
        assert_eq!(requests.len(), 4);
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.starts_with("POST /api/user/auth/refresh "))
                .count(),
            1
        );
        assert!(requests[3].contains("Bearer renewed-token"));
    }

    #[tokio::test]
    async fn forbidden_memory_request_does_not_refresh() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind server");
        let address = listener.local_addr().expect("server address");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut request = [0_u8; 2048];
            let _ = stream.read(&mut request).expect("read request");
            stream
                .write_all(
                    b"HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nContent-Length: 21\r\nConnection: close\r\n\r\n{\"error\":\"forbidden\"}",
                )
                .expect("write response");
        });
        let client = MemoryEngineClient::new_direct(
            format!("http://{address}/api/memory"),
            Duration::from_secs(2),
            "chatos",
        )
        .expect("client")
        .with_bearer_token("valid-token")
        .with_bearer_token_refresh_endpoint(format!("http://{address}/api/user/auth/refresh"));

        let error = client
            .send_json::<serde_json::Value, ()>(reqwest::Method::GET, "/probe", None)
            .await
            .expect_err("403 must remain a permission error");

        assert!(error.contains("status=403 Forbidden"));
        server.join().expect("server");
    }

    #[tokio::test]
    async fn response_header_rotates_the_shared_token_for_all_client_clones() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind server");
        let address = listener.local_addr().expect("server address");
        let server = std::thread::spawn(move || {
            for index in 0..2 {
                let (mut stream, _) = listener.accept().expect("accept request");
                let mut buffer = [0_u8; 2048];
                let count = stream.read(&mut buffer).expect("read request");
                let request = String::from_utf8_lossy(&buffer[..count]);
                if index == 0 {
                    assert!(request.contains("Bearer current-token"));
                    stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nX-Access-Token: proactive-token\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}",
                        )
                        .expect("write first response");
                } else {
                    assert!(request.contains("Bearer proactive-token"));
                    stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}",
                        )
                        .expect("write second response");
                }
            }
        });
        let client = MemoryEngineClient::new_direct(
            format!("http://{address}/api/memory"),
            Duration::from_secs(2),
            "chatos",
        )
        .expect("client")
        .with_bearer_token("current-token");
        let clone = client.clone();

        let _: serde_json::Value = client
            .send_json(reqwest::Method::GET, "/probe", Option::<&()>::None)
            .await
            .expect("first request");
        let _: serde_json::Value = clone
            .send_json(reqwest::Method::GET, "/probe", Option::<&()>::None)
            .await
            .expect("clone request");

        server.join().expect("server");
    }
}
