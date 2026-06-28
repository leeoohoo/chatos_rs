use reqwest::Method;
use serde::Serialize;

mod http;
mod types;

use http::{request_empty, request_json};
pub use types::{
    CreateUserServiceAgentAccountRequest, CreateUserServiceModelConfigRequest,
    CreateUserServiceModelProviderRequest, UpdateUserServiceModelConfigRequest,
    UpdateUserServiceModelProviderRequest, UpdateUserServiceModelSettingsRequest,
    UserServiceAgentAccountSummary, UserServiceAuthUser, UserServiceLoginResponse,
    UserServiceMeResponse, UserServiceModelConfigRecord, UserServiceModelProviderRecord,
    UserServiceModelSettingsRecord, UserServiceVerifyResponse,
};

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

pub async fn verify_token(
    base_url: &str,
    access_token: &str,
    timeout_ms: i64,
) -> Result<UserServiceVerifyResponse, String> {
    request_json::<(), _>(
        Method::GET,
        base_url,
        "/api/auth/verify",
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

pub async fn list_model_configs(
    base_url: &str,
    access_token: &str,
    user_id: Option<&str>,
    timeout_ms: i64,
) -> Result<Vec<UserServiceModelConfigRecord>, String> {
    let path = match user_id.map(str::trim).filter(|value| !value.is_empty()) {
        Some(user_id) => format!(
            "/api/model-configs?user_id={}",
            urlencoding::encode(user_id)
        ),
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

pub async fn list_model_providers(
    base_url: &str,
    access_token: &str,
    user_id: Option<&str>,
    timeout_ms: i64,
) -> Result<Vec<UserServiceModelProviderRecord>, String> {
    let path = match user_id.map(str::trim).filter(|value| !value.is_empty()) {
        Some(user_id) => format!(
            "/api/model-providers?user_id={}",
            urlencoding::encode(user_id)
        ),
        None => "/api/model-providers".to_string(),
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

pub async fn get_model_provider(
    base_url: &str,
    access_token: &str,
    id: &str,
    include_secret: bool,
    timeout_ms: i64,
) -> Result<UserServiceModelProviderRecord, String> {
    let path = if include_secret {
        format!(
            "/api/model-providers/{}?include_secret=true",
            urlencoding::encode(id.trim())
        )
    } else {
        format!("/api/model-providers/{}", urlencoding::encode(id.trim()))
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

pub async fn get_model_settings(
    base_url: &str,
    access_token: &str,
    user_id: Option<&str>,
    timeout_ms: i64,
) -> Result<UserServiceModelSettingsRecord, String> {
    let path = match user_id.map(str::trim).filter(|value| !value.is_empty()) {
        Some(user_id) => format!(
            "/api/model-configs/settings?user_id={}",
            urlencoding::encode(user_id)
        ),
        None => "/api/model-configs/settings".to_string(),
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

pub async fn update_model_settings(
    base_url: &str,
    access_token: &str,
    payload: &UpdateUserServiceModelSettingsRequest,
    timeout_ms: i64,
) -> Result<UserServiceModelSettingsRecord, String> {
    request_json(
        Method::PUT,
        base_url,
        "/api/model-configs/settings",
        Some(access_token),
        Some(payload),
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

pub async fn create_model_provider(
    base_url: &str,
    access_token: &str,
    payload: &CreateUserServiceModelProviderRequest,
    timeout_ms: i64,
) -> Result<UserServiceModelProviderRecord, String> {
    request_json(
        Method::POST,
        base_url,
        "/api/model-providers",
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

pub async fn update_model_provider(
    base_url: &str,
    access_token: &str,
    id: &str,
    payload: &UpdateUserServiceModelProviderRequest,
    timeout_ms: i64,
) -> Result<UserServiceModelProviderRecord, String> {
    let path = format!("/api/model-providers/{}", urlencoding::encode(id.trim()));
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

pub async fn refresh_model_config(
    base_url: &str,
    access_token: &str,
    id: &str,
    payload: &UpdateUserServiceModelConfigRequest,
    timeout_ms: i64,
) -> Result<UserServiceModelConfigRecord, String> {
    let path = format!(
        "/api/model-configs/{}/refresh",
        urlencoding::encode(id.trim())
    );
    request_json(
        Method::POST,
        base_url,
        path.as_str(),
        Some(access_token),
        Some(payload),
        timeout_ms,
    )
    .await
}

pub async fn refresh_model_provider(
    base_url: &str,
    access_token: &str,
    id: &str,
    payload: &UpdateUserServiceModelProviderRequest,
    timeout_ms: i64,
) -> Result<UserServiceModelProviderRecord, String> {
    let path = format!(
        "/api/model-providers/{}/refresh",
        urlencoding::encode(id.trim())
    );
    request_json(
        Method::POST,
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

pub async fn delete_model_provider(
    base_url: &str,
    access_token: &str,
    id: &str,
    timeout_ms: i64,
) -> Result<(), String> {
    let path = format!("/api/model-providers/{}", urlencoding::encode(id.trim()));
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
