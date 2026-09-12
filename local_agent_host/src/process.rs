// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::sync::CancellationToken;

use crate::{
    assemble_local_agent_host, read_local_agent_host_launch_request, write_local_agent_host_ready,
    LocalAgentHostAssemblyDependencies, LocalAgentHostAssemblyError, LocalAgentHostBootstrapError,
    LocalAgentHostReady, LocalAgentHostServiceError, LocalAgentHostServiceExit,
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
    let assembly = assemble_local_agent_host(&request, dependencies).await?;
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
