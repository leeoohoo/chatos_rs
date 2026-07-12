// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config::AppConfig;

#[derive(Debug, Deserialize, Serialize)]
pub struct SendRegistrationCodeRequest {
    pub email: String,
    pub invite_code: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RegisterRequest {
    pub email: String,
    pub display_name: Option<String>,
    pub password: String,
    pub invite_code: String,
    pub verification_code: String,
}

pub async fn send_registration_code(
    State(config): State<AppConfig>,
    Json(payload): Json<SendRegistrationCodeRequest>,
) -> (StatusCode, Json<Value>) {
    proxy_json(
        &config,
        "/api/auth/register/send-code",
        serde_json::to_value(payload).unwrap_or_else(|_| json!({})),
    )
    .await
}

pub async fn register(
    State(config): State<AppConfig>,
    Json(payload): Json<RegisterRequest>,
) -> (StatusCode, Json<Value>) {
    let email = payload.email;
    let upstream = json!({
        "username": email.clone(),
        "email": email,
        "display_name": payload.display_name,
        "password": payload.password,
        "invite_code": payload.invite_code,
        "verification_code": payload.verification_code,
    });
    let (status, Json(mut body)) = proxy_json(&config, "/api/auth/register", upstream).await;
    if status.is_success() {
        if let Some(object) = body.as_object_mut() {
            object.remove("token");
            object.remove("access_token");
            object.insert("ok".to_string(), Value::Bool(true));
        }
    }
    (status, Json(body))
}

async fn proxy_json(config: &AppConfig, path: &str, payload: Value) -> (StatusCode, Json<Value>) {
    let url = format!("{}{}", config.user_service_base_url, path);
    let response = match reqwest::Client::new().post(url).json(&payload).send().await {
        Ok(response) => response,
        Err(err) => {
            tracing::warn!(error = %err, path, "official website registration proxy failed");
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "registration service is temporarily unavailable"})),
            );
        }
    };

    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let body = response.text().await.unwrap_or_default();
    let json_body = serde_json::from_str::<Value>(&body).unwrap_or_else(|_| {
        json!({
            "error": if body.trim().is_empty() {
                "registration request failed"
            } else {
                body.trim()
            }
        })
    });
    (status, Json(json_body))
}
