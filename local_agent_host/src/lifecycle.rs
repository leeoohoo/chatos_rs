// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{ClientStorage, RecordScope};
use tokio_util::sync::CancellationToken;

use crate::{
    LocalAgentExecutionSession, LocalAgentHost, LocalAgentHostControlExecutor,
    LocalAgentHostCreationExecutor, LocalAgentHostError, LocalAgentHostWorker,
    LocalAgentIpcMutationExecutor, LocalAgentIpcServer, LocalAgentIpcServerError,
    LocalAgentStorageIpcExecutor, LocalAgentStoragePlatform, LocalAgentWorkerExit,
};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LocalAgentIpcTransportError {
    #[error("{platform} local IPC failed: {detail}")]
    Failed {
        platform: &'static str,
        detail: String,
    },
}

#[async_trait]
pub trait LocalAgentIpcTransport: Send {
    async fn serve(
        self: Box<Self>,
        cancellation: CancellationToken,
    ) -> Result<(), LocalAgentIpcTransportError>;
}

#[cfg(unix)]
#[async_trait]
impl LocalAgentIpcTransport for crate::UnixLocalAgentIpcTransport {
    async fn serve(
        self: Box<Self>,
        cancellation: CancellationToken,
    ) -> Result<(), LocalAgentIpcTransportError> {
        (*self)
            .serve(cancellation)
            .await
            .map_err(|error| LocalAgentIpcTransportError::Failed {
                platform: "Unix",
                detail: error.to_string(),
            })
    }
}

#[cfg(windows)]
#[async_trait]
impl LocalAgentIpcTransport for crate::WindowsLocalAgentIpcTransport {
    async fn serve(
        self: Box<Self>,
        cancellation: CancellationToken,
    ) -> Result<(), LocalAgentIpcTransportError> {
        (*self)
            .serve(cancellation)
            .await
            .map_err(|error| LocalAgentIpcTransportError::Failed {
                platform: "Windows",
                detail: error.to_string(),
            })
    }
}

#[async_trait]
pub trait LocalAgentWorkerRuntime: Send + Sync {
    async fn run(
        &self,
        cancellation: CancellationToken,
    ) -> Result<LocalAgentWorkerExit, LocalAgentHostError>;
}

#[async_trait]
impl LocalAgentWorkerRuntime for LocalAgentHostWorker {
    async fn run(
        &self,
        cancellation: CancellationToken,
    ) -> Result<LocalAgentWorkerExit, LocalAgentHostError> {
        self.run(cancellation).await
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalAgentHostServiceExit {
    pub worker: LocalAgentWorkerExit,
}

#[derive(Debug, thiserror::Error)]
pub enum LocalAgentHostServiceError {
    #[error(transparent)]
    Worker(#[from] LocalAgentHostError),
    #[error(transparent)]
    Transport(#[from] LocalAgentIpcTransportError),
    #[error("local Agent Worker exited before Host shutdown")]
    UnexpectedWorkerExit,
    #[error("local Agent IPC transport exited before Host shutdown")]
    UnexpectedTransportExit,
}

/// Owns the two long-lived pieces of the local Host process: exactly one
/// durable event Worker and exactly one protected native IPC listener. If
/// either one fails or exits unexpectedly, the sibling is cancelled before
/// this service returns.
pub struct LocalAgentHostService {
    worker: Arc<dyn LocalAgentWorkerRuntime>,
    transport: Box<dyn LocalAgentIpcTransport>,
}

impl LocalAgentHostService {
    pub fn new(
        worker: Arc<dyn LocalAgentWorkerRuntime>,
        transport: Box<dyn LocalAgentIpcTransport>,
    ) -> Self {
        Self { worker, transport }
    }

    pub async fn run(
        self,
        shutdown: CancellationToken,
    ) -> Result<LocalAgentHostServiceExit, LocalAgentHostServiceError> {
        let service_cancellation = shutdown.child_token();
        let worker_cancellation = service_cancellation.clone();
        let transport_cancellation = service_cancellation.clone();
        let worker = self.worker;
        let mut worker_run = Box::pin(worker.run(worker_cancellation));
        let mut transport_run = Box::pin(self.transport.serve(transport_cancellation));

        tokio::select! {
            worker_result = &mut worker_run => {
                service_cancellation.cancel();
                let transport_result = transport_run.await;
                if shutdown.is_cancelled() {
                    transport_result?;
                    Ok(LocalAgentHostServiceExit { worker: worker_result? })
                } else {
                    transport_result?;
                    worker_result?;
                    Err(LocalAgentHostServiceError::UnexpectedWorkerExit)
                }
            }
            transport_result = &mut transport_run => {
                service_cancellation.cancel();
                let worker_result = worker_run.await;
                if shutdown.is_cancelled() {
                    transport_result?;
                    Ok(LocalAgentHostServiceExit { worker: worker_result? })
                } else {
                    transport_result?;
                    worker_result?;
                    Err(LocalAgentHostServiceError::UnexpectedTransportExit)
                }
            }
        }
    }
}

/// Builds the only mutation chain exposed to the native client. The tail is a
/// required typed executor for platform-owned commands such as tool approval;
/// no command is redirected to a remote Agent service or a legacy fallback.
#[allow(clippy::too_many_arguments)]
pub fn build_local_agent_ipc_server(
    storage: Arc<dyn ClientStorage>,
    scope: RecordScope,
    host: Arc<LocalAgentHost>,
    session: LocalAgentExecutionSession,
    storage_platform: Arc<dyn LocalAgentStoragePlatform>,
    terminal_mutation_executor: Arc<dyn LocalAgentIpcMutationExecutor>,
) -> Result<Arc<LocalAgentIpcServer>, LocalAgentIpcServerError> {
    let storage_executor: Arc<dyn LocalAgentIpcMutationExecutor> =
        Arc::new(LocalAgentStorageIpcExecutor::new(
            storage.clone(),
            scope.clone(),
            storage_platform,
            terminal_mutation_executor,
        ));
    let control_executor: Arc<dyn LocalAgentIpcMutationExecutor> = Arc::new(
        LocalAgentHostControlExecutor::new(host.clone(), storage_executor),
    );
    let creation_executor: Arc<dyn LocalAgentIpcMutationExecutor> = Arc::new(
        LocalAgentHostCreationExecutor::new(host, session, control_executor),
    );
    Ok(Arc::new(LocalAgentIpcServer::new(
        storage,
        scope,
        creation_executor,
    )?))
}
