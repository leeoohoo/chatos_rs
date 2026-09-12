// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_local_agent_host::{
    LocalAgentHostError, LocalAgentHostService, LocalAgentHostServiceError,
    LocalAgentHostServiceExit, LocalAgentIpcTransport, LocalAgentIpcTransportError,
    LocalAgentWorkerExit, LocalAgentWorkerRuntime,
};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

struct WaitingWorker {
    started: Arc<Notify>,
    cancelled: Arc<Notify>,
}

#[async_trait]
impl LocalAgentWorkerRuntime for WaitingWorker {
    async fn run(
        &self,
        cancellation: CancellationToken,
    ) -> Result<LocalAgentWorkerExit, LocalAgentHostError> {
        self.started.notify_one();
        cancellation.cancelled().await;
        self.cancelled.notify_one();
        Ok(LocalAgentWorkerExit {
            processed_event_count: 0,
        })
    }
}

struct WaitingTransport {
    started: Arc<Notify>,
}

#[async_trait]
impl LocalAgentIpcTransport for WaitingTransport {
    async fn serve(
        self: Box<Self>,
        cancellation: CancellationToken,
    ) -> Result<(), LocalAgentIpcTransportError> {
        self.started.notify_one();
        cancellation.cancelled().await;
        Ok(())
    }
}

struct FailingTransport;

#[async_trait]
impl LocalAgentIpcTransport for FailingTransport {
    async fn serve(
        self: Box<Self>,
        _cancellation: CancellationToken,
    ) -> Result<(), LocalAgentIpcTransportError> {
        Err(LocalAgentIpcTransportError::Failed {
            platform: "test",
            detail: "listener failed".to_string(),
        })
    }
}

struct ExitingTransport;

#[async_trait]
impl LocalAgentIpcTransport for ExitingTransport {
    async fn serve(
        self: Box<Self>,
        _cancellation: CancellationToken,
    ) -> Result<(), LocalAgentIpcTransportError> {
        Ok(())
    }
}

fn waiting_worker(
    started: Arc<Notify>,
    cancelled: Arc<Notify>,
) -> Arc<dyn LocalAgentWorkerRuntime> {
    Arc::new(WaitingWorker { started, cancelled })
}

#[tokio::test]
async fn explicit_shutdown_cancels_worker_and_transport_as_one_host() {
    let worker_started = Arc::new(Notify::new());
    let worker_cancelled = Arc::new(Notify::new());
    let transport_started = Arc::new(Notify::new());
    let service = LocalAgentHostService::new(
        waiting_worker(worker_started.clone(), worker_cancelled.clone()),
        Box::new(WaitingTransport {
            started: transport_started.clone(),
        }),
    );
    let shutdown = CancellationToken::new();
    let running = tokio::spawn(service.run(shutdown.clone()));
    worker_started.notified().await;
    transport_started.notified().await;
    shutdown.cancel();

    assert_eq!(
        running.await.unwrap().unwrap(),
        LocalAgentHostServiceExit {
            worker: LocalAgentWorkerExit {
                processed_event_count: 0,
            },
        }
    );
    worker_cancelled.notified().await;
}

#[tokio::test]
async fn transport_failure_cancels_the_worker_and_is_not_hidden() {
    let worker_started = Arc::new(Notify::new());
    let worker_cancelled = Arc::new(Notify::new());
    let service = LocalAgentHostService::new(
        waiting_worker(worker_started.clone(), worker_cancelled.clone()),
        Box::new(FailingTransport),
    );
    let error = service.run(CancellationToken::new()).await.unwrap_err();

    assert!(matches!(
        error,
        LocalAgentHostServiceError::Transport(LocalAgentIpcTransportError::Failed {
            platform: "test",
            ..
        })
    ));
    worker_cancelled.notified().await;
}

#[tokio::test]
async fn clean_transport_exit_without_shutdown_is_a_host_failure() {
    let worker_started = Arc::new(Notify::new());
    let worker_cancelled = Arc::new(Notify::new());
    let service = LocalAgentHostService::new(
        waiting_worker(worker_started, worker_cancelled.clone()),
        Box::new(ExitingTransport),
    );
    let error = service.run(CancellationToken::new()).await.unwrap_err();

    assert!(matches!(
        error,
        LocalAgentHostServiceError::UnexpectedTransportExit
    ));
    worker_cancelled.notified().await;
}
