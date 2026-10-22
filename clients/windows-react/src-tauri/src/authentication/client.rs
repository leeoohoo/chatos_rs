use reqwest::{Client, StatusCode, Url};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::time::Duration;

use super::{AuthSession, AuthUser, RegistrationCodeDelivery};

const MAXIMUM_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone)]
pub struct AuthenticationApiClient {
    client: Client,
    user_service_url: Url,
    gateway_url: Url,
}

#[derive(Debug)]
pub struct ApiError {
    pub message: String,
    pub unauthorized: bool,
}

struct ApiReply<T> {
    value: T,
    refreshed_token: Option<String>,
}

#[derive(Serialize)]
struct LoginRequest<'a> {
    username: &'a str,
    password: &'a str,
}

#[derive(Serialize)]
struct SendRegistrationCodeRequest<'a> {
    email: &'a str,
    invite_code: &'a str,
}

#[derive(Serialize)]
struct RegisterRequest<'a> {
    email: &'a str,
    password: &'a str,
    invite_code: &'a str,
    verification_code: &'a str,
}

#[derive(Deserialize)]
struct LoginResponse {
    #[serde(alias = "access_token")]
    token: String,
    user: AuthUser,
}

#[derive(Deserialize)]
struct MeResponse {
    user: AuthUser,
}

#[derive(Deserialize)]
struct RegistrationCodeResponse {
    expires_in_seconds: i64,
    resend_after_seconds: i64,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct GatewayModelConfig {
    pub id: String,
    pub owner_user_id: String,
    pub name: String,
    pub provider: String,
    pub model: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub task_thinking_level: Option<String>,
    pub temperature: Option<f64>,
    pub max_output_tokens: Option<i64>,
    pub enabled: bool,
    #[serde(default)]
    pub task_enabled: Option<bool>,
    #[serde(default)]
    pub has_api_key: bool,
    #[serde(default)]
    pub supports_images: bool,
    #[serde(default)]
    pub supports_reasoning: bool,
    #[serde(default)]
    pub supports_responses: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct GatewayAgentCapability {
    pub agent_key: String,
    pub owner_user_id: String,
    pub policy_revision: String,
    #[serde(default = "default_true")]
    pub agent_enabled: bool,
    #[serde(default)]
    pub mcps: Vec<GatewayResolvedCapability>,
    #[serde(default)]
    pub plugins: Vec<GatewayResolvedCapability>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct GatewayResolvedCapability {
    pub binding: GatewayCapabilityBinding,
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub status: String,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct GatewayCapabilityBinding {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub required: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Default, Deserialize)]
struct ErrorPayload {
    message: Option<String>,
    error: Option<serde_json::Value>,
    detail: Option<String>,
}

impl AuthenticationApiClient {
    pub fn from_runtime() -> Result<Self, String> {
        Self::new(&runtime_api_base_url())
    }

    fn new(base_url: &str) -> Result<Self, String> {
        let mut url = Url::parse(base_url).map_err(|_| "ChatOS API 地址无效。".to_owned())?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err("ChatOS API 地址无效。".into());
        }
        let prefix = url.path().trim_matches('/').to_owned();
        let mut gateway_url = url.clone();
        let gateway_path = if prefix.is_empty() {
            "/".to_owned()
        } else {
            // A trailing slash makes all control-plane paths relative to the deployment prefix.
            format!("/{prefix}/")
        };
        gateway_url.set_path(&gateway_path);
        gateway_url.set_query(None);
        gateway_url.set_fragment(None);
        let path = if prefix.is_empty() {
            "/api/user/".to_owned()
        } else {
            format!("/{prefix}/api/user/")
        };
        url.set_path(&path);
        url.set_query(None);
        url.set_fragment(None);
        Ok(Self {
            client: Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|_| "无法初始化 ChatOS 网络客户端。".to_owned())?,
            user_service_url: url,
            gateway_url,
        })
    }

    pub async fn restore(&self, token: &str) -> Result<(AuthSession, Option<String>), ApiError> {
        let reply: ApiReply<MeResponse> = self
            .request("auth/me", "GET", None::<&()>, Some(token))
            .await?;
        Ok((
            AuthSession {
                user: reply.value.user,
            },
            reply.refreshed_token,
        ))
    }

    pub async fn login(
        &self,
        username: &str,
        password: &str,
    ) -> Result<(AuthSession, String), ApiError> {
        let reply: ApiReply<LoginResponse> = self
            .request(
                "auth/login",
                "POST",
                Some(&LoginRequest { username, password }),
                None,
            )
            .await?;
        Ok((
            AuthSession {
                user: reply.value.user,
            },
            reply.value.token,
        ))
    }

    pub async fn send_registration_code(
        &self,
        email: &str,
        invite_code: &str,
    ) -> Result<RegistrationCodeDelivery, ApiError> {
        let reply: ApiReply<RegistrationCodeResponse> = self
            .request(
                "auth/register/send-code",
                "POST",
                Some(&SendRegistrationCodeRequest { email, invite_code }),
                None,
            )
            .await?;
        Ok(RegistrationCodeDelivery {
            expires_in_seconds: reply.value.expires_in_seconds,
            resend_after_seconds: reply.value.resend_after_seconds,
        })
    }

    pub async fn register(
        &self,
        email: &str,
        password: &str,
        invite_code: &str,
        verification_code: &str,
    ) -> Result<(AuthSession, String), ApiError> {
        let reply: ApiReply<LoginResponse> = self
            .request(
                "auth/register",
                "POST",
                Some(&RegisterRequest {
                    email,
                    password,
                    invite_code,
                    verification_code,
                }),
                None,
            )
            .await?;
        Ok((
            AuthSession {
                user: reply.value.user,
            },
            reply.value.token,
        ))
    }

    pub(crate) async fn model_configs(
        &self,
        access_token: &str,
    ) -> Result<Vec<GatewayModelConfig>, ApiError> {
        self.control_plane_request("api/model-configs", None, access_token)
            .await
    }

    pub(crate) async fn model_config(
        &self,
        access_token: &str,
        model_config_ref: &str,
    ) -> Result<GatewayModelConfig, ApiError> {
        self.control_plane_request(
            "api/model-configs",
            Some((model_config_ref, "include_secret=true")),
            access_token,
        )
        .await
    }

    pub(crate) async fn local_agent_capability(
        &self,
        access_token: &str,
    ) -> Result<GatewayAgentCapability, ApiError> {
        self.control_plane_request(
            "api/plugin-management/agent-capabilities",
            Some(("local_agent_execution_agent", "")),
            access_token,
        )
        .await
    }

    pub(crate) async fn authenticated_user_service<R: DeserializeOwned>(
        &self,
        endpoint: &str,
        method: &str,
        access_token: &str,
    ) -> Result<R, ApiError> {
        self.request::<(), R>(endpoint, method, None, Some(access_token))
            .await
            .map(|reply| reply.value)
    }

    async fn control_plane_request<R: DeserializeOwned>(
        &self,
        base_path: &str,
        tail_and_query: Option<(&str, &str)>,
        access_token: &str,
    ) -> Result<R, ApiError> {
        let mut url = self.gateway_url.clone();
        {
            let mut segments = url.path_segments_mut().map_err(|_| ApiError {
                message: "ChatOS API 地址无效。".into(),
                unauthorized: false,
            })?;
            segments.pop_if_empty();
            for segment in base_path.split('/') {
                segments.push(segment);
            }
            if let Some((tail, _)) = tail_and_query {
                segments.push(tail);
            }
        }
        if let Some((_, query)) = tail_and_query {
            if !query.is_empty() {
                url.set_query(Some(query));
            }
        }
        let reply = self
            .send::<(), R>(url, "GET", None, Some(access_token))
            .await?;
        Ok(reply.value)
    }

    async fn request<B: Serialize + ?Sized, R: DeserializeOwned>(
        &self,
        endpoint: &str,
        method: &str,
        body: Option<&B>,
        access_token: Option<&str>,
    ) -> Result<ApiReply<R>, ApiError> {
        let url = self.user_service_url.join(endpoint).map_err(|_| ApiError {
            message: "ChatOS API 地址无效。".into(),
            unauthorized: false,
        })?;
        self.send(url, method, body, access_token).await
    }

    async fn send<B: Serialize + ?Sized, R: DeserializeOwned>(
        &self,
        url: Url,
        method: &str,
        body: Option<&B>,
        access_token: Option<&str>,
    ) -> Result<ApiReply<R>, ApiError> {
        let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(|_| ApiError {
            message: "请求方法无效。".into(),
            unauthorized: false,
        })?;
        let mut request = self
            .client
            .request(method, url)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .header("X-Chatos-Client-Surface", "local-connector-desktop");
        if let Some(token) = access_token {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request.send().await.map_err(|_| ApiError {
            message: "无法连接 ChatOS 服务，请检查网络后重试。".into(),
            unauthorized: false,
        })?;
        let status = response.status();
        let refreshed_token = response
            .headers()
            .get("x-access-token")
            .and_then(|value| value.to_str().ok())
            .and_then(non_empty_owned);
        if response
            .content_length()
            .is_some_and(|size| size > MAXIMUM_RESPONSE_BYTES as u64)
        {
            return Err(ApiError {
                message: "服务器响应过大。".into(),
                unauthorized: false,
            });
        }
        let bytes = response.bytes().await.map_err(|_| ApiError {
            message: "无法读取服务器响应。".into(),
            unauthorized: false,
        })?;
        if bytes.len() > MAXIMUM_RESPONSE_BYTES {
            return Err(ApiError {
                message: "服务器响应过大。".into(),
                unauthorized: false,
            });
        }
        if !status.is_success() {
            return Err(ApiError {
                message: error_message(status, &bytes),
                unauthorized: status == StatusCode::UNAUTHORIZED,
            });
        }
        let source = if bytes.is_empty() {
            b"null".as_slice()
        } else {
            bytes.as_ref()
        };
        let value = serde_json::from_slice(source).map_err(|_| ApiError {
            message: "服务器返回了无法识别的数据。".into(),
            unauthorized: false,
        })?;
        Ok(ApiReply {
            value,
            refreshed_token,
        })
    }
}

pub(crate) fn runtime_api_base_url() -> String {
    if let Ok(value) = std::env::var("CHATOS_API_BASE_URL") {
        if !value.trim().is_empty() {
            return value;
        }
    }
    if std::env::var("CHATOS_DEPLOYMENT_PROFILE").as_deref() == Ok("production")
        || !cfg!(debug_assertions)
    {
        "https://gateway.jgoool.com".into()
    } else {
        "http://127.0.0.1:9080".into()
    }
}

fn error_message(status: StatusCode, bytes: &[u8]) -> String {
    if let Ok(payload) = serde_json::from_slice::<ErrorPayload>(bytes) {
        if let Some(message) = payload.message.as_deref().and_then(non_empty_owned) {
            return message;
        }
        if let Some(error) = payload.error {
            if let Some(message) = error.as_str().and_then(non_empty_owned) {
                if matches!(
                    message.as_str(),
                    "register via user_service failed"
                        | "send register verification code via user_service failed"
                ) {
                    if let Some(detail) = payload.detail.as_deref().and_then(non_empty_owned) {
                        return detail;
                    }
                }
                return message;
            }
            if let Some(message) = error
                .get("message")
                .and_then(|value| value.as_str())
                .and_then(non_empty_owned)
            {
                return message;
            }
        }
        if let Some(detail) = payload.detail.as_deref().and_then(non_empty_owned) {
            return detail;
        }
    }
    match status.as_u16() {
        401 => "登录状态已失效，请重新登录。",
        502 => "服务网关暂时无法连接后端，请稍后重试。",
        503 => "服务正在启动或暂时不可用，请稍后重试。",
        504 => "服务响应超时，请稍后重试。",
        _ => "服务器请求失败，请稍后重试。",
    }
    .into()
}

fn non_empty_owned(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_the_same_user_service_prefix_as_macos() {
        let client = AuthenticationApiClient::new("https://example.test/prefix").unwrap();
        assert_eq!(
            client.user_service_url.as_str(),
            "https://example.test/prefix/api/user/"
        );
        assert_eq!(client.gateway_url.as_str(), "https://example.test/prefix/");
    }

    #[test]
    fn login_response_accepts_both_token_field_names() {
        for field in ["token", "access_token"] {
            let body = format!(
                r#"{{"{field}":"secret","user":{{"id":"u1","username":"a@example.com","display_name":null,"role":"user"}}}}"#
            );
            let response: LoginResponse = serde_json::from_str(&body).unwrap();
            assert_eq!(response.token, "secret");
            assert_eq!(response.user.id, "u1");
        }
    }

    #[test]
    fn rejects_urls_with_embedded_credentials() {
        assert!(AuthenticationApiClient::new("https://user:secret@example.test").is_err());
    }
}
