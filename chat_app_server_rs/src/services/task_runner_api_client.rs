use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct UserServiceTaskRunnerExchange {
    pub base_url: String,
    pub access_token: String,
    pub task_runner_agent_account_id: String,
    pub contact_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UserServiceTaskRunnerTokenResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct TaskRunnerSkillResponse {
    content: String,
}

pub async fn exchange_task_runner_token_via_user_service(
    request: &UserServiceTaskRunnerExchange,
) -> Result<String, String> {
    let endpoint = format!(
        "{}/api/token/exchange/task-runner",
        request.base_url.trim().trim_end_matches('/')
    );
    let response = reqwest::Client::new()
        .post(endpoint)
        .bearer_auth(request.access_token.trim())
        .json(&serde_json::json!({
            "task_runner_agent_account_id": request.task_runner_agent_account_id,
            "contact_id": request.contact_id,
        }))
        .send()
        .await
        .map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "User service task runner token exchange failed: {status} {body}"
        ));
    }
    let payload = response
        .json::<UserServiceTaskRunnerTokenResponse>()
        .await
        .map_err(|err| err.to_string())?;
    let token = payload.access_token.trim();
    if token.is_empty() {
        return Err("User service task runner token exchange returned empty token".to_string());
    }
    Ok(token.to_string())
}

pub async fn fetch_task_runner_skill(base_url: &str, lang: &str) -> Result<String, String> {
    let normalized_lang = match lang.trim() {
        "en" | "en-US" | "english" => "en-US",
        _ => "zh-CN",
    };
    let endpoint = format!(
        "{}/api/skills/task-runner?lang={}",
        base_url.trim().trim_end_matches('/'),
        normalized_lang
    );
    let response = reqwest::Client::new()
        .get(endpoint)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Task Runner skill request failed: {status} {body}"));
    }
    let payload = response
        .json::<TaskRunnerSkillResponse>()
        .await
        .map_err(|err| err.to_string())?;
    let content = payload.content.trim();
    if content.is_empty() {
        return Err("Task Runner skill request returned empty content".to_string());
    }
    Ok(content.to_string())
}

async fn get_internal_json(
    base_url: &str,
    path: &str,
    query: &[(&str, &str)],
) -> Result<Value, String> {
    let endpoint = format!("{}{}", base_url.trim().trim_end_matches('/'), path);
    let response = reqwest::Client::new()
        .get(endpoint)
        .query(query)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "Task Runner internal request failed: {status} {body}"
        ));
    }
    response
        .json::<Value>()
        .await
        .map_err(|err| err.to_string())
}

async fn post_internal_json<T: Serialize + ?Sized>(
    base_url: &str,
    path: &str,
    body: &T,
) -> Result<Value, String> {
    let endpoint = format!("{}{}", base_url.trim().trim_end_matches('/'), path);
    let response = reqwest::Client::new()
        .post(endpoint)
        .json(body)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "Task Runner internal request failed: {status} {body}"
        ));
    }
    response
        .json::<Value>()
        .await
        .map_err(|err| err.to_string())
}

#[derive(Debug, Serialize)]
struct SessionActiveMessageTasksRequest<'a> {
    source_session_id: &'a str,
    source_user_message_ids: &'a [String],
    source_turn_ids: &'a [String],
}

pub async fn list_session_active_message_tasks(
    base_url: &str,
    source_session_id: &str,
    source_user_message_ids: &[String],
    source_turn_ids: &[String],
) -> Result<Value, String> {
    post_internal_json(
        base_url,
        "/internal/chatos/session-active-message-tasks",
        &SessionActiveMessageTasksRequest {
            source_session_id,
            source_user_message_ids,
            source_turn_ids,
        },
    )
    .await
}

pub async fn list_message_tasks(
    base_url: &str,
    source_session_id: &str,
    source_user_message_id: Option<&str>,
    source_turn_id: Option<&str>,
) -> Result<Value, String> {
    let mut query = vec![("source_session_id", source_session_id)];
    if let Some(source_user_message_id) = source_user_message_id {
        query.push(("source_user_message_id", source_user_message_id));
    }
    if let Some(source_turn_id) = source_turn_id {
        query.push(("source_turn_id", source_turn_id));
    }
    get_internal_json(base_url, "/internal/chatos/message-tasks", query.as_slice()).await
}

pub async fn get_message_task_graph(
    base_url: &str,
    source_session_id: &str,
    source_user_message_id: Option<&str>,
    source_turn_id: Option<&str>,
) -> Result<Value, String> {
    let mut query = vec![("source_session_id", source_session_id)];
    if let Some(source_user_message_id) = source_user_message_id {
        query.push(("source_user_message_id", source_user_message_id));
    }
    if let Some(source_turn_id) = source_turn_id {
        query.push(("source_turn_id", source_turn_id));
    }
    get_internal_json(base_url, "/internal/chatos/message-graph", query.as_slice()).await
}

pub async fn get_message_task(
    base_url: &str,
    task_id: &str,
    source_session_id: &str,
    source_user_message_id: Option<&str>,
    source_turn_id: Option<&str>,
) -> Result<Value, String> {
    let path = format!(
        "/internal/chatos/message-tasks/{}",
        urlencoding::encode(task_id.trim())
    );
    let mut query = vec![("source_session_id", source_session_id)];
    if let Some(source_user_message_id) = source_user_message_id {
        query.push(("source_user_message_id", source_user_message_id));
    }
    if let Some(source_turn_id) = source_turn_id {
        query.push(("source_turn_id", source_turn_id));
    }
    get_internal_json(base_url, path.as_str(), query.as_slice()).await
}

pub async fn get_message_run(
    base_url: &str,
    run_id: &str,
    source_session_id: &str,
    source_user_message_id: Option<&str>,
    source_turn_id: Option<&str>,
) -> Result<Value, String> {
    let path = format!(
        "/internal/chatos/message-runs/{}",
        urlencoding::encode(run_id.trim())
    );
    let mut query = vec![("source_session_id", source_session_id)];
    if let Some(source_user_message_id) = source_user_message_id {
        query.push(("source_user_message_id", source_user_message_id));
    }
    if let Some(source_turn_id) = source_turn_id {
        query.push(("source_turn_id", source_turn_id));
    }
    get_internal_json(base_url, path.as_str(), query.as_slice()).await
}

pub async fn get_message_graph_run(
    base_url: &str,
    run_id: &str,
    source_session_id: &str,
    source_user_message_id: Option<&str>,
    source_turn_id: Option<&str>,
) -> Result<Value, String> {
    let path = format!(
        "/internal/chatos/message-graph/runs/{}",
        urlencoding::encode(run_id.trim())
    );
    let mut query = vec![("source_session_id", source_session_id)];
    if let Some(source_user_message_id) = source_user_message_id {
        query.push(("source_user_message_id", source_user_message_id));
    }
    if let Some(source_turn_id) = source_turn_id {
        query.push(("source_turn_id", source_turn_id));
    }
    get_internal_json(base_url, path.as_str(), query.as_slice()).await
}

#[cfg(test)]
mod tests {
    use super::{exchange_task_runner_token_via_user_service, UserServiceTaskRunnerExchange};
    use axum::extract::State;
    use axum::http::{header::AUTHORIZATION, HeaderMap, StatusCode};
    use axum::{routing::post, Json, Router};
    use serde_json::{json, Value};
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[derive(Debug, Default)]
    struct CapturedExchange {
        authorization: Option<String>,
        body: Option<Value>,
    }

    #[derive(Clone)]
    struct ExchangeServerState {
        captured: Arc<Mutex<CapturedExchange>>,
        response_status: StatusCode,
        response_body: Value,
    }

    async fn start_test_server(
        captured: Arc<Mutex<CapturedExchange>>,
        status: StatusCode,
        body: Value,
    ) -> (String, tokio::task::JoinHandle<()>) {
        async fn handler(
            State(state): State<ExchangeServerState>,
            headers: HeaderMap,
            Json(payload): Json<Value>,
        ) -> (StatusCode, Json<Value>) {
            let mut captured = state.captured.lock().await;
            captured.authorization = headers
                .get(AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .map(ToOwned::to_owned);
            captured.body = Some(payload);
            (state.response_status, Json(state.response_body))
        }

        let app = Router::new()
            .route("/api/token/exchange/task-runner", post(handler))
            .with_state(ExchangeServerState {
                captured,
                response_status: status,
                response_body: body,
            });
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
    async fn exchange_task_runner_token_via_user_service_sends_bearer_and_body() {
        let captured = Arc::new(Mutex::new(CapturedExchange::default()));
        let (base_url, handle) =
            start_test_server(captured.clone(), StatusCode::OK, json!({})).await;

        let token = exchange_task_runner_token_via_user_service(&UserServiceTaskRunnerExchange {
            base_url,
            access_token: "human-user-token".to_string(),
            task_runner_agent_account_id: "agent-123".to_string(),
            contact_id: Some("contact-456".to_string()),
        })
        .await
        .expect("exchange response");

        assert_eq!(token, "task-runner-token");
        let captured = captured.lock().await;
        assert_eq!(
            captured.authorization.as_deref(),
            Some("Bearer human-user-token")
        );
        assert_eq!(
            captured
                .body
                .as_ref()
                .and_then(|value| value.get("task_runner_agent_account_id"))
                .and_then(Value::as_str),
            Some("agent-123")
        );
        assert_eq!(
            captured
                .body
                .as_ref()
                .and_then(|value| value.get("contact_id"))
                .and_then(Value::as_str),
            Some("contact-456")
        );

        handle.abort();
    }

    #[tokio::test]
    async fn exchange_task_runner_token_via_user_service_surfaces_remote_error() {
        let captured = Arc::new(Mutex::new(CapturedExchange::default()));
        let (base_url, handle) = start_test_server(
            captured,
            StatusCode::FORBIDDEN,
            json!({ "error": "owner mismatch" }),
        )
        .await;

        let error = exchange_task_runner_token_via_user_service(&UserServiceTaskRunnerExchange {
            base_url,
            access_token: "human-user-token".to_string(),
            task_runner_agent_account_id: "agent-123".to_string(),
            contact_id: None,
        })
        .await
        .expect_err("expected remote error");

        assert!(error.contains("403"));
        assert!(error.contains("owner mismatch"));

        handle.abort();
    }
}
