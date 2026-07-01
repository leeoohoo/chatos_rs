// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use uuid::Uuid;

use crate::config::Config;
use crate::models::agent::Agent;
use crate::services::{access_token_scope, user_service_api_client};

pub(super) async fn provision_task_runner_agent_account(agent: &Agent) -> Result<String, String> {
    let config = Config::get();
    let user_service_base_url = config
        .user_service_base_url
        .as_deref()
        .ok_or_else(|| "CHATOS_USER_SERVICE_BASE_URL is not configured".to_string())?;
    let access_token = access_token_scope::get_current_access_token()
        .ok_or_else(|| "current user access token is missing".to_string())?;
    let username = build_task_runner_agent_username(agent.id.as_str());
    if let Some(existing_id) = find_existing_task_runner_agent_account_id(
        user_service_base_url,
        access_token.as_str(),
        username.as_str(),
        config.user_service_request_timeout_ms,
    )
    .await?
    {
        return Ok(existing_id);
    }

    let created = user_service_api_client::create_agent_account(
        user_service_base_url,
        access_token.as_str(),
        &user_service_api_client::CreateUserServiceAgentAccountRequest {
            username: username.clone(),
            display_name: Some(agent.name.clone()),
            password: build_task_runner_agent_password(),
            owner_user_id: Some(agent.user_id.clone()),
            enabled: Some(agent.enabled),
        },
        config.user_service_request_timeout_ms,
    )
    .await;

    match created {
        Ok(created) => Ok(created.id),
        Err(err) if is_existing_agent_account_error(err.as_str()) => {
            find_existing_task_runner_agent_account_id(
                user_service_base_url,
                access_token.as_str(),
                username.as_str(),
                config.user_service_request_timeout_ms,
            )
            .await?
            .ok_or(err)
        }
        Err(err) => Err(err),
    }
}

fn build_task_runner_agent_username(agent_id: &str) -> String {
    let normalized = agent_id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    format!("chatos-agent-{normalized}")
}

fn build_task_runner_agent_password() -> String {
    format!("tr-{}", Uuid::new_v4().simple())
}

async fn find_existing_task_runner_agent_account_id(
    user_service_base_url: &str,
    access_token: &str,
    username: &str,
    timeout_ms: i64,
) -> Result<Option<String>, String> {
    let items = user_service_api_client::list_agent_accounts(
        user_service_base_url,
        access_token,
        timeout_ms,
    )
    .await?;
    Ok(items
        .into_iter()
        .find(|item| usernames_match(item.username.as_str(), username))
        .map(|item| item.id))
}

fn usernames_match(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

fn is_existing_agent_account_error(err: &str) -> bool {
    let normalized = err.trim().to_ascii_lowercase();
    normalized.contains("agent username already exists")
        || (normalized.contains("already exists") && normalized.contains(" 400 "))
        || (normalized.contains("already exists") && normalized.contains(": 400 "))
}
