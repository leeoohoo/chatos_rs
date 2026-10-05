// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

use crate::http_body::{
    read_response_json_limited, read_response_preview_text_limited_or_message,
    ERROR_BODY_PREVIEW_LIMIT_BYTES, JSON_BODY_LIMIT_BYTES,
};

#[derive(Debug, Serialize)]
pub struct UserServiceLoginRequest<'a> {
    pub username: &'a str,
    pub password: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct UserServiceAuthUser {
    pub id: String,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub role: Option<String>,
    pub principal_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UserServiceLoginResponse {
    pub token: String,
    pub user: UserServiceAuthUser,
}

#[derive(Debug, Deserialize)]
pub struct UserServiceVerifiedPrincipal {
    #[serde(default)]
    pub jti: String,
    pub principal_type: String,
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub role: Option<String>,
    pub agent_account_id: Option<String>,
    pub owner_user_id: Option<String>,
    pub owner_username: Option<String>,
    pub owner_display_name: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct UserServiceVerifyResponse {
    pub principal: UserServiceVerifiedPrincipal,
}

pub async fn request_user_service_json<TBody, TResponse>(
    client: &reqwest::Client,
    base_url: &str,
    method: reqwest::Method,
    path: &str,
    access_token: Option<&str>,
    body: Option<&TBody>,
) -> Result<TResponse, String>
where
    TBody: Serialize + ?Sized,
    TResponse: serde::de::DeserializeOwned,
{
    let endpoint = format!("{}{}", base_url.trim().trim_end_matches('/'), path);
    let mut request = client.request(method, endpoint);
    if let Some(access_token) = access_token {
        request = request.bearer_auth(access_token.trim());
    }
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request
        .send()
        .await
        .map_err(|err| format!("user_service request failed: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let text =
            read_response_preview_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES)
                .await;
        return Err(if text.trim().is_empty() {
            format!("user_service request failed with status {status}")
        } else {
            text
        });
    }
    read_response_json_limited::<TResponse>(response, JSON_BODY_LIMIT_BYTES)
        .await
        .map_err(|err| format!("parse user_service response failed: {err}"))
}
