mod client;
mod credentials;

use std::sync::Arc;

use client::{ApiError, AuthenticationApiClient};
pub(crate) use client::{GatewayAgentCapability, GatewayModelConfig};
pub(crate) use credentials::model_credential_environment_variable;
use credentials::{CredentialStore, PlatformCredentialStore};
use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinSet;

use crate::{
    host_lifecycle::HostManager, local_terminal::LocalTerminalRegistry,
    project_run::ProjectRunManager, remote_connection::RemoteConnectionManager,
};

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
    user_id: Option<String>,
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
                user_id: None,
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
                self.commit_token(
                    generation,
                    refreshed_token.as_deref().unwrap_or(&token),
                    &session.user.id,
                )
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
        self.commit_token(generation, &token, &session.user.id)
            .await?;
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
        self.commit_token(generation, &token, &session.user.id)
            .await?;
        Ok(session)
    }

    async fn commit_token(
        &self,
        generation: u64,
        token: &str,
        user_id: &str,
    ) -> Result<(), String> {
        let mut state = self.state.lock().await;
        if state.generation != generation {
            return Err("认证操作已被更新的请求替代。".into());
        }
        self.credentials.save_access_token(token)?;
        state.access_token = Some(token.to_owned());
        state.user_id = Some(user_id.to_owned());
        Ok(())
    }

    async fn clear_if_current(&self, generation: u64) -> Result<(), String> {
        let mut state = self.state.lock().await;
        if state.generation != generation {
            return Ok(());
        }
        self.credentials.delete_access_token()?;
        state.access_token = None;
        state.user_id = None;
        Ok(())
    }

    async fn logout(&self) -> Result<(), String> {
        let mut state = self.state.lock().await;
        state.generation = state.generation.wrapping_add(1);
        self.credentials.delete_access_token()?;
        state.access_token = None;
        state.user_id = None;
        Ok(())
    }

    pub(crate) async fn access_token_for_owner(
        &self,
        owner_user_id: &str,
    ) -> Result<String, String> {
        let state = self.state.lock().await;
        if state.user_id.as_deref() != Some(owner_user_id) {
            return Err("登录账号与本地服务账号不一致。".into());
        }
        state
            .access_token
            .clone()
            .ok_or_else(|| "登录状态尚未准备好。".into())
    }

    pub(crate) async fn model_catalog_for_owner(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<GatewayModelConfig>, ApiError> {
        let token = self
            .access_token_for_owner(owner_user_id)
            .await
            .map_err(|message| ApiError {
                message,
                unauthorized: true,
            })?;
        self.client.model_configs(&token).await
    }

    pub(crate) async fn resolved_models_for_owner(
        &self,
        owner_user_id: &str,
        model_config_refs: Vec<String>,
    ) -> Result<Vec<GatewayModelConfig>, ApiError> {
        let token = self
            .access_token_for_owner(owner_user_id)
            .await
            .map_err(|message| ApiError {
                message,
                unauthorized: true,
            })?;
        let concurrency = Arc::new(Semaphore::new(4));
        let mut tasks = JoinSet::new();
        for (index, model_config_ref) in model_config_refs.into_iter().enumerate() {
            let client = self.client.clone();
            let token = token.clone();
            let concurrency = Arc::clone(&concurrency);
            tasks.spawn(async move {
                let _permit = concurrency.acquire_owned().await.map_err(|_| ApiError {
                    message: "模型配置刷新已取消。".into(),
                    unauthorized: false,
                })?;
                client
                    .model_config(&token, &model_config_ref)
                    .await
                    .map(|model| (index, model))
            });
        }
        let mut resolved = Vec::new();
        while let Some(result) = tasks.join_next().await {
            match result {
                Ok(Ok(model)) => resolved.push(model),
                Ok(Err(error)) => {
                    tasks.abort_all();
                    return Err(error);
                }
                Err(_) => {
                    tasks.abort_all();
                    return Err(ApiError {
                        message: "模型配置刷新已取消。".into(),
                        unauthorized: false,
                    });
                }
            }
        }
        resolved.sort_by_key(|(index, _)| *index);
        Ok(resolved.into_iter().map(|(_, model)| model).collect())
    }

    pub(crate) async fn local_agent_capability_for_owner(
        &self,
        owner_user_id: &str,
    ) -> Result<GatewayAgentCapability, ApiError> {
        let token = self
            .access_token_for_owner(owner_user_id)
            .await
            .map_err(|message| ApiError {
                message,
                unauthorized: true,
            })?;
        self.client.local_agent_capability(&token).await
    }

    pub(crate) fn load_model_credential(
        &self,
        owner_user_id: &str,
        model_config_ref: &str,
    ) -> Result<Option<String>, String> {
        self.credentials
            .load_model_credential(owner_user_id, model_config_ref)
    }

    pub(crate) fn save_model_credential(
        &self,
        owner_user_id: &str,
        model_config_ref: &str,
        credential: &str,
    ) -> Result<(), String> {
        self.credentials
            .save_model_credential(owner_user_id, model_config_ref, credential)
    }
}

fn api_message(error: ApiError) -> String {
    error.message
}

#[tauri::command]
pub async fn restore_authentication_session(
    manager: State<'_, AuthenticationManager>,
    project_runs: State<'_, ProjectRunManager>,
    remote_connections: State<'_, RemoteConnectionManager>,
    local_terminals: State<'_, LocalTerminalRegistry>,
) -> Result<Option<AuthSession>, String> {
    local_terminals.reset().await;
    remote_connections.reset().await;
    let session = manager.restore_session().await?;
    if let Some(session) = &session {
        project_runs.retain_owner(&session.user.id).await;
    } else {
        project_runs.reset().await;
    }
    Ok(session)
}

#[tauri::command]
pub async fn login(
    username: String,
    password: String,
    manager: State<'_, AuthenticationManager>,
    project_runs: State<'_, ProjectRunManager>,
    remote_connections: State<'_, RemoteConnectionManager>,
    local_terminals: State<'_, LocalTerminalRegistry>,
) -> Result<AuthSession, String> {
    local_terminals.reset().await;
    remote_connections.reset().await;
    let session = manager.login(username, password).await?;
    project_runs.retain_owner(&session.user.id).await;
    Ok(session)
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
    project_runs: State<'_, ProjectRunManager>,
    remote_connections: State<'_, RemoteConnectionManager>,
    local_terminals: State<'_, LocalTerminalRegistry>,
) -> Result<AuthSession, String> {
    local_terminals.reset().await;
    remote_connections.reset().await;
    let session = manager
        .register(email, password, invite_code, verification_code)
        .await?;
    project_runs.retain_owner(&session.user.id).await;
    Ok(session)
}

#[tauri::command]
pub async fn logout(
    manager: State<'_, AuthenticationManager>,
    host_manager: State<'_, HostManager>,
    project_runs: State<'_, ProjectRunManager>,
    remote_connections: State<'_, RemoteConnectionManager>,
    local_terminals: State<'_, LocalTerminalRegistry>,
) -> Result<(), String> {
    local_terminals.reset().await;
    remote_connections.reset().await;
    project_runs.reset().await;
    let result = manager.logout().await;
    host_manager.reset_approval_sessions();
    result
}
