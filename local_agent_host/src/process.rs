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
    LocalAgentHostReady, LocalAgentHostServiceError, LocalAgentHostServiceExit,
    LocalAgentIpcMutationExecutor, NativeLocalAgentStoragePlatform,
    NativeLocalAgentStoragePlatformError, RegisteredLocalCapabilityRuntime,
    LOCAL_AGENT_HOST_LAUNCH_PROTOCOL_VERSION,
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
    #[error("native local Agent credential store is unavailable")]
    CredentialStore,
    #[error("native local Agent Host is unsupported on this operating system")]
    UnsupportedPlatform,
    #[error(transparent)]
    StoragePlatform(#[from] NativeLocalAgentStoragePlatformError),
    #[error(transparent)]
    Process(#[from] LocalAgentHostProcessError),
}

/// Runs the complete one-account Host process boundary.
///
/// The native launcher writes exactly one secret-bearing length-prefixed
/// launch frame to `launch_reader`. A non-secret ready frame is emitted only
/// after storage recovery and protected IPC binding have succeeded. From that
/// point, the process owns exactly one durable Worker and one IPC listener
/// until the launcher cancels `shutdown` or either runtime fails.
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
/// Platform-owned credentials and path grants are resolved inside the Host;
/// native clients cannot replace the storage control plane with a remote or
/// legacy executor. The empty capability registry fails closed until verified
/// installed plugin bundles are registered by the production capability
/// loader.
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
    let storage_platform = native_storage_platform(&request)?;
    let dependencies = LocalAgentHostAssemblyDependencies {
        storage_platform,
        terminal_mutation_executor: Arc::new(RejectUnknownNativeMutation),
        capability_runtime: Arc::new(RegisteredLocalCapabilityRuntime::new()),
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

#[cfg(target_os = "macos")]
fn native_storage_platform(
    request: &crate::LocalAgentHostLaunchRequest,
) -> Result<Arc<dyn crate::LocalAgentStoragePlatform>, NativeLocalAgentHostProcessError> {
    let reader = Arc::new(crate::MacOsLocalAgentCredentialReader::production());
    let credentials: Arc<dyn crate::LocalAgentPlatformCredentialReader> = reader.clone();
    let device_keys: Arc<dyn crate::LocalAgentPlatformDeviceKeyReader> = reader;
    NativeLocalAgentStoragePlatform::new(
        request.owner_user_id.clone(),
        request.platform_state_directory.clone(),
        request.storage_profile.clone(),
        credentials,
        device_keys,
    )
    .map(|platform| Arc::new(platform) as Arc<dyn crate::LocalAgentStoragePlatform>)
    .map_err(Into::into)
}

#[cfg(windows)]
fn native_storage_platform(
    request: &crate::LocalAgentHostLaunchRequest,
) -> Result<Arc<dyn crate::LocalAgentStoragePlatform>, NativeLocalAgentHostProcessError> {
    let credentials: Arc<dyn crate::LocalAgentPlatformCredentialReader> = Arc::new(
        crate::WindowsLocalAgentCredentialReader::production()
            .map_err(|_| NativeLocalAgentHostProcessError::CredentialStore)?,
    );
    let device_keys: Arc<dyn crate::LocalAgentPlatformDeviceKeyReader> = Arc::new(
        crate::WindowsLocalAgentDeviceKeyReader::production()
            .map_err(|_| NativeLocalAgentHostProcessError::CredentialStore)?,
    );
    NativeLocalAgentStoragePlatform::new(
        request.owner_user_id.clone(),
        request.platform_state_directory.clone(),
        request.storage_profile.clone(),
        credentials,
        device_keys,
    )
    .map(|platform| Arc::new(platform) as Arc<dyn crate::LocalAgentStoragePlatform>)
    .map_err(Into::into)
}

#[cfg(not(any(target_os = "macos", windows)))]
fn native_storage_platform(
    _request: &crate::LocalAgentHostLaunchRequest,
) -> Result<Arc<dyn crate::LocalAgentStoragePlatform>, NativeLocalAgentHostProcessError> {
    Err(NativeLocalAgentHostProcessError::UnsupportedPlatform)
}
