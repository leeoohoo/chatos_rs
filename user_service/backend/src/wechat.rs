// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Duration;

use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::config::AppConfig;

const CODE_MAX_BYTES: usize = 512;
const INVALID_ACCESS_TOKEN_CODES: &[i64] = &[40001, 40014, 42001];

#[derive(Clone)]
pub struct WeChatMiniProgramClient {
    http: reqwest::Client,
    app_id: String,
    app_secret: String,
    api_base_url: String,
    env_version: String,
    access_token_cache: Arc<Mutex<Option<CachedAccessToken>>>,
}

#[derive(Debug, Clone)]
struct CachedAccessToken {
    value: String,
    refresh_after_unix: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeChatMiniProgramIdentity {
    pub open_id: String,
    pub union_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WeChatExchangeError {
    NotConfigured,
    InvalidCode,
    Transport,
    ProviderRejected { code: i64 },
    InvalidResponse,
}

impl WeChatExchangeError {
    pub fn public_message(&self) -> &'static str {
        match self {
            Self::NotConfigured => "WeChat Mini Program login is not configured",
            Self::InvalidCode => "WeChat login code is invalid",
            Self::Transport | Self::InvalidResponse => "WeChat login is temporarily unavailable",
            Self::ProviderRejected { .. } => "WeChat login code was rejected",
        }
    }
}

impl std::fmt::Display for WeChatExchangeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConfigured => formatter.write_str("WeChat Mini Program is not configured"),
            Self::InvalidCode => formatter.write_str("invalid WeChat login code"),
            Self::Transport => formatter.write_str("WeChat code exchange transport failed"),
            Self::ProviderRejected { code } => {
                write!(formatter, "WeChat code exchange rejected with code {code}")
            }
            Self::InvalidResponse => formatter.write_str("invalid WeChat code exchange response"),
        }
    }
}

impl std::error::Error for WeChatExchangeError {}

#[derive(Debug, Deserialize)]
struct CodeExchangeResponse {
    openid: Option<String>,
    unionid: Option<String>,
    errcode: Option<i64>,
    #[allow(dead_code)]
    errmsg: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AccessTokenResponse {
    access_token: Option<String>,
    expires_in: Option<i64>,
    errcode: Option<i64>,
}

#[derive(Debug, Serialize)]
struct MiniProgramCodeRequest<'a> {
    scene: &'a str,
    page: &'static str,
    width: u16,
    check_path: bool,
    env_version: &'a str,
}

#[derive(Debug, Deserialize)]
struct ProviderErrorResponse {
    errcode: Option<i64>,
}

impl WeChatMiniProgramClient {
    pub fn from_config(config: &AppConfig) -> Result<Option<Self>, String> {
        let Some(app_id) = config.wechat_mini_program_app_id.as_deref() else {
            return Ok(None);
        };
        let app_secret = config
            .wechat_mini_program_app_secret
            .as_deref()
            .ok_or_else(|| "WeChat Mini Program app secret is missing".to_string())?;
        let http = reqwest::Client::builder()
            .timeout(Duration::from_millis(
                config.wechat_mini_program_request_timeout_ms.max(500) as u64,
            ))
            .build()
            .map_err(|err| format!("build WeChat Mini Program HTTP client failed: {err}"))?;
        Ok(Some(Self {
            http,
            app_id: app_id.to_string(),
            app_secret: app_secret.to_string(),
            api_base_url: config
                .wechat_mini_program_api_base_url
                .trim_end_matches('/')
                .to_string(),
            env_version: config.wechat_mini_program_env_version.clone(),
            access_token_cache: Arc::new(Mutex::new(None)),
        }))
    }

    pub fn app_id(&self) -> &str {
        self.app_id.as_str()
    }

    pub async fn exchange_code(
        &self,
        code: &str,
    ) -> Result<WeChatMiniProgramIdentity, WeChatExchangeError> {
        let code = normalize_code(code)?;
        let response = self
            .http
            .get(format!("{}/sns/jscode2session", self.api_base_url.as_str()))
            .query(&[
                ("appid", self.app_id.as_str()),
                ("secret", self.app_secret.as_str()),
                ("js_code", code),
                ("grant_type", "authorization_code"),
            ])
            .send()
            .await
            .map_err(|_| WeChatExchangeError::Transport)?;
        let status = response.status();
        let body = response
            .bytes()
            .await
            .map_err(|_| WeChatExchangeError::Transport)?;
        parse_exchange_response(status, body.as_ref())
    }

    pub async fn generate_bind_code(&self, scene: &str) -> Result<Vec<u8>, WeChatExchangeError> {
        let scene = normalize_scene(scene)?;
        let access_token = self.access_token().await?;
        let first = self.request_bind_code(scene, access_token.as_str()).await;
        if !matches!(
            &first,
            Err(WeChatExchangeError::ProviderRejected { code })
                if INVALID_ACCESS_TOKEN_CODES.contains(code)
        ) {
            return first;
        }
        self.invalidate_access_token(access_token.as_str()).await;
        let refreshed_access_token = self.access_token().await?;
        self.request_bind_code(scene, refreshed_access_token.as_str())
            .await
    }

    async fn request_bind_code(
        &self,
        scene: &str,
        access_token: &str,
    ) -> Result<Vec<u8>, WeChatExchangeError> {
        let response = self
            .http
            .post(format!(
                "{}/wxa/getwxacodeunlimit",
                self.api_base_url.as_str()
            ))
            .query(&[("access_token", access_token)])
            .json(&MiniProgramCodeRequest {
                scene,
                page: "pages/bind/index",
                width: 430,
                check_path: false,
                env_version: self.env_version.as_str(),
            })
            .send()
            .await
            .map_err(|_| WeChatExchangeError::Transport)?;
        let status = response.status();
        let body = response
            .bytes()
            .await
            .map_err(|_| WeChatExchangeError::Transport)?;
        parse_mini_program_code_response(status, body.as_ref())
    }

    async fn invalidate_access_token(&self, rejected_token: &str) {
        let mut cache = self.access_token_cache.lock().await;
        if cache
            .as_ref()
            .is_some_and(|cached| cached.value == rejected_token)
        {
            *cache = None;
        }
    }

    async fn access_token(&self) -> Result<String, WeChatExchangeError> {
        let mut cache = self.access_token_cache.lock().await;
        let now = chrono::Utc::now().timestamp();
        if let Some(cached) = cache
            .as_ref()
            .filter(|cached| cached.refresh_after_unix > now)
        {
            return Ok(cached.value.clone());
        }
        let response = self
            .http
            .get(format!("{}/cgi-bin/token", self.api_base_url.as_str()))
            .query(&[
                ("grant_type", "client_credential"),
                ("appid", self.app_id.as_str()),
                ("secret", self.app_secret.as_str()),
            ])
            .send()
            .await
            .map_err(|_| WeChatExchangeError::Transport)?;
        let status = response.status();
        let body = response
            .bytes()
            .await
            .map_err(|_| WeChatExchangeError::Transport)?;
        let (value, expires_in) = parse_access_token_response(status, body.as_ref())?;
        *cache = Some(CachedAccessToken {
            value: value.clone(),
            refresh_after_unix: now + expires_in.saturating_sub(120).max(60),
        });
        Ok(value)
    }
}

fn normalize_code(code: &str) -> Result<&str, WeChatExchangeError> {
    let normalized = code.trim();
    if normalized.is_empty()
        || normalized.len() > CODE_MAX_BYTES
        || normalized.contains(char::is_whitespace)
    {
        return Err(WeChatExchangeError::InvalidCode);
    }
    Ok(normalized)
}

fn normalize_scene(scene: &str) -> Result<&str, WeChatExchangeError> {
    let normalized = scene.trim();
    if normalized.is_empty()
        || normalized.len() > 32
        || !normalized
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(WeChatExchangeError::InvalidCode);
    }
    Ok(normalized)
}

fn parse_access_token_response(
    status: StatusCode,
    body: &[u8],
) -> Result<(String, i64), WeChatExchangeError> {
    if !status.is_success() {
        return Err(WeChatExchangeError::Transport);
    }
    let response: AccessTokenResponse =
        serde_json::from_slice(body).map_err(|_| WeChatExchangeError::InvalidResponse)?;
    if let Some(code) = response.errcode.filter(|code| *code != 0) {
        return Err(WeChatExchangeError::ProviderRejected { code });
    }
    let token = response
        .access_token
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or(WeChatExchangeError::InvalidResponse)?;
    let expires_in = response
        .expires_in
        .filter(|value| *value >= 60)
        .ok_or(WeChatExchangeError::InvalidResponse)?;
    Ok((token, expires_in))
}

fn parse_mini_program_code_response(
    status: StatusCode,
    body: &[u8],
) -> Result<Vec<u8>, WeChatExchangeError> {
    if !status.is_success() {
        return Err(WeChatExchangeError::Transport);
    }
    if body.starts_with(b"\x89PNG\r\n\x1a\n") || body.starts_with(&[0xff, 0xd8, 0xff]) {
        if body.len() > 2 * 1_024 * 1_024 {
            return Err(WeChatExchangeError::InvalidResponse);
        }
        return Ok(body.to_vec());
    }
    let response: ProviderErrorResponse =
        serde_json::from_slice(body).map_err(|_| WeChatExchangeError::InvalidResponse)?;
    if let Some(code) = response.errcode.filter(|code| *code != 0) {
        return Err(WeChatExchangeError::ProviderRejected { code });
    }
    Err(WeChatExchangeError::InvalidResponse)
}

fn parse_exchange_response(
    status: StatusCode,
    body: &[u8],
) -> Result<WeChatMiniProgramIdentity, WeChatExchangeError> {
    if !status.is_success() {
        return Err(WeChatExchangeError::Transport);
    }
    let response: CodeExchangeResponse =
        serde_json::from_slice(body).map_err(|_| WeChatExchangeError::InvalidResponse)?;
    if let Some(code) = response.errcode.filter(|code| *code != 0) {
        return Err(WeChatExchangeError::ProviderRejected { code });
    }
    let open_id =
        normalize_provider_subject(response.openid).ok_or(WeChatExchangeError::InvalidResponse)?;
    Ok(WeChatMiniProgramIdentity {
        open_id,
        union_id: normalize_provider_subject(response.unionid),
    })
}

fn normalize_provider_subject(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && value.len() <= 256)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_blank_or_whitespace_codes() {
        assert_eq!(normalize_code("   "), Err(WeChatExchangeError::InvalidCode));
        assert_eq!(
            normalize_code("code with spaces"),
            Err(WeChatExchangeError::InvalidCode)
        );
    }

    #[test]
    fn parses_success_without_exposing_session_key() {
        let value = parse_exchange_response(
            StatusCode::OK,
            br#"{"openid":"openid-1","session_key":"sensitive","unionid":"union-1"}"#,
        )
        .expect("parse successful exchange");
        assert_eq!(value.open_id, "openid-1");
        assert_eq!(value.union_id.as_deref(), Some("union-1"));
    }

    #[test]
    fn preserves_only_safe_provider_error_code() {
        let error = parse_exchange_response(
            StatusCode::OK,
            br#"{"errcode":40029,"errmsg":"invalid code containing provider details"}"#,
        )
        .expect_err("provider rejection");
        assert_eq!(error, WeChatExchangeError::ProviderRejected { code: 40029 });
        assert!(!error.to_string().contains("provider details"));
    }

    #[test]
    fn rejects_success_payload_without_open_id() {
        assert_eq!(
            parse_exchange_response(StatusCode::OK, br#"{"session_key":"secret"}"#),
            Err(WeChatExchangeError::InvalidResponse)
        );
    }

    #[test]
    fn parses_access_token_without_retaining_provider_message() {
        let (token, expires_in) = parse_access_token_response(
            StatusCode::OK,
            br#"{"access_token":"provider-token","expires_in":7200}"#,
        )
        .expect("parse token");
        assert_eq!(token, "provider-token");
        assert_eq!(expires_in, 7200);
        assert_eq!(
            parse_access_token_response(
                StatusCode::OK,
                br#"{"errcode":40013,"errmsg":"sensitive provider detail"}"#,
            ),
            Err(WeChatExchangeError::ProviderRejected { code: 40013 })
        );
    }

    #[test]
    fn accepts_only_image_bind_code_responses() {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend_from_slice(b"image-data");
        assert_eq!(
            parse_mini_program_code_response(StatusCode::OK, png.as_slice()),
            Ok(png)
        );
        assert_eq!(
            parse_mini_program_code_response(
                StatusCode::OK,
                br#"{"errcode":41030,"errmsg":"page invalid"}"#,
            ),
            Err(WeChatExchangeError::ProviderRejected { code: 41030 })
        );
    }

    #[test]
    fn recognizes_only_wechat_access_token_refresh_errors() {
        for code in [40001, 40014, 42001] {
            assert!(INVALID_ACCESS_TOKEN_CODES.contains(&code));
        }
        for code in [40013, 41030, 45009] {
            assert!(!INVALID_ACCESS_TOKEN_CODES.contains(&code));
        }
    }
}
