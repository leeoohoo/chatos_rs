// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::StatusCode;
use base64::Engine as _;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::http_body::{read_response_text_limited_or_message, ERROR_BODY_PREVIEW_LIMIT_BYTES};

use super::HarnessMcpContext;

const DEFAULT_MAX_FILE_BYTES: i64 = 256 * 1024;
const MAX_COMMIT_ACTIONS: usize = 500;

#[derive(Debug, Clone, Deserialize)]
pub(super) struct HarnessApiAccessResponse {
    pub(super) base_url: String,
    pub(super) access_token: String,
    #[serde(default)]
    pub(super) space_identifier: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct HarnessContentResponse {
    #[serde(rename = "type")]
    pub(super) kind: String,
    #[serde(default)]
    pub(super) sha: String,
    #[serde(default)]
    pub(super) path: String,
    #[serde(default)]
    pub(super) content: Value,
}

#[derive(Debug, Deserialize)]
struct HarnessFileContent {
    #[serde(default)]
    encoding: String,
    #[serde(default)]
    data: String,
    #[serde(default)]
    size: i64,
    #[serde(default)]
    data_size: i64,
}

#[derive(Debug, Deserialize)]
pub(super) struct HarnessDirContent {
    #[serde(default)]
    pub(super) entries: Vec<HarnessContentInfo>,
}

#[derive(Debug, Deserialize)]
pub(super) struct HarnessContentInfo {
    #[serde(rename = "type")]
    pub(super) kind: String,
    #[serde(default)]
    pub(super) name: String,
    #[serde(default)]
    pub(super) path: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct HarnessListPathsResponse {
    #[serde(default)]
    pub(super) files: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct HarnessBranch {
    pub(super) name: String,
    #[serde(default)]
    pub(super) sha: String,
    #[serde(default)]
    pub(super) is_default: bool,
}

#[derive(Debug, Serialize)]
struct HarnessCommitRequest {
    title: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    branch: Option<String>,
    actions: Vec<HarnessCommitAction>,
}

#[derive(Debug, Serialize)]
pub(super) struct HarnessCommitAction {
    pub(super) action: String,
    pub(super) path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) payload: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) encoding: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) sha: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct HarnessFile {
    pub(super) path: String,
    pub(super) size: i64,
    pub(super) sha256: String,
    pub(super) harness_blob_sha: String,
    pub(super) content: String,
}

#[derive(Debug)]
pub(super) struct HarnessRequestError {
    status: Option<StatusCode>,
    message: String,
}

impl HarnessRequestError {
    fn from_message(message: impl Into<String>) -> Self {
        Self {
            status: None,
            message: message.into(),
        }
    }

    pub(super) fn is_not_found(&self) -> bool {
        self.status == Some(StatusCode::NOT_FOUND)
            || self.message.to_ascii_lowercase().contains("not found")
    }
}

impl std::fmt::Display for HarnessRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(status) = self.status {
            write!(f, "{status} {}", self.message)
        } else {
            f.write_str(self.message.as_str())
        }
    }
}

pub(super) async fn read_harness_file(
    ctx: &HarnessMcpContext,
    path: &str,
) -> Result<HarnessFile, String> {
    let content = fetch_harness_content(ctx, path)
        .await
        .map_err(|err| err.to_string())?;
    if content.kind != "file" {
        return Err("Target is not a file.".to_string());
    }
    let path = if content.path.trim().is_empty() {
        path.to_string()
    } else {
        content.path
    };
    let file_content: HarnessFileContent = serde_json::from_value(content.content)
        .map_err(|err| format!("parse Harness file content failed: {err}"))?;
    let bytes = decode_harness_file_content(&file_content)?;
    if file_content.size > DEFAULT_MAX_FILE_BYTES {
        return Err(format!("File too large ({} bytes).", file_content.size));
    }
    if bytes.len() as i64 > DEFAULT_MAX_FILE_BYTES {
        return Err(format!("File too large ({} bytes).", bytes.len()));
    }
    if file_content.data_size > 0 && file_content.size > file_content.data_size {
        return Err(format!(
            "File too large or truncated ({} bytes).",
            file_content.size
        ));
    }
    if bytes.contains(&0) {
        return Err("Binary file not supported.".to_string());
    }
    let text = String::from_utf8_lossy(bytes.as_slice()).to_string();
    Ok(HarnessFile {
        path,
        size: file_content.size.max(bytes.len() as i64),
        sha256: sha256_hex(bytes.as_slice()),
        harness_blob_sha: content.sha,
        content: text,
    })
}

pub(super) async fn fetch_harness_content(
    ctx: &HarnessMcpContext,
    path: &str,
) -> Result<HarnessContentResponse, HarnessRequestError> {
    let endpoint = harness_repo_url(
        ctx.access.base_url.as_str(),
        ctx.repo_path.as_str(),
        "content",
        Some(path),
    );
    harness_request_json::<HarnessContentResponse, ()>(
        &ctx.client,
        Method::GET,
        endpoint.as_str(),
        ctx.access.access_token.as_str(),
        None,
    )
    .await
}

pub(super) async fn list_harness_paths(
    ctx: &HarnessMcpContext,
) -> Result<HarnessListPathsResponse, String> {
    let endpoint = format!(
        "{}/api/v1/repos/{}/+/paths?include_directories=true",
        ctx.access.base_url.trim().trim_end_matches('/'),
        encode_path_segments(ctx.repo_path.as_str())
    );
    harness_request_json::<HarnessListPathsResponse, ()>(
        &ctx.client,
        Method::GET,
        endpoint.as_str(),
        ctx.access.access_token.as_str(),
        None,
    )
    .await
    .map_err(|err| err.to_string())
}

pub(super) async fn list_harness_branches(
    ctx: &HarnessMcpContext,
) -> Result<Vec<HarnessBranch>, String> {
    let endpoint = format!(
        "{}/api/v1/repos/{}/+/branches",
        ctx.access.base_url.trim().trim_end_matches('/'),
        encode_path_segments(ctx.repo_path.as_str())
    );
    harness_request_json::<Vec<HarnessBranch>, ()>(
        &ctx.client,
        Method::GET,
        endpoint.as_str(),
        ctx.access.access_token.as_str(),
        None,
    )
    .await
    .map_err(|err| err.to_string())
}

pub(super) async fn commit_single_file_action(
    ctx: &HarnessMcpContext,
    action: &str,
    path: &str,
    payload: Option<&str>,
    sha: Option<String>,
    title: &str,
) -> Result<Value, String> {
    let action_payload = HarnessCommitAction {
        action: action.to_string(),
        path: path.to_string(),
        payload: payload.map(ToOwned::to_owned),
        encoding: payload.map(|_| "utf8".to_string()),
        sha,
    };
    commit_file_actions(ctx, title, vec![action_payload]).await
}

pub(super) async fn commit_file_actions(
    ctx: &HarnessMcpContext,
    title: &str,
    actions: Vec<HarnessCommitAction>,
) -> Result<Value, String> {
    ensure_action_count(actions.len())?;
    let body = HarnessCommitRequest {
        title: title.to_string(),
        message: format!(
            "Applied by Chatos Project Service for project {}",
            ctx.project_id
        ),
        branch: None,
        actions,
    };
    let endpoint = format!(
        "{}/api/v1/repos/{}/+/commits/",
        ctx.access.base_url.trim().trim_end_matches('/'),
        encode_path_segments(ctx.repo_path.as_str())
    );
    harness_request_json::<Value, _>(
        &ctx.client,
        Method::POST,
        endpoint.as_str(),
        ctx.access.access_token.as_str(),
        Some(&body),
    )
    .await
    .map_err(|err| err.to_string())
}

pub(super) fn ensure_action_count(count: usize) -> Result<(), String> {
    if count > MAX_COMMIT_ACTIONS {
        Err(format!(
            "Harness commit action count exceeds limit: {count} > {MAX_COMMIT_ACTIONS}"
        ))
    } else {
        Ok(())
    }
}

async fn harness_request_json<TResp, TBody>(
    client: &reqwest::Client,
    method: Method,
    endpoint: &str,
    bearer_token: &str,
    body: Option<&TBody>,
) -> Result<TResp, HarnessRequestError>
where
    TResp: serde::de::DeserializeOwned,
    TBody: Serialize + ?Sized,
{
    let mut request = client
        .request(method, endpoint)
        .bearer_auth(bearer_token.trim());
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request
        .send()
        .await
        .map_err(|err| HarnessRequestError::from_message(err.to_string()))?;
    if !response.status().is_success() {
        let status = response.status();
        let text =
            read_response_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES).await;
        return Err(HarnessRequestError {
            status: Some(status),
            message: text,
        });
    }
    response
        .json::<TResp>()
        .await
        .map_err(|err| HarnessRequestError::from_message(err.to_string()))
}

fn harness_repo_url(
    base_url: &str,
    repo_path: &str,
    operation: &str,
    path: Option<&str>,
) -> String {
    let mut url = format!(
        "{}/api/v1/repos/{}/+/{operation}",
        base_url.trim().trim_end_matches('/'),
        encode_path_segments(repo_path)
    );
    if let Some(path) = path {
        url.push('/');
        if !path.is_empty() {
            url.push_str(encode_path_segments(path).as_str());
        }
    }
    url
}

fn encode_path_segments(path: &str) -> String {
    path.trim()
        .trim_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .map(|part| urlencoding::encode(part).into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn decode_harness_file_content(content: &HarnessFileContent) -> Result<Vec<u8>, String> {
    if content.encoding.eq_ignore_ascii_case("base64") {
        return base64::engine::general_purpose::STANDARD
            .decode(content.data.as_bytes())
            .map_err(|err| format!("decode Harness base64 content failed: {err}"));
    }
    Ok(content.data.as_bytes().to_vec())
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harness_commit_request_omits_branch_to_use_repo_default() {
        let body = HarnessCommitRequest {
            title: "test".to_string(),
            message: "message".to_string(),
            branch: None,
            actions: vec![HarnessCommitAction {
                action: "CREATE".to_string(),
                path: "README.md".to_string(),
                payload: Some("hello".to_string()),
                encoding: Some("utf8".to_string()),
                sha: None,
            }],
        };

        let value = serde_json::to_value(body).expect("serialize body");

        assert!(value.get("branch").is_none());
    }
}
