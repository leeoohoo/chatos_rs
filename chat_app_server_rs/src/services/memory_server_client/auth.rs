use super::dto::{MemoryAuthLoginResponse, MemoryAuthMeResponse};
use super::http::{client, send_json_without_service_token, try_build_url, try_timeout_duration};

pub async fn auth_login(username: &str, password: &str) -> Result<MemoryAuthLoginResponse, String> {
    let req = client()
        .post(try_build_url("/auth/login")?)
        .timeout(try_timeout_duration()?)
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
        .get(try_build_url("/auth/me")?)
        .timeout(try_timeout_duration()?)
        .bearer_auth(trimmed);
    send_json_without_service_token(req).await
}
