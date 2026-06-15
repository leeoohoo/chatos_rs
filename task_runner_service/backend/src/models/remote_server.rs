use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteServerRecord {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: i64,
    pub username: String,
    pub auth_type: String,
    pub password: Option<String>,
    pub private_key_path: Option<String>,
    pub certificate_path: Option<String>,
    pub default_remote_path: Option<String>,
    pub host_key_policy: String,
    pub enabled: bool,
    pub last_tested_at: Option<String>,
    pub last_test_status: Option<String>,
    pub last_test_message: Option<String>,
    pub last_active_at: Option<String>,
    #[serde(default)]
    pub creator_user_id: Option<String>,
    #[serde(default)]
    pub creator_username: Option<String>,
    #[serde(default)]
    pub creator_display_name: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRemoteServerRequest {
    pub name: String,
    pub host: String,
    pub port: Option<i64>,
    pub username: String,
    pub auth_type: String,
    pub password: Option<String>,
    pub private_key_path: Option<String>,
    pub certificate_path: Option<String>,
    pub default_remote_path: Option<String>,
    pub host_key_policy: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateRemoteServerRequest {
    pub name: Option<String>,
    pub host: Option<String>,
    pub port: Option<i64>,
    pub username: Option<String>,
    pub auth_type: Option<String>,
    pub password: Option<String>,
    pub private_key_path: Option<String>,
    pub certificate_path: Option<String>,
    pub default_remote_path: Option<String>,
    pub host_key_policy: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestRemoteServerRequest {
    pub name: Option<String>,
    pub host: Option<String>,
    pub port: Option<i64>,
    pub username: Option<String>,
    pub auth_type: Option<String>,
    pub password: Option<String>,
    pub private_key_path: Option<String>,
    pub certificate_path: Option<String>,
    pub default_remote_path: Option<String>,
    pub host_key_policy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteServerTestResponse {
    pub ok: bool,
    pub server_id: Option<String>,
    pub name: String,
    pub host: String,
    pub port: i64,
    pub username: String,
    pub auth_type: String,
    pub remote_host: Option<String>,
    pub error: Option<String>,
    pub tested_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteServerSummaryRecord {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: i64,
    pub username: String,
    pub auth_type: String,
    pub enabled: bool,
    pub updated_at: String,
    pub last_tested_at: Option<String>,
    pub last_test_status: Option<String>,
}

impl From<&RemoteServerRecord> for RemoteServerSummaryRecord {
    fn from(value: &RemoteServerRecord) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            host: value.host.clone(),
            port: value.port,
            username: value.username.clone(),
            auth_type: value.auth_type.clone(),
            enabled: value.enabled,
            updated_at: value.updated_at.clone(),
            last_tested_at: value.last_tested_at.clone(),
            last_test_status: value.last_test_status.clone(),
        }
    }
}
