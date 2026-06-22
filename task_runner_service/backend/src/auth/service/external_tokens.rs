use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::Deserialize;

use super::*;

#[derive(Debug, Deserialize)]
struct UserServiceTaskRunnerClaims {
    iss: String,
    aud: String,
    exp: usize,
    principal_type: String,
    username: Option<String>,
    display_name: Option<String>,
    agent_account_id: Option<String>,
    owner_user_id: Option<String>,
}

impl AuthService {
    pub(super) fn current_user_from_user_service_token(&self, token: &str) -> Option<CurrentUser> {
        current_user_from_user_service_token_with_config(&self.config, token)
    }
}

fn current_user_from_user_service_token_with_config(
    config: &AppConfig,
    token: &str,
) -> Option<CurrentUser> {
    let secret = config.user_service_jwt_secret.as_deref()?;
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&[config.user_service_jwt_issuer.as_str()]);
    validation.set_audience(&[config.user_service_task_runner_audience.as_str()]);
    let claims = decode::<UserServiceTaskRunnerClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .ok()?
    .claims;
    if claims.iss.trim().is_empty()
        || claims.aud.trim().is_empty()
        || claims.exp == 0
        || claims.principal_type != "agent_account"
    {
        return None;
    }
    let id = claims.agent_account_id?.trim().to_string();
    let username = claims.username?.trim().to_string();
    if id.is_empty() || username.is_empty() {
        return None;
    }
    let display_name = claims
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(username.as_str())
        .to_string();
    let owner_user_id = claims
        .owner_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    Some(CurrentUser {
        id,
        username,
        display_name,
        role: UserRole::Agent,
        owner_user_id,
    })
}

#[cfg(test)]
mod tests {
    use super::current_user_from_user_service_token_with_config;
    use crate::config::{AppConfig, StoreMode, DEFAULT_TASK_RUN_EXECUTION_TIMEOUT_MS};
    use chrono::Utc;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use serde::Serialize;
    use std::time::Duration;

    #[derive(Debug, Serialize)]
    struct TestClaims<'a> {
        iss: &'a str,
        aud: &'a str,
        exp: usize,
        principal_type: &'a str,
        username: &'a str,
        display_name: &'a str,
        agent_account_id: Option<&'a str>,
        owner_user_id: Option<&'a str>,
    }

    #[test]
    fn accepts_valid_user_service_agent_token() {
        let config = test_config();
        let token = encode_test_token(
            &config,
            TestClaims {
                iss: "user_service",
                aud: "task_runner",
                exp: (Utc::now().timestamp() + 3600) as usize,
                principal_type: "agent_account",
                username: "agent-alpha",
                display_name: "Agent Alpha",
                agent_account_id: Some("agent-1"),
                owner_user_id: Some("user-1"),
            },
        );

        let current_user = current_user_from_user_service_token_with_config(&config, &token)
            .expect("expected token to be accepted");

        assert_eq!(current_user.id, "agent-1");
        assert_eq!(current_user.username, "agent-alpha");
        assert_eq!(current_user.display_name, "Agent Alpha");
        assert_eq!(current_user.owner_user_id.as_deref(), Some("user-1"));
        assert!(current_user.is_agent());
    }

    #[test]
    fn rejects_human_user_token_from_user_service() {
        let config = test_config();
        let token = encode_test_token(
            &config,
            TestClaims {
                iss: "user_service",
                aud: "task_runner",
                exp: (Utc::now().timestamp() + 3600) as usize,
                principal_type: "human_user",
                username: "user-alpha",
                display_name: "User Alpha",
                agent_account_id: None,
                owner_user_id: None,
            },
        );

        assert!(current_user_from_user_service_token_with_config(&config, &token).is_none());
    }

    fn encode_test_token(config: &AppConfig, claims: TestClaims<'_>) -> String {
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(
                config
                    .user_service_jwt_secret
                    .as_deref()
                    .expect("missing test user_service secret")
                    .as_bytes(),
            ),
        )
        .expect("encode test token")
    }

    fn test_config() -> AppConfig {
        AppConfig {
            host: "127.0.0.1".parse().expect("parse host"),
            port: 3000,
            store_mode: StoreMode::Memory,
            database_url: "sqlite::memory:".to_string(),
            memory_engine_base_url: None,
            memory_engine_source_id: "memory-source".to_string(),
            memory_engine_operator_token: None,
            default_tenant_id: "tenant".to_string(),
            default_subject_id: "subject".to_string(),
            default_workspace_dir: ".".to_string(),
            memory_timeout: Duration::from_secs(5),
            execution_timeout: Duration::from_millis(DEFAULT_TASK_RUN_EXECUTION_TIMEOUT_MS),
            scheduler_poll_interval: Duration::from_secs(5),
            auto_memory_summary: false,
            default_task_execution_max_iterations: 16,
            default_tool_result_model_max_chars: 50_000,
            default_tool_results_model_total_max_chars: 200_000,
            chatos_callback_url: None,
            chatos_callback_secret: None,
            callback_timeout: Duration::from_secs(5),
            admin_username: "admin".to_string(),
            admin_password: "admin123456".to_string(),
            admin_display_name: "Admin".to_string(),
            user_service_jwt_secret: Some("change_me_user_service_secret".to_string()),
            user_service_jwt_issuer: "user_service".to_string(),
            user_service_task_runner_audience: "task_runner".to_string(),
        }
    }
}
