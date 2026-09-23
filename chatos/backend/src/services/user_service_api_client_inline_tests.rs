#[cfg(test)]
mod tests {
    use super::{
        create_agent_account, get_internal_model_runtime_config, get_me, list_agent_accounts,
        login, response_status_from_error, CreateUserServiceAgentAccountRequest,
    };
    use axum::{
        extract::Path,
        http::HeaderMap,
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

    #[test]
    fn response_status_is_extracted_from_user_service_error() {
        assert_eq!(
            response_status_from_error(
                "user_service request failed: 401 invalid or expired access token"
            ),
            Some(401)
        );
        assert_eq!(
            response_status_from_error("connection refused by user_service"),
            None
        );
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

    #[tokio::test]
    async fn internal_model_runtime_request_uses_chatos_signed_service_identity() {
        let secret = "a-long-chatos-user-service-secret";
        let app = Router::new().route(
            "/api/internal/users/{user_id}/model-configs/{model_id}/runtime",
            get(
                move |Path((user_id, model_id)): Path<(String, String)>, headers: HeaderMap| async move {
                    assert_eq!(user_id, "user-1");
                    assert_eq!(model_id, "model-1");
                    assert_eq!(
                        headers
                            .get("x-user-service-caller")
                            .and_then(|value| value.to_str().ok()),
                        Some("chatos-backend")
                    );
                    let token = headers
                        .get("x-user-service-internal-token")
                        .and_then(|value| value.to_str().ok())
                        .expect("signed internal token");
                    chatos_service_runtime::verify_internal_service_token(
                        token,
                        secret,
                        "chatos-backend",
                        "user-service",
                        "model-runtime.read",
                    )
                    .expect("valid ChatOS service identity");
                    Json(json!({
                        "id": "model-1",
                        "owner_user_id": "user-1",
                        "name": "Primary",
                        "provider": "openai",
                        "base_url": "https://api.openai.com/v1",
                        "api_key": "secret-key",
                        "model": "gpt-5.6-sol",
                        "thinking_level": "high",
                        "temperature": 0.2,
                        "max_output_tokens": 4096,
                        "supports_images": true,
                        "supports_reasoning": true,
                        "supports_responses": true
                    }))
                },
            ),
        );
        let (base_url, handle) = start_test_server(app).await;

        let record = get_internal_model_runtime_config(
            &reqwest::Client::new(),
            base_url.as_str(),
            secret,
            "user-1",
            "model-1",
        )
        .await
        .expect("internal model runtime response");

        assert_eq!(record.id, "model-1");
        assert_eq!(record.owner_user_id, "user-1");
        assert_eq!(record.model, "gpt-5.6-sol");
        handle.abort();
    }
}
