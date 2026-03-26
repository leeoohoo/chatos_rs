use super::dto::{MemoryAuthLoginResponse, MemoryAuthMeResponse};
use super::http::{build_url, client, send_json_without_service_token, timeout_duration};

pub async fn auth_login(username: &str, password: &str) -> Result<MemoryAuthLoginResponse, String> {
    let req = client()
        .post(build_url("/auth/login").as_str())
        .timeout(timeout_duration())
        .json(&serde_json::json!({
            "username": username,
            "password": password
        }));
    send_json_without_service_token(req).await
}

pub async fn auth_me(access_token: &str) -> Result<MemoryAuthMeResponse, String> {
    let trimmed = access_token.trim();
    if trimmed.is_empty() {
        return Err("access_token is required".to_string());
    }

    let req = client()
        .get(build_url("/auth/me").as_str())
        .timeout(timeout_duration())
        .bearer_auth(trimmed);
    send_json_without_service_token(req).await
}
