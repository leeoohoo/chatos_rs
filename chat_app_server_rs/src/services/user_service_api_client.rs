use chrono::Utc;
use jsonwebtoken::{encode, EncodingKey, Header};
use reqwest::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::config::Config;

#[derive(Debug, Deserialize)]
pub struct UserServiceAuthUser {
    pub id: String,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub role: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UserServiceLoginResponse {
    pub token: String,
    pub user: UserServiceAuthUser,
}

#[derive(Debug, Deserialize)]
pub struct UserServiceMeResponse {
    pub user: UserServiceAuthUser,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UserServiceAgentAccountSummary {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub owner_user_id: String,
    pub owner_username: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateUserServiceAgentAccountRequest {
    pub username: String,
    pub display_name: Option<String>,
    pub password: String,
    pub owner_user_id: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserServiceModelConfigRecord {
    pub id: String,
    pub owner_user_id: String,
    pub name: String,
    pub provider: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub model_name: String,
    pub thinking_level: Option<String>,
    pub api_key: Option<String>,
    #[serde(default)]
    pub has_api_key: bool,
    pub base_url: Option<String>,
    pub enabled: bool,
    #[serde(default)]
    pub supports_images: bool,
    #[serde(default)]
    pub supports_reasoning: bool,
    #[serde(default)]
    pub supports_responses: bool,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub sync_warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateUserServiceModelConfigRequest {
    pub id: Option<String>,
    pub owner_user_id: Option<String>,
    pub name: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub thinking_level: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub enabled: Option<bool>,
    pub supports_images: Option<bool>,
    pub supports_reasoning: Option<bool>,
    pub supports_responses: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct UpdateUserServiceModelConfigRequest {
    pub name: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub thinking_level: Option<String>,
    pub api_key: Option<String>,
    pub clear_api_key: Option<bool>,
    pub base_url: Option<String>,
    pub enabled: Option<bool>,
    pub supports_images: Option<bool>,
    pub supports_reasoning: Option<bool>,
    pub supports_responses: Option<bool>,
}

#[derive(Debug, Serialize)]
struct UserServiceProxyClaims {
    iss: String,
    aud: String,
    sub: String,
    exp: usize,
    iat: usize,
    jti: String,
    principal_type: String,
    user_id: Option<String>,
    username: Option<String>,
    display_name: Option<String>,
    role: Option<String>,
    agent_account_id: Option<String>,
    owner_user_id: Option<String>,
    owner_username: Option<String>,
    scopes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct UserServiceAuthRequest<'a> {
    username: &'a str,
    password: &'a str,
}

pub async fn login(
    base_url: &str,
    username: &str,
    password: &str,
    timeout_ms: i64,
) -> Result<UserServiceLoginResponse, String> {
    request_json(
        Method::POST,
        base_url,
        "/api/auth/login",
        None,
        Some(&UserServiceAuthRequest { username, password }),
        timeout_ms,
    )
    .await
}

pub async fn register(
    base_url: &str,
    username: &str,
    password: &str,
    timeout_ms: i64,
) -> Result<UserServiceLoginResponse, String> {
    request_json(
        Method::POST,
        base_url,
        "/api/auth/register",
        None,
        Some(&UserServiceAuthRequest { username, password }),
        timeout_ms,
    )
    .await
}

pub async fn get_me(
    base_url: &str,
    access_token: &str,
    timeout_ms: i64,
) -> Result<UserServiceMeResponse, String> {
    request_json::<(), _>(
        Method::GET,
        base_url,
        "/api/auth/me",
        Some(access_token),
        None,
        timeout_ms,
    )
    .await
}

pub async fn list_agent_accounts(
    base_url: &str,
    access_token: &str,
    timeout_ms: i64,
) -> Result<Vec<UserServiceAgentAccountSummary>, String> {
    request_json::<(), _>(
        Method::GET,
        base_url,
        "/api/agent-accounts",
        Some(access_token),
        None,
        timeout_ms,
    )
    .await
}

pub async fn create_agent_account(
    base_url: &str,
    access_token: &str,
    payload: &CreateUserServiceAgentAccountRequest,
    timeout_ms: i64,
) -> Result<UserServiceAgentAccountSummary, String> {
    request_json(
        Method::POST,
        base_url,
        "/api/agent-accounts",
        Some(access_token),
        Some(payload),
        timeout_ms,
    )
    .await
}

pub fn build_user_access_token(user_id: &str, role: Option<&str>) -> Result<String, String> {
    let cfg = Config::try_get()?;
    let secret = cfg
        .user_service_jwt_secret
        .as_deref()
        .ok_or_else(|| "user_service jwt secret is not configured".to_string())?;
    let now = Utc::now().timestamp().max(0) as usize;
    let exp = (Utc::now().timestamp() + 3600).max(0) as usize;
    encode(
        &Header::default(),
        &UserServiceProxyClaims {
            iss: cfg.user_service_jwt_issuer.clone(),
            aud: cfg.user_service_user_audience.clone(),
            sub: format!("user:{user_id}"),
            exp,
            iat: now,
            jti: Uuid::new_v4().to_string(),
            principal_type: "human_user".to_string(),
            user_id: Some(user_id.trim().to_string()),
            username: None,
            display_name: None,
            role: Some(role.unwrap_or("user").trim().to_string()),
            agent_account_id: None,
            owner_user_id: None,
            owner_username: None,
            scopes: vec!["user_service".to_string()],
        },
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|err| err.to_string())
}

pub async fn list_model_configs(
    base_url: &str,
    access_token: &str,
    user_id: Option<&str>,
    timeout_ms: i64,
) -> Result<Vec<UserServiceModelConfigRecord>, String> {
    let path = match user_id.map(str::trim).filter(|value| !value.is_empty()) {
        Some(user_id) => format!("/api/model-configs?user_id={}", urlencoding::encode(user_id)),
        None => "/api/model-configs".to_string(),
    };
    request_json::<(), _>(
        Method::GET,
        base_url,
        path.as_str(),
        Some(access_token),
        None,
        timeout_ms,
    )
    .await
}

pub async fn get_model_config(
    base_url: &str,
    access_token: &str,
    id: &str,
    include_secret: bool,
    timeout_ms: i64,
) -> Result<UserServiceModelConfigRecord, String> {
    let path = if include_secret {
        format!(
            "/api/model-configs/{}?include_secret=true",
            urlencoding::encode(id.trim())
        )
    } else {
        format!("/api/model-configs/{}", urlencoding::encode(id.trim()))
    };
    request_json::<(), _>(
        Method::GET,
        base_url,
        path.as_str(),
        Some(access_token),
        None,
        timeout_ms,
    )
    .await
}

pub async fn create_model_config(
    base_url: &str,
    access_token: &str,
    payload: &CreateUserServiceModelConfigRequest,
    timeout_ms: i64,
) -> Result<UserServiceModelConfigRecord, String> {
    request_json(
        Method::POST,
        base_url,
        "/api/model-configs",
        Some(access_token),
        Some(payload),
        timeout_ms,
    )
    .await
}

pub async fn update_model_config(
    base_url: &str,
    access_token: &str,
    id: &str,
    payload: &UpdateUserServiceModelConfigRequest,
    timeout_ms: i64,
) -> Result<UserServiceModelConfigRecord, String> {
    let path = format!("/api/model-configs/{}", urlencoding::encode(id.trim()));
    request_json(
        Method::PATCH,
        base_url,
        path.as_str(),
        Some(access_token),
        Some(payload),
        timeout_ms,
    )
    .await
}

pub async fn delete_model_config(
    base_url: &str,
    access_token: &str,
    id: &str,
    timeout_ms: i64,
) -> Result<(), String> {
    let path = format!("/api/model-configs/{}", urlencoding::encode(id.trim()));
    request_empty::<()>(
        Method::DELETE,
        base_url,
        path.as_str(),
        Some(access_token),
        None,
        timeout_ms,
    )
    .await
}

async fn request_json<TBody, TResp>(
    method: Method,
    base_url: &str,
    path: &str,
    access_token: Option<&str>,
    body: Option<&TBody>,
    timeout_ms: i64,
) -> Result<TResp, String>
where
    TBody: Serialize + ?Sized,
    TResp: serde::de::DeserializeOwned,
{
    let endpoint = format!("{}{}", base_url.trim().trim_end_matches('/'), path);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms.max(300) as u64))
        .build()
        .map_err(|err| err.to_string())?;
    let mut request = client.request(method, endpoint);
    if let Some(access_token) = access_token {
        request = request.bearer_auth(access_token.trim());
    }
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request.send().await.map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "user_service request failed: {} {}",
            status.as_u16(),
            extract_error_message(status, body.as_str())
        ));
    }
    response
        .json::<TResp>()
        .await
        .map_err(|err| err.to_string())
}

async fn request_empty<TBody>(
    method: Method,
    base_url: &str,
    path: &str,
    access_token: Option<&str>,
    body: Option<&TBody>,
    timeout_ms: i64,
) -> Result<(), String>
where
    TBody: Serialize + ?Sized,
{
    let endpoint = format!("{}{}", base_url.trim().trim_end_matches('/'), path);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms.max(300) as u64))
        .build()
        .map_err(|err| err.to_string())?;
    let mut request = client.request(method, endpoint);
    if let Some(access_token) = access_token {
        request = request.bearer_auth(access_token.trim());
    }
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request.send().await.map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "user_service request failed: {} {}",
            status.as_u16(),
            extract_error_message(status, body.as_str())
        ));
    }
    Ok(())
}

fn extract_error_message(status: StatusCode, body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|item| item.as_str())
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| format!("HTTP {}", status.as_u16()))
}

#[cfg(test)]
mod tests {
    use super::{
        create_agent_account, get_me, list_agent_accounts, login,
        CreateUserServiceAgentAccountRequest,
    };
    use axum::{
        routing::{get, post},
        Json, Router,
    };
    use serde_json::{json, Value};

    async fn start_test_server(app: Router) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test server");
        let addr = listener.local_addr().expect("read test server addr");
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (format!("http://{addr}"), handle)
    }

    #[tokio::test]
    async fn login_parses_user_service_response() {
        let app = Router::new().route(
            "/api/auth/login",
            post(|| async {
                Json(json!({
                    "token": "user-service-token",
                    "user": {
                        "id": "user-1",
                        "username": "alice",
                        "display_name": "Alice",
                        "role": "user"
                    }
                }))
            }),
        );
        let (base_url, handle) = start_test_server(app).await;

        let response = login(base_url.as_str(), "alice", "secret", 3000)
            .await
            .expect("login response");

        assert_eq!(response.token, "user-service-token");
        assert_eq!(response.user.id, "user-1");
        assert_eq!(response.user.username.as_deref(), Some("alice"));
        assert_eq!(response.user.display_name.as_deref(), Some("Alice"));
        assert_eq!(response.user.role.as_deref(), Some("user"));

        handle.abort();
    }

    #[tokio::test]
    async fn get_me_parses_user_profile_response() {
        let app = Router::new().route(
            "/api/auth/me",
            get(|| async {
                Json(json!({
                    "user": {
                        "id": "user-2",
                        "username": "bob",
                        "display_name": "Bob",
                        "role": "super_admin"
                    }
                }))
            }),
        );
        let (base_url, handle) = start_test_server(app).await;

        let response = get_me(base_url.as_str(), "bearer-token", 3000)
            .await
            .expect("me response");

        assert_eq!(response.user.id, "user-2");
        assert_eq!(response.user.username.as_deref(), Some("bob"));
        assert_eq!(response.user.display_name.as_deref(), Some("Bob"));
        assert_eq!(response.user.role.as_deref(), Some("super_admin"));

        handle.abort();
    }

    #[tokio::test]
    async fn list_agent_accounts_extracts_remote_error_message() {
        let app = Router::new().route(
            "/api/agent-accounts",
            get(|| async {
                (
                    axum::http::StatusCode::FORBIDDEN,
                    Json(json!({ "error": "forbidden by user service" })),
                )
            }),
        );
        let (base_url, handle) = start_test_server(app).await;

        let error = list_agent_accounts(base_url.as_str(), "bearer-token", 3000)
            .await
            .expect_err("expected remote error");

        assert!(error.contains("403"));
        assert!(error.contains("forbidden by user service"));

        handle.abort();
    }

    #[tokio::test]
    async fn list_agent_accounts_parses_items() {
        let app = Router::new().route(
            "/api/agent-accounts",
            get(|| async {
                Json(Value::Array(vec![json!({
                    "id": "agent-1",
                    "username": "agent-alpha",
                    "display_name": "Agent Alpha",
                    "owner_user_id": "user-1",
                    "owner_username": "alice",
                    "enabled": true
                })]))
            }),
        );
        let (base_url, handle) = start_test_server(app).await;

        let items = list_agent_accounts(base_url.as_str(), "bearer-token", 3000)
            .await
            .expect("agent account list");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "agent-1");
        assert_eq!(items[0].username, "agent-alpha");
        assert_eq!(items[0].display_name, "Agent Alpha");
        assert_eq!(items[0].owner_user_id, "user-1");
        assert_eq!(items[0].owner_username, "alice");
        assert!(items[0].enabled);

        handle.abort();
    }

    #[tokio::test]
    async fn create_agent_account_posts_payload_and_parses_response() {
        let app = Router::new().route(
            "/api/agent-accounts",
            post(|Json(payload): Json<Value>| async move {
                assert_eq!(
                    payload.get("username").and_then(Value::as_str),
                    Some("agent-alpha")
                );
                assert_eq!(
                    payload.get("display_name").and_then(Value::as_str),
                    Some("Agent Alpha")
                );
                assert_eq!(
                    payload.get("password").and_then(Value::as_str),
                    Some("secret-123")
                );
                assert_eq!(
                    payload.get("owner_user_id").and_then(Value::as_str),
                    Some("user-1")
                );
                assert_eq!(payload.get("enabled").and_then(Value::as_bool), Some(true));
                Json(json!({
                    "id": "agent-1",
                    "username": "agent-alpha",
                    "display_name": "Agent Alpha",
                    "owner_user_id": "user-1",
                    "owner_username": "alice",
                    "enabled": true
                }))
            }),
        );
        let (base_url, handle) = start_test_server(app).await;

        let created = create_agent_account(
            base_url.as_str(),
            "bearer-token",
            &CreateUserServiceAgentAccountRequest {
                username: "agent-alpha".to_string(),
                display_name: Some("Agent Alpha".to_string()),
                password: "secret-123".to_string(),
                owner_user_id: Some("user-1".to_string()),
                enabled: Some(true),
            },
            3000,
        )
        .await
        .expect("create agent account");

        assert_eq!(created.id, "agent-1");
        assert_eq!(created.username, "agent-alpha");
        assert_eq!(created.display_name, "Agent Alpha");
        assert_eq!(created.owner_user_id, "user-1");
        assert_eq!(created.owner_username, "alice");
        assert!(created.enabled);

        handle.abort();
    }
}
