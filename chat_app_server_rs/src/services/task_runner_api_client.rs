use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct TaskRunnerAgentCredentials {
    pub base_url: String,
    pub username: String,
    pub password: String,
    pub contact_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct AgentTokenRequest<'a> {
    username: &'a str,
    password: &'a str,
    client: &'a str,
    contact_id: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct AgentTokenResponse {
    token: String,
}

pub async fn exchange_agent_token(
    credentials: &TaskRunnerAgentCredentials,
) -> Result<String, String> {
    let endpoint = format!(
        "{}/api/auth/agent-token",
        credentials.base_url.trim().trim_end_matches('/')
    );
    let response = reqwest::Client::new()
        .post(endpoint)
        .json(&AgentTokenRequest {
            username: credentials.username.as_str(),
            password: credentials.password.as_str(),
            client: "chatos-contact-mcp",
            contact_id: credentials.contact_id.as_deref(),
        })
        .send()
        .await
        .map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "Task Runner token exchange failed: {status} {body}"
        ));
    }
    let payload = response
        .json::<AgentTokenResponse>()
        .await
        .map_err(|err| err.to_string())?;
    if payload.token.trim().is_empty() {
        return Err("Task Runner token exchange returned empty token".to_string());
    }
    Ok(payload.token)
}
