// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_local_agent_protocol::{LocalAgentCommand, LocalAgentIpcError, LocalAgentIpcResponse};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::sync::CancellationToken;

use crate::{
    assemble_local_agent_host, read_local_agent_host_launch_request, write_local_agent_host_ready,
    LocalAgentHostAssemblyDependencies, LocalAgentHostAssemblyError, LocalAgentHostBootstrapError,
    LocalAgentHostReady, LocalAgentHostResolvedCredentials, LocalAgentHostServiceError,
    LocalAgentHostServiceExit, LocalAgentIpcMutationExecutor, NativeLocalAgentStoragePlatform,
    NativeLocalAgentStoragePlatformError, LOCAL_AGENT_HOST_LAUNCH_PROTOCOL_VERSION,
};

#[derive(Debug, thiserror::Error)]
pub enum LocalAgentHostProcessError {
    #[error(transparent)]
    Bootstrap(#[from] LocalAgentHostBootstrapError),
    #[error(transparent)]
    Assembly(#[from] LocalAgentHostAssemblyError),
    #[error(transparent)]
    Service(#[from] LocalAgentHostServiceError),
}

#[derive(Debug, thiserror::Error)]
pub enum NativeLocalAgentHostProcessError {
    #[error(transparent)]
    Bootstrap(#[from] LocalAgentHostBootstrapError),
    #[error("native local Agent {credential} is unavailable: {source}")]
    CredentialStore {
        credential: &'static str,
        #[source]
        source: crate::LocalAgentPlatformCredentialError,
    },
    #[error(transparent)]
    StoragePlatform(#[from] NativeLocalAgentStoragePlatformError),
    #[error(transparent)]
    Process(#[from] LocalAgentHostProcessError),
}

/// Runs the complete one-account Host process boundary.
///
/// This dependency-injected boundary reads one reference-only launch frame.
/// A ready frame is emitted only after storage recovery and protected IPC
/// binding have succeeded. From that point, the process owns exactly one
/// durable Worker and one IPC listener until the launcher cancels `shutdown`
/// or either runtime fails.
pub async fn run_local_agent_host_process<R, W>(
    launch_reader: &mut R,
    ready_writer: &mut W,
    dependencies: LocalAgentHostAssemblyDependencies,
    shutdown: CancellationToken,
) -> Result<LocalAgentHostServiceExit, LocalAgentHostProcessError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let request = read_local_agent_host_launch_request(launch_reader).await?;
    run_local_agent_host_request(&request, ready_writer, dependencies, shutdown).await
}

/// Runs a previously validated request. Native process assembly uses this
/// boundary after it has constructed the operating-system credential and
/// storage adapters from the request's owner and private state directory.
pub async fn run_local_agent_host_request<W>(
    request: &crate::LocalAgentHostLaunchRequest,
    ready_writer: &mut W,
    dependencies: LocalAgentHostAssemblyDependencies,
    shutdown: CancellationToken,
) -> Result<LocalAgentHostServiceExit, LocalAgentHostProcessError>
where
    W: AsyncWrite + Unpin,
{
    let assembly = assemble_local_agent_host(request, dependencies).await?;
    let ready = LocalAgentHostReady {
        protocol_version: LOCAL_AGENT_HOST_LAUNCH_PROTOCOL_VERSION,
        launch_id: request.launch_id.clone(),
        process_id: std::process::id(),
        client_endpoint: assembly.client_endpoint.clone(),
    };
    if let Err(error) = write_local_agent_host_ready(ready_writer, &ready).await {
        assembly.session.cancel();
        return Err(error.into());
    }
    let session = assembly.session.clone();
    let result = assembly.service.run(shutdown).await;
    session.cancel();
    result.map_err(Into::into)
}

/// Production entry point used by the bundled macOS and Windows executable.
///
/// The native client first sends a reference-only launch frame and then one
/// correlated, bounded secret frame over the inherited anonymous pipe. The
/// Host zeroizes the encoded frame and retains only the current launch's
/// in-memory credential map. Installed Plugin records are signature- and
/// digest-checked before their project-scoped local MCP tools enter the shared
/// registry.
pub async fn run_native_local_agent_host_process<R, W>(
    launch_reader: &mut R,
    ready_writer: &mut W,
    shutdown: CancellationToken,
) -> Result<LocalAgentHostServiceExit, NativeLocalAgentHostProcessError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let request = read_local_agent_host_launch_request(launch_reader).await?;
    let provided_credentials =
        Arc::new(crate::read_local_agent_host_secret_frame(launch_reader, &request).await?);
    let credential_reader: Arc<dyn crate::LocalAgentPlatformCredentialReader> =
        provided_credentials.clone();
    let device_key_reader: Arc<dyn crate::LocalAgentPlatformDeviceKeyReader> = provided_credentials;
    let (storage_platform, capability_platform, credentials) =
        build_native_process_dependencies(&request, credential_reader, device_key_reader)?;
    let dependencies = LocalAgentHostAssemblyDependencies {
        credentials,
        storage_platform,
        capability_platform,
        terminal_mutation_executor: Arc::new(RejectUnknownNativeMutation),
    };
    run_local_agent_host_request(&request, ready_writer, dependencies, shutdown)
        .await
        .map_err(Into::into)
}

struct RejectUnknownNativeMutation;

#[async_trait]
impl LocalAgentIpcMutationExecutor for RejectUnknownNativeMutation {
    async fn execute_mutation(
        &self,
        _request_id: &str,
        _command: LocalAgentCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        Err(LocalAgentIpcError {
            code: "unsupported_local_agent_command".to_string(),
            message: "local Agent command is not implemented by the native Host".to_string(),
            retryable: false,
        })
    }
}

type NativeProcessDependencies = (
    Arc<dyn crate::LocalAgentStoragePlatform>,
    Arc<dyn crate::LocalCapabilityPlatform>,
    LocalAgentHostResolvedCredentials,
);

fn build_native_process_dependencies(
    request: &crate::LocalAgentHostLaunchRequest,
    credential_reader: Arc<dyn crate::LocalAgentPlatformCredentialReader>,
    device_key_reader: Arc<dyn crate::LocalAgentPlatformDeviceKeyReader>,
) -> Result<NativeProcessDependencies, NativeLocalAgentHostProcessError> {
    const MAXIMUM_NATIVE_SECRET_BYTES: usize = 64 * 1024;

    let model_access_token = credential_reader
        .read(
            request.owner_user_id.as_str(),
            request
                .credential_references
                .model_access_token_reference
                .as_str(),
        )
        .map_err(|source| NativeLocalAgentHostProcessError::CredentialStore {
            credential: "model access token",
            source,
        })?;
    if model_access_token.is_empty() || model_access_token.len() > MAXIMUM_NATIVE_SECRET_BYTES {
        return Err(NativeLocalAgentHostProcessError::CredentialStore {
            credential: "model access token",
            source: crate::LocalAgentPlatformCredentialError::Unavailable,
        });
    }
    let model_access_token = String::from_utf8(model_access_token.to_vec()).map_err(|_| {
        NativeLocalAgentHostProcessError::CredentialStore {
            credential: "model access token",
            source: crate::LocalAgentPlatformCredentialError::Unavailable,
        }
    })?;
    let provider_context_key = device_key_reader
        .read_device_key(
            request.owner_user_id.as_str(),
            request
                .credential_references
                .provider_context_key_reference
                .as_str(),
        )
        .map_err(|source| NativeLocalAgentHostProcessError::CredentialStore {
            credential: "provider context key",
            source,
        })?;
    let provider_context_key =
        <[u8; 32]>::try_from(provider_context_key.as_slice()).map_err(|_| {
            NativeLocalAgentHostProcessError::CredentialStore {
                credential: "provider context key",
                source: crate::LocalAgentPlatformCredentialError::Unavailable,
            }
        })?;

    let platform = Arc::new(NativeLocalAgentStoragePlatform::new(
        request.owner_user_id.clone(),
        request.platform_state_directory.clone(),
        request.storage_profile.clone(),
        credential_reader,
        device_key_reader,
    )?);
    let storage_secrets: Arc<dyn chatos_client_storage::StorageSecretResolver> = platform.clone();
    let resolved = LocalAgentHostResolvedCredentials::new(
        model_access_token,
        provider_context_key,
        storage_secrets,
    )
    .map_err(|_| NativeLocalAgentHostProcessError::CredentialStore {
        credential: "resolved credential set",
        source: crate::LocalAgentPlatformCredentialError::Unavailable,
    })?;
    let storage_platform: Arc<dyn crate::LocalAgentStoragePlatform> = platform.clone();
    let capability_platform: Arc<dyn crate::LocalCapabilityPlatform> = platform;
    Ok((storage_platform, capability_platform, resolved))
}
