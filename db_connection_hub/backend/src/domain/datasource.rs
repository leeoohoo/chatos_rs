use crate::domain::meta::{AuthMode, DbType, NetworkMode, SslMode};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth_mode: String,
    pub password: Option<String>,
    pub private_key: Option<String>,
    pub passphrase: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub mode: NetworkMode,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub service_name: Option<String>,
    pub sid: Option<String>,
    pub file_path: Option<String>,
    pub ssh: Option<SshConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    pub enabled: bool,
    pub ssl_mode: Option<SslMode>,
    pub ca_cert: Option<String>,
    pub client_cert: Option<String>,
    pub client_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub mode: AuthMode,
    pub username: Option<String>,
    pub password: Option<String>,
    pub access_token: Option<String>,
    pub client_cert: Option<String>,
    pub client_key: Option<String>,
    pub key_ref: Option<String>,
    pub wallet_ref: Option<String>,
    pub principal: Option<String>,
    pub realm: Option<String>,
    pub kdc: Option<String>,
    pub service_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSourceOptions {
    pub connect_timeout_ms: Option<u64>,
    pub statement_timeout_ms: Option<u64>,
    pub pool_min: Option<u32>,
    pub pool_max: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DataSourceCreateRequest {
    pub name: String,
    pub db_type: DbType,
    pub network: NetworkConfig,
    pub auth: AuthConfig,
    pub tls: Option<TlsConfig>,
    pub options: Option<DataSourceOptions>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DataSourceUpdateRequest {
    pub name: Option<String>,
    pub network: Option<NetworkConfig>,
    pub auth: Option<AuthConfig>,
    pub tls: Option<TlsConfig>,
    pub options: Option<DataSourceOptions>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DataSourceMutationResponse {
    pub id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DataSourceListItem {
    pub id: String,
    pub name: String,
    pub db_type: DbType,
    pub status: ConnectionStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct DataSourceListResponse {
    pub items: Vec<DataSourceListItem>,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    Unknown,
    Online,
    Offline,
    Degraded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionTestStageResult {
    pub stage: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionTestResult {
    pub ok: bool,
    pub latency_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_version: Option<String>,
    pub auth_mode: AuthMode,
    pub checks: Vec<ConnectionTestStageResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DataSourceHealthResponse {
    pub status: ConnectionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_test_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_latency_ms: Option<u64>,
    pub failed_count_1h: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DatabaseSummaryResponse {
    pub database_count: u64,
    pub visible_database_count: u64,
    pub visibility_scope: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DatabaseListResponse {
    pub items: Vec<DatabaseInfo>,
    pub page: u32,
    pub page_size: u32,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSource {
    pub id: String,
    pub name: String,
    pub db_type: DbType,
    pub network: NetworkConfig,
    pub auth: AuthConfig,
    pub tls: Option<TlsConfig>,
    pub options: DataSourceOptions,
    pub tags: Vec<String>,
    pub status: ConnectionStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_test: Option<ConnectionTestResult>,
}

impl DataSourceOptions {
    pub fn with_defaults(options: Option<DataSourceOptions>) -> Self {
        let incoming = options.unwrap_or(Self {
            connect_timeout_ms: None,
            statement_timeout_ms: None,
            pool_min: None,
            pool_max: None,
        });

        Self {
            connect_timeout_ms: Some(incoming.connect_timeout_ms.unwrap_or(5_000)),
            statement_timeout_ms: Some(incoming.statement_timeout_ms.unwrap_or(15_000)),
            pool_min: Some(incoming.pool_min.unwrap_or(1)),
            pool_max: Some(incoming.pool_max.unwrap_or(20)),
        }
    }
}
