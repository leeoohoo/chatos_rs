// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;
use std::{
    collections::{HashMap, HashSet},
    fmt,
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use chatos_client_storage::BootstrapStorageProfile;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use url::Url;
use zeroize::{Zeroize, Zeroizing};

use crate::ProvidedLocalAgentCredentials;

pub const LOCAL_AGENT_HOST_LAUNCH_PROTOCOL_VERSION: u32 = 4;
pub const MAXIMUM_LOCAL_AGENT_LAUNCH_FRAME_BYTES: usize = 1024 * 1024;
pub const MAXIMUM_LOCAL_AGENT_SECRET_FRAME_BYTES: usize = 512 * 1024;
const MAXIMUM_LOCAL_AGENT_SECRET_BYTES: usize = 64 * 1024;

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
    #[error("local Agent Host secret frame is empty or exceeds its size boundary")]
    InvalidSecretFrameSize,
    #[error("local Agent Host secret frame is invalid")]
    InvalidSecretFrame,
    #[error("local Agent Host secret frame belongs to another launch")]
    SecretLaunchMismatch,
    #[error("unsupported local Agent Host launch protocol version")]
    UnsupportedProtocol,
    #[error("local Agent Host launch identity is invalid: {0}")]
    InvalidIdentity(&'static str),
    #[error("local Agent Host service endpoint is invalid: {0}")]
    InvalidServiceEndpoint(&'static str),
    #[error("local Agent Host IPC endpoint is invalid")]
    InvalidIpcEndpoint,
    #[error("local Agent Host ready frame could not be encoded")]
    ReadyEncoding,
    #[error("local Agent Host bootstrap I/O failed")]
    Io,
}

/// The ordinary launch frame contains only opaque secure-store references.
/// Values arrive in a separately bounded, correlated one-launch secret frame
/// after the native client has validated the bundled Host identity.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalAgentHostCredentialReferences {
    pub model_access_token_reference: String,
    pub provider_context_key_reference: String,
}

impl LocalAgentHostCredentialReferences {
    fn validate(&self) -> Result<(), LocalAgentHostBootstrapError> {
        validate_identity(
            "model_access_token_reference",
            &self.model_access_token_reference,
        )?;
        validate_identity(
            "provider_context_key_reference",
            &self.provider_context_key_reference,
        )
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
    pub attachment_grant_directory: String,
    pub platform_state_directory: String,
    pub model_gateway_base_url: String,
    pub memory_engine_base_url: String,
    pub memory_source_id: String,
    pub storage_profile: BootstrapStorageProfile,
    pub credential_references: LocalAgentHostCredentialReferences,
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
            .field("attachment_grant_directory", &"[PRIVATE DIRECTORY]")
            .field("platform_state_directory", &"[PRIVATE DIRECTORY]")
            .field("model_gateway_base_url", &self.model_gateway_base_url)
            .field("memory_engine_base_url", &self.memory_engine_base_url)
            .field("memory_source_id", &self.memory_source_id)
            .field("storage_profile", &self.storage_profile)
            .field("credential_references", &self.credential_references)
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
        for (field, value) in [
            (
                "attachment_grant_directory",
                self.attachment_grant_directory.as_str(),
            ),
            (
                "platform_state_directory",
                self.platform_state_directory.as_str(),
            ),
        ] {
            let directory = Path::new(value);
            if !directory.is_absolute() || value.trim().is_empty() {
                return Err(LocalAgentHostBootstrapError::InvalidIdentity(field));
            }
        }
        self.storage_profile
            .validate()
            .map_err(|_| LocalAgentHostBootstrapError::InvalidIdentity("storage_profile"))?;
        self.credential_references.validate()
    }

    fn required_credential_references(&self) -> HashSet<&str> {
        let mut references = HashSet::from([
            self.credential_references
                .model_access_token_reference
                .as_str(),
            self.credential_references
                .provider_context_key_reference
                .as_str(),
        ]);
        match &self.storage_profile {
            BootstrapStorageProfile::Sqlite(profile) => {
                references.insert(profile.encryption_secret.as_str());
            }
            BootstrapStorageProfile::Postgres(profile) => {
                references.insert(profile.connection_secret.as_str());
            }
        }
        references
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EncodedLocalAgentHostSecretFrame {
    protocol_version: u32,
    launch_id: String,
    secrets: Vec<EncodedLocalAgentHostSecret>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EncodedLocalAgentHostSecret {
    reference: String,
    value_base64: String,
}

impl Drop for EncodedLocalAgentHostSecret {
    fn drop(&mut self) {
        self.value_base64.zeroize();
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

pub async fn read_local_agent_host_secret_frame<R>(
    reader: &mut R,
    request: &LocalAgentHostLaunchRequest,
) -> Result<ProvidedLocalAgentCredentials, LocalAgentHostBootstrapError>
where
    R: AsyncRead + Unpin,
{
    let length = reader.read_u32().await.map_err(map_read_error)? as usize;
    if length == 0 || length > MAXIMUM_LOCAL_AGENT_SECRET_FRAME_BYTES {
        return Err(LocalAgentHostBootstrapError::InvalidSecretFrameSize);
    }
    let mut frame = vec![0_u8; length];
    if let Err(error) = reader.read_exact(frame.as_mut_slice()).await {
        frame.zeroize();
        return Err(map_read_error(error));
    }
    let decoded = serde_json::from_slice::<EncodedLocalAgentHostSecretFrame>(frame.as_slice())
        .map_err(|_| LocalAgentHostBootstrapError::InvalidSecretFrame);
    frame.zeroize();
    let secret_frame = decoded?;
    if secret_frame.protocol_version != LOCAL_AGENT_HOST_LAUNCH_PROTOCOL_VERSION {
        return Err(LocalAgentHostBootstrapError::UnsupportedProtocol);
    }
    if secret_frame.launch_id != request.launch_id {
        return Err(LocalAgentHostBootstrapError::SecretLaunchMismatch);
    }

    let required = request.required_credential_references();
    if secret_frame.secrets.len() != required.len() {
        return Err(LocalAgentHostBootstrapError::InvalidSecretFrame);
    }
    let mut values = HashMap::with_capacity(required.len());
    for secret in secret_frame.secrets {
        if !required.contains(secret.reference.as_str())
            || values.contains_key(secret.reference.as_str())
        {
            return Err(LocalAgentHostBootstrapError::InvalidSecretFrame);
        }
        let decoded = Zeroizing::new(
            STANDARD
                .decode(secret.value_base64.as_bytes())
                .map_err(|_| LocalAgentHostBootstrapError::InvalidSecretFrame)?,
        );
        if decoded.is_empty() || decoded.len() > MAXIMUM_LOCAL_AGENT_SECRET_BYTES {
            return Err(LocalAgentHostBootstrapError::InvalidSecretFrame);
        }
        values.insert(secret.reference.clone(), decoded);
    }
    if values.len() != required.len() {
        return Err(LocalAgentHostBootstrapError::InvalidSecretFrame);
    }
    ProvidedLocalAgentCredentials::new(request.owner_user_id.clone(), values)
        .map_err(|_| LocalAgentHostBootstrapError::InvalidSecretFrame)
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

fn map_read_error(error: std::io::Error) -> LocalAgentHostBootstrapError {
    if error.kind() == std::io::ErrorKind::UnexpectedEof {
        LocalAgentHostBootstrapError::TruncatedFrame
    } else {
        LocalAgentHostBootstrapError::Io
    }
}
