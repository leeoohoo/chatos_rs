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

fn should_retry_send_error(
    error: &reqwest::Error,
    completed_retries: usize,
    retry_request_errors: bool,
) -> bool {
    completed_retries < CONNECT_RETRY_DELAYS.len()
        && (error.is_connect() || (retry_request_errors && error.is_request()))
}

impl MemoryEngineClient {
    /// Calls a sibling service on the same origin using this client's user
    /// session, including its shared token rotation and single-flight refresh.
    /// System keys and internal Memory credentials are never sent to it.
    pub async fn post_with_user_auth<T, B>(&self, endpoint: &str, body: &B) -> Result<T, String>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.post_with_user_auth_retry_policy(endpoint, body, false)
            .await
    }

    /// Replays transient failures only when the caller guarantees that repeating the POST is safe.
    pub async fn post_retryable_with_user_auth<T, B>(
        &self,
        endpoint: &str,
        body: &B,
    ) -> Result<T, String>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.post_with_user_auth_retry_policy(endpoint, body, true)
            .await
    }

    async fn post_with_user_auth_retry_policy<T, B>(
        &self,
        endpoint: &str,
        body: &B,
        retry_request_errors: bool,
    ) -> Result<T, String>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        if !matches!(self.auth, AuthMode::Direct { .. })
            || self.internal_service_auth.is_some()
            || self.current_access_token().is_none()
        {
            return Err("Sibling service requests require a user bearer session".to_string());
        }
        let base = reqwest::Url::parse(&self.base_url)
            .map_err(|_| "Memory base URL is invalid".to_string())?;
        let url = reqwest::Url::parse(endpoint)
            .map_err(|_| "Sibling service URL is invalid".to_string())?;
        if !matches!(url.scheme(), "http" | "https")
            || url.origin() != base.origin()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err(
                "Sibling service URL must use the Memory origin without credentials or fragment"
                    .to_string(),
            );
        }
        self.send_json_to_url(
            Method::POST,
            url.as_str(),
            url.path(),
            Some(body),
            retry_request_errors,
        )
        .await
    }

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
        self.send_json_to_url(method, &url, path, body, false).await
    }

    async fn send_json_to_url<T, B>(
        &self,
        method: Method,
        url: &str,
        path: &str,
        body: Option<&B>,
        retry_request_errors: bool,
    ) -> Result<T, String>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let method_label = method.as_str().to_string();
        let mut completed_auth_retries = 0usize;
        let resp = loop {
            let request_access_token = self.current_access_token();
            let mut connect_retry_index = 0usize;
            let resp = loop {
                let req = self.http.request(method.clone(), url);
                let req = self.apply_auth(req, path, request_access_token.as_deref())?;
                let req = if let Some(body) = body {
                    req.json(body)
                } else {
                    req
                };
                match req.send().await {
                    Ok(resp) => break resp,
                    Err(err)
                        if should_retry_send_error(
                            &err,
                            connect_retry_index,
                            retry_request_errors,
                        ) =>
                    {
                        let delay = CONNECT_RETRY_DELAYS[connect_retry_index];
                        connect_retry_index += 1;
                        tokio::time::sleep(delay).await;
                    }
                    Err(err) => {
                        return Err(format_reqwest_error(
                            "send",
                            method_label.as_str(),
                            url,
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
        resp.json::<T>()
            .await
            .map_err(|err| format_reqwest_error("decode_json", method_label.as_str(), url, err))
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
#[path = "transport_user_auth_tests.rs"]
mod user_auth_tests;

#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;
