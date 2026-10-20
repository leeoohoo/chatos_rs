mod client;
mod credentials;

use std::sync::Arc;

use client::{ApiError, AuthenticationApiClient};
use credentials::{CredentialStore, PlatformCredentialStore};
use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::Mutex;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthUser {
    pub id: String,
    pub username: String,
    #[serde(alias = "display_name")]
    pub display_name: Option<String>,
    pub role: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AuthSession {
    pub user: AuthUser,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistrationCodeDelivery {
    pub expires_in_seconds: i64,
    pub resend_after_seconds: i64,
}

struct AuthenticationState {
    generation: u64,
    access_token: Option<String>,
}

pub struct AuthenticationManager {
    client: AuthenticationApiClient,
    credentials: Arc<dyn CredentialStore>,
    state: Mutex<AuthenticationState>,
}

impl AuthenticationManager {
    pub fn from_runtime() -> Result<Self, String> {
        Ok(Self {
            client: AuthenticationApiClient::from_runtime()?,
            credentials: Arc::new(PlatformCredentialStore),
            state: Mutex::new(AuthenticationState {
                generation: 0,
                access_token: None,
            }),
        })
    }

    async fn begin_authentication(&self) -> u64 {
        let mut state = self.state.lock().await;
        state.generation = state.generation.wrapping_add(1);
        state.generation
    }

    async fn restore_session(&self) -> Result<Option<AuthSession>, String> {
        let generation = self.begin_authentication().await;
        let Some(token) = self.credentials.load_access_token()? else {
            return Ok(None);
        };
        match self.client.restore(&token).await {
            Ok((session, refreshed_token)) => {
                self.commit_token(generation, refreshed_token.as_deref().unwrap_or(&token))
                    .await?;
                Ok(Some(session))
            }
            Err(error) if error.unauthorized => {
                self.clear_if_current(generation).await?;
                Ok(None)
            }
            Err(error) => Err(error.message),
        }
    }

    async fn login(&self, username: String, password: String) -> Result<AuthSession, String> {
        let username = username.trim().to_owned();
        if username.is_empty() || password.is_empty() {
            return Err("请输入账号和密码。".into());
        }
        let generation = self.begin_authentication().await;
        let (session, token) = self
            .client
            .login(&username, &password)
            .await
            .map_err(api_message)?;
        self.commit_token(generation, &token).await?;
        Ok(session)
    }

    async fn register(
        &self,
        email: String,
        password: String,
        invite_code: String,
        verification_code: String,
    ) -> Result<AuthSession, String> {
        let generation = self.begin_authentication().await;
        let (session, token) = self
            .client
            .register(
                email.trim(),
                &password,
                invite_code.trim(),
                verification_code.trim(),
            )
            .await
            .map_err(api_message)?;
        self.commit_token(generation, &token).await?;
        Ok(session)
    }

    async fn commit_token(&self, generation: u64, token: &str) -> Result<(), String> {
        let mut state = self.state.lock().await;
        if state.generation != generation {
            return Err("认证操作已被更新的请求替代。".into());
        }
        self.credentials.save_access_token(token)?;
        state.access_token = Some(token.to_owned());
        Ok(())
    }

    async fn clear_if_current(&self, generation: u64) -> Result<(), String> {
        let mut state = self.state.lock().await;
        if state.generation != generation {
            return Ok(());
        }
        self.credentials.delete_access_token()?;
        state.access_token = None;
        Ok(())
    }

    async fn logout(&self) -> Result<(), String> {
        let mut state = self.state.lock().await;
        state.generation = state.generation.wrapping_add(1);
        self.credentials.delete_access_token()?;
        state.access_token = None;
        Ok(())
    }
}

fn api_message(error: ApiError) -> String {
    error.message
}

#[tauri::command]
pub async fn restore_authentication_session(
    manager: State<'_, AuthenticationManager>,
) -> Result<Option<AuthSession>, String> {
    manager.restore_session().await
}

#[tauri::command]
pub async fn login(
    username: String,
    password: String,
    manager: State<'_, AuthenticationManager>,
) -> Result<AuthSession, String> {
    manager.login(username, password).await
}

#[tauri::command]
pub async fn send_registration_code(
    email: String,
    invite_code: String,
    manager: State<'_, AuthenticationManager>,
) -> Result<RegistrationCodeDelivery, String> {
    manager
        .client
        .send_registration_code(email.trim(), invite_code.trim())
        .await
        .map_err(api_message)
}

#[tauri::command]
pub async fn register(
    email: String,
    password: String,
    invite_code: String,
    verification_code: String,
    manager: State<'_, AuthenticationManager>,
) -> Result<AuthSession, String> {
    manager
        .register(email, password, invite_code, verification_code)
        .await
}

#[tauri::command]
pub async fn logout(manager: State<'_, AuthenticationManager>) -> Result<(), String> {
    manager.logout().await
}
