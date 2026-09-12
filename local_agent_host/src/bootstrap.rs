// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fmt;
use std::path::Path;

use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chatos_client_storage::{
    BootstrapStorageProfile, PostgresConnectionSettings, PostgresCredentials, PostgresEndpoint,
    PostgresTlsMode, SecretReference, StorageEncryptionKey, StorageError, StorageResult,
    StorageSecretResolver,
};
use serde::{Deserialize, Deserializer, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use url::Url;
use zeroize::{Zeroize, Zeroizing};

pub const LOCAL_AGENT_HOST_LAUNCH_PROTOCOL_VERSION: u32 = 1;
pub const MAXIMUM_LOCAL_AGENT_LAUNCH_FRAME_BYTES: usize = 1024 * 1024;

const WINDOWS_PIPE_PREFIX: &str = r"\\.\pipe\chatos-local-agent-";
const MINIMUM_OPAQUE_ENDPOINT_ID_BYTES: usize = 8;
const MAXIMUM_OPAQUE_ENDPOINT_ID_BYTES: usize = 128;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LocalAgentHostBootstrapError {
    #[error("local Agent Host launch frame is empty or exceeds its size boundary")]
    InvalidFrameSize,
    #[error("local Agent Host launch frame ended before it was complete")]
    TruncatedFrame,
    #[error("local Agent Host launch frame is not valid protocol JSON")]
    InvalidJson,
    #[error("unsupported local Agent Host launch protocol version")]
    UnsupportedProtocol,
    #[error("local Agent Host launch identity is invalid: {0}")]
    InvalidIdentity(&'static str),
    #[error("local Agent Host service endpoint is invalid: {0}")]
    InvalidServiceEndpoint(&'static str),
    #[error("local Agent Host IPC endpoint is invalid")]
    InvalidIpcEndpoint,
    #[error("local Agent Host credential bundle does not match the active storage profile")]
    StorageCredentialMismatch,
    #[error("local Agent Host credential is empty or malformed: {0}")]
    InvalidCredential(&'static str),
    #[error("local Agent Host ready frame could not be encoded")]
    ReadyEncoding,
    #[error("local Agent Host bootstrap I/O failed")]
    Io,
}

/// A secret string whose memory is zeroized on drop and whose Debug output is
/// always redacted. Launch protocol types intentionally implement only
/// Deserialize: the Host must never serialize the credential bundle again.
pub struct LocalAgentLaunchSecret(Zeroizing<String>);

impl LocalAgentLaunchSecret {
    pub fn expose(&self) -> &str {
        self.0.as_str()
    }

    fn validate(&self, field: &'static str) -> Result<(), LocalAgentHostBootstrapError> {
        let value = self.expose();
        if value.trim().is_empty() || value.trim() != value {
            Err(LocalAgentHostBootstrapError::InvalidCredential(field))
        } else {
            Ok(())
        }
    }
}

impl<'de> Deserialize<'de> for LocalAgentLaunchSecret {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(|value| Self(Zeroizing::new(value)))
    }
}

impl fmt::Debug for LocalAgentLaunchSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalAgentLaunchPostgresTlsMode {
    Disabled,
    VerifyFull,
}

impl From<LocalAgentLaunchPostgresTlsMode> for PostgresTlsMode {
    fn from(value: LocalAgentLaunchPostgresTlsMode) -> Self {
        match value {
            LocalAgentLaunchPostgresTlsMode::Disabled => Self::Disabled,
            LocalAgentLaunchPostgresTlsMode::VerifyFull => Self::VerifyFull,
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "backend", rename_all = "snake_case", deny_unknown_fields)]
pub enum LocalAgentLaunchStorageCredentials {
    Sqlite {
        encryption_secret_reference: String,
        encryption_key_base64: LocalAgentLaunchSecret,
    },
    Postgres {
        connection_secret_reference: String,
        host: String,
        port: u16,
        database: String,
        tls_mode: LocalAgentLaunchPostgresTlsMode,
        username: String,
        password: LocalAgentLaunchSecret,
    },
}

impl fmt::Debug for LocalAgentLaunchStorageCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite {
                encryption_secret_reference,
                ..
            } => formatter
                .debug_struct("Sqlite")
                .field("encryption_secret_reference", encryption_secret_reference)
                .field("encryption_key_base64", &"[REDACTED]")
                .finish(),
            Self::Postgres {
                connection_secret_reference,
                port,
                tls_mode,
                ..
            } => formatter
                .debug_struct("Postgres")
                .field("connection_secret_reference", connection_secret_reference)
                .field("host", &"[REDACTED]")
                .field("port", port)
                .field("database", &"[REDACTED]")
                .field("tls_mode", tls_mode)
                .field("username", &"[REDACTED]")
                .field("password", &"[REDACTED]")
                .finish(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalAgentHostCredentialBundle {
    pub model_access_token: LocalAgentLaunchSecret,
    pub provider_context_key_base64: LocalAgentLaunchSecret,
    pub storage: LocalAgentLaunchStorageCredentials,
}

impl fmt::Debug for LocalAgentHostCredentialBundle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LocalAgentHostCredentialBundle")
            .field("model_access_token", &"[REDACTED]")
            .field("provider_context_key_base64", &"[REDACTED]")
            .field("storage", &self.storage)
            .finish()
    }
}

#[async_trait]
impl StorageSecretResolver for LocalAgentHostCredentialBundle {
    async fn resolve_sqlite_encryption_key(
        &self,
        reference: &SecretReference,
    ) -> StorageResult<StorageEncryptionKey> {
        let LocalAgentLaunchStorageCredentials::Sqlite {
            encryption_secret_reference,
            encryption_key_base64,
        } = &self.storage
        else {
            return Err(storage_credential_error());
        };
        if encryption_secret_reference != reference.as_str() {
            return Err(storage_credential_error());
        }
        let decoded = STANDARD
            .decode(encryption_key_base64.expose())
            .map_err(|_| storage_credential_error())?;
        let bytes =
            <[u8; 32]>::try_from(decoded.as_slice()).map_err(|_| storage_credential_error())?;
        Ok(StorageEncryptionKey::new(bytes))
    }

    async fn resolve_postgres(
        &self,
        reference: &SecretReference,
    ) -> StorageResult<PostgresConnectionSettings> {
        let LocalAgentLaunchStorageCredentials::Postgres {
            connection_secret_reference,
            host,
            port,
            database,
            tls_mode,
            username,
            password,
        } = &self.storage
        else {
            return Err(storage_credential_error());
        };
        if connection_secret_reference != reference.as_str() {
            return Err(storage_credential_error());
        }
        let settings = PostgresConnectionSettings {
            endpoint: PostgresEndpoint {
                host: host.clone(),
                port: *port,
                database: database.clone(),
                tls_mode: tls_mode.clone().into(),
            },
            credentials: PostgresCredentials::new(username.clone(), password.expose().to_string())
                .map_err(|_| storage_credential_error())?,
        };
        settings
            .validate()
            .map_err(|_| storage_credential_error())?;
        Ok(settings)
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "transport", rename_all = "snake_case", deny_unknown_fields)]
pub enum LocalAgentHostIpcEndpoint {
    UnixSocket { path: String },
    WindowsNamedPipe { pipe_name: String },
}

impl LocalAgentHostIpcEndpoint {
    pub fn client_endpoint(&self) -> &str {
        match self {
            Self::UnixSocket { path } => path,
            Self::WindowsNamedPipe { pipe_name } => pipe_name
                .strip_prefix(r"\\.\pipe\")
                .unwrap_or(pipe_name.as_str()),
        }
    }

    fn validate(&self) -> Result<(), LocalAgentHostBootstrapError> {
        match self {
            Self::UnixSocket { path } => {
                let path = Path::new(path);
                if !path.is_absolute() || path.as_os_str().is_empty() || path.exists() {
                    return Err(LocalAgentHostBootstrapError::InvalidIpcEndpoint);
                }
            }
            Self::WindowsNamedPipe { pipe_name } => {
                let opaque_id = pipe_name
                    .strip_prefix(WINDOWS_PIPE_PREFIX)
                    .ok_or(LocalAgentHostBootstrapError::InvalidIpcEndpoint)?;
                if !(MINIMUM_OPAQUE_ENDPOINT_ID_BYTES..=MAXIMUM_OPAQUE_ENDPOINT_ID_BYTES)
                    .contains(&opaque_id.len())
                    || !opaque_id
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
                {
                    return Err(LocalAgentHostBootstrapError::InvalidIpcEndpoint);
                }
            }
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalAgentHostLaunchRequest {
    pub protocol_version: u32,
    pub launch_id: String,
    pub owner_user_id: String,
    pub device_id: String,
    pub worker_id: String,
    pub ipc_endpoint: LocalAgentHostIpcEndpoint,
    pub model_gateway_base_url: String,
    pub memory_engine_base_url: String,
    pub memory_source_id: String,
    pub storage_profile: BootstrapStorageProfile,
    pub credentials: LocalAgentHostCredentialBundle,
}

impl fmt::Debug for LocalAgentHostLaunchRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LocalAgentHostLaunchRequest")
            .field("protocol_version", &self.protocol_version)
            .field("launch_id", &self.launch_id)
            .field("owner_user_id", &self.owner_user_id)
            .field("device_id", &self.device_id)
            .field("worker_id", &self.worker_id)
            .field("ipc_endpoint", &self.ipc_endpoint)
            .field("model_gateway_base_url", &self.model_gateway_base_url)
            .field("memory_engine_base_url", &self.memory_engine_base_url)
            .field("memory_source_id", &self.memory_source_id)
            .field("storage_profile", &self.storage_profile)
            .field("credentials", &self.credentials)
            .finish()
    }
}

impl LocalAgentHostLaunchRequest {
    pub fn validate(&self) -> Result<(), LocalAgentHostBootstrapError> {
        if self.protocol_version != LOCAL_AGENT_HOST_LAUNCH_PROTOCOL_VERSION {
            return Err(LocalAgentHostBootstrapError::UnsupportedProtocol);
        }
        for (field, value) in [
            ("launch_id", self.launch_id.as_str()),
            ("owner_user_id", self.owner_user_id.as_str()),
            ("device_id", self.device_id.as_str()),
            ("worker_id", self.worker_id.as_str()),
            ("memory_source_id", self.memory_source_id.as_str()),
        ] {
            validate_identity(field, value)?;
        }
        validate_service_url("model_gateway_base_url", &self.model_gateway_base_url)?;
        validate_service_url("memory_engine_base_url", &self.memory_engine_base_url)?;
        self.ipc_endpoint.validate()?;
        self.storage_profile
            .validate()
            .map_err(|_| LocalAgentHostBootstrapError::StorageCredentialMismatch)?;
        self.credentials
            .model_access_token
            .validate("model_access_token")?;
        self.credentials
            .provider_context_key_base64
            .validate("provider_context_key_base64")?;
        self.provider_context_key()?;
        validate_storage_credentials(&self.storage_profile, &self.credentials.storage)
    }

    pub fn provider_context_key(&self) -> Result<[u8; 32], LocalAgentHostBootstrapError> {
        let decoded = STANDARD
            .decode(self.credentials.provider_context_key_base64.expose())
            .map_err(|_| {
                LocalAgentHostBootstrapError::InvalidCredential("provider_context_key_base64")
            })?;
        <[u8; 32]>::try_from(decoded.as_slice()).map_err(|_| {
            LocalAgentHostBootstrapError::InvalidCredential("provider_context_key_base64")
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalAgentHostReady {
    pub protocol_version: u32,
    pub launch_id: String,
    pub process_id: u32,
    pub client_endpoint: String,
}

pub async fn read_local_agent_host_launch_request<R>(
    reader: &mut R,
) -> Result<LocalAgentHostLaunchRequest, LocalAgentHostBootstrapError>
where
    R: AsyncRead + Unpin,
{
    let length = reader.read_u32().await.map_err(map_read_error)? as usize;
    if length == 0 || length > MAXIMUM_LOCAL_AGENT_LAUNCH_FRAME_BYTES {
        return Err(LocalAgentHostBootstrapError::InvalidFrameSize);
    }
    let mut frame = vec![0_u8; length];
    if let Err(error) = reader.read_exact(frame.as_mut_slice()).await {
        frame.zeroize();
        return Err(map_read_error(error));
    }
    let decoded = serde_json::from_slice::<LocalAgentHostLaunchRequest>(frame.as_slice())
        .map_err(|_| LocalAgentHostBootstrapError::InvalidJson);
    frame.zeroize();
    let request = decoded?;
    request.validate()?;
    Ok(request)
}

pub async fn write_local_agent_host_ready<W>(
    writer: &mut W,
    ready: &LocalAgentHostReady,
) -> Result<(), LocalAgentHostBootstrapError>
where
    W: AsyncWrite + Unpin,
{
    if ready.protocol_version != LOCAL_AGENT_HOST_LAUNCH_PROTOCOL_VERSION
        || ready.process_id == 0
        || ready.launch_id.trim().is_empty()
        || ready.client_endpoint.trim().is_empty()
    {
        return Err(LocalAgentHostBootstrapError::ReadyEncoding);
    }
    let body =
        serde_json::to_vec(ready).map_err(|_| LocalAgentHostBootstrapError::ReadyEncoding)?;
    if body.is_empty() || body.len() > MAXIMUM_LOCAL_AGENT_LAUNCH_FRAME_BYTES {
        return Err(LocalAgentHostBootstrapError::ReadyEncoding);
    }
    writer
        .write_u32(body.len() as u32)
        .await
        .map_err(|_| LocalAgentHostBootstrapError::Io)?;
    writer
        .write_all(body.as_slice())
        .await
        .map_err(|_| LocalAgentHostBootstrapError::Io)?;
    writer
        .flush()
        .await
        .map_err(|_| LocalAgentHostBootstrapError::Io)
}

fn validate_identity(field: &'static str, value: &str) -> Result<(), LocalAgentHostBootstrapError> {
    if value.trim().is_empty()
        || value.trim() != value
        || value.len() > 512
        || value.chars().any(char::is_control)
    {
        Err(LocalAgentHostBootstrapError::InvalidIdentity(field))
    } else {
        Ok(())
    }
}

fn validate_service_url(
    field: &'static str,
    value: &str,
) -> Result<(), LocalAgentHostBootstrapError> {
    let url = Url::parse(value.trim())
        .map_err(|_| LocalAgentHostBootstrapError::InvalidServiceEndpoint(field))?;
    if value.trim() != value
        || !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(LocalAgentHostBootstrapError::InvalidServiceEndpoint(field));
    }
    Ok(())
}

fn validate_storage_credentials(
    profile: &BootstrapStorageProfile,
    credentials: &LocalAgentLaunchStorageCredentials,
) -> Result<(), LocalAgentHostBootstrapError> {
    match (profile, credentials) {
        (
            BootstrapStorageProfile::Sqlite(profile),
            LocalAgentLaunchStorageCredentials::Sqlite {
                encryption_secret_reference,
                encryption_key_base64,
            },
        ) if profile.encryption_secret.as_str() == encryption_secret_reference => {
            encryption_key_base64.validate("sqlite_encryption_key")?;
            let decoded = STANDARD
                .decode(encryption_key_base64.expose())
                .map_err(|_| {
                    LocalAgentHostBootstrapError::InvalidCredential("sqlite_encryption_key")
                })?;
            if decoded.len() != 32 {
                return Err(LocalAgentHostBootstrapError::InvalidCredential(
                    "sqlite_encryption_key",
                ));
            }
            Ok(())
        }
        (
            BootstrapStorageProfile::Postgres(profile),
            LocalAgentLaunchStorageCredentials::Postgres {
                connection_secret_reference,
                host,
                port,
                database,
                tls_mode,
                username,
                password,
            },
        ) if profile.connection_secret.as_str() == connection_secret_reference => {
            for (field, value) in [
                ("postgres_host", host.as_str()),
                ("postgres_database", database.as_str()),
                ("postgres_username", username.as_str()),
            ] {
                if value.trim().is_empty() || value.trim() != value {
                    return Err(LocalAgentHostBootstrapError::InvalidCredential(field));
                }
            }
            if *port == 0 {
                return Err(LocalAgentHostBootstrapError::InvalidCredential(
                    "postgres_port",
                ));
            }
            password.validate("postgres_password")?;
            let native_tls: PostgresTlsMode = tls_mode.clone().into();
            if native_tls == PostgresTlsMode::Disabled && !is_loopback_host(host) {
                return Err(LocalAgentHostBootstrapError::InvalidCredential(
                    "postgres_tls_mode",
                ));
            }
            Ok(())
        }
        _ => Err(LocalAgentHostBootstrapError::StorageCredentialMismatch),
    }
}

fn is_loopback_host(host: &str) -> bool {
    let normalized = host.trim().trim_matches(['[', ']']);
    normalized.eq_ignore_ascii_case("localhost")
        || normalized
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn map_read_error(error: std::io::Error) -> LocalAgentHostBootstrapError {
    if error.kind() == std::io::ErrorKind::UnexpectedEof {
        LocalAgentHostBootstrapError::TruncatedFrame
    } else {
        LocalAgentHostBootstrapError::Io
    }
}

fn storage_credential_error() -> StorageError {
    StorageError::InvalidData {
        reason: "Host launch storage credentials are missing or do not match the profile"
            .to_string(),
    }
}
