// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Duration;

use chatos_agent_profiles::{MainChatAgentProfile, TaskRunnerAgentProfile};
use chatos_client_storage::{ClientStorage, ClientStorageFactory, RecordScope, StorageError};
use chatos_local_agent_runtime::{
    HttpModelGatewayClient, LocalAgentProfile, MemoryEngineContextAdapter, MemorySyncPolicy,
    MemorySynchronizer, ModelGatewayCallbacks,
};
use chrono::Utc;
use memory_engine_sdk::MemoryEngineClient;
use tokio_util::sync::CancellationToken;

use crate::{
    build_local_agent_ipc_server, FrozenCapabilityLocalToolRuntime, LocalAgentContextRuntimeError,
    LocalAgentExecutionSession, LocalAgentHost, LocalAgentHostBootstrapError,
    LocalAgentHostIpcEndpoint, LocalAgentHostPolicy, LocalAgentHostService,
    LocalAgentHostStartupReport, LocalAgentHostWorker, LocalAgentIpcMutationExecutor,
    LocalAgentIpcServerError, LocalAgentProfileRegistry, LocalAgentStoragePlatform,
    LocalAttachmentGrantResolver, ProviderContextEncryptionKey, RegisteredLocalCapabilityRuntime,
    StandardLocalAgentContextRuntime, StoredLocalTaskCreationPlanner,
    StoredMainChatContextProvider, StoredTaskRunnerContextProvider,
};

const MEMORY_ENGINE_TIMEOUT: Duration = Duration::from_secs(180);

pub struct LocalAgentHostAssemblyDependencies {
    pub storage_platform: Arc<dyn LocalAgentStoragePlatform>,
    pub terminal_mutation_executor: Arc<dyn LocalAgentIpcMutationExecutor>,
    pub capability_runtime: Arc<RegisteredLocalCapabilityRuntime>,
}

#[derive(Debug, thiserror::Error)]
pub enum LocalAgentHostAssemblyError {
    #[error(transparent)]
    Bootstrap(#[from] LocalAgentHostBootstrapError),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("model gateway could not be configured: {0}")]
    ModelGateway(String),
    #[error("Memory Engine could not be configured: {0}")]
    MemoryEngine(String),
    #[error(transparent)]
    Context(#[from] LocalAgentContextRuntimeError),
    #[error("local Agent profiles could not be registered: {0}")]
    Profiles(String),
    #[error("local Agent Host could not start: {0}")]
    Host(String),
    #[error(transparent)]
    IpcServer(#[from] LocalAgentIpcServerError),
    #[error("the launch IPC transport does not match this operating system")]
    WrongPlatformTransport,
    #[error("local Agent IPC listener could not bind: {0}")]
    IpcTransport(String),
}

/// Fully assembled production Host resources. The native launcher may register
/// verified local capability bundles before running `service`; planning and
/// execution share this exact registry instance.
pub struct AssembledLocalAgentHost {
    pub service: LocalAgentHostService,
    pub session: LocalAgentExecutionSession,
    pub capability_runtime: Arc<RegisteredLocalCapabilityRuntime>,
    pub startup_report: LocalAgentHostStartupReport,
    pub client_endpoint: String,
}

pub async fn assemble_local_agent_host(
    request: &crate::LocalAgentHostLaunchRequest,
    dependencies: LocalAgentHostAssemblyDependencies,
) -> Result<AssembledLocalAgentHost, LocalAgentHostAssemblyError> {
    request.validate()?;
    let storage: Arc<dyn ClientStorage> = Arc::from(
        ClientStorageFactory::open(&request.storage_profile, &request.credentials).await?,
    );
    let scope = RecordScope {
        owner_user_id: request.owner_user_id.clone(),
    };
    let attachment_resolver = Arc::new(
        LocalAttachmentGrantResolver::open(&request.attachment_grant_directory)
            .map_err(|error| LocalAgentHostAssemblyError::Host(error.to_string()))?,
    );
    let gateway = Arc::new(
        HttpModelGatewayClient::new(request.model_gateway_base_url.as_str())
            .map_err(|error| LocalAgentHostAssemblyError::ModelGateway(error.to_string()))?,
    );
    let memory_client = MemoryEngineClient::new_direct(
        request.memory_engine_base_url.clone(),
        MEMORY_ENGINE_TIMEOUT,
        request.memory_source_id.clone(),
    )
    .map_err(LocalAgentHostAssemblyError::MemoryEngine)?
    .with_bearer_token(request.credentials.model_access_token.expose().to_string());
    let memory_context = MemoryEngineContextAdapter::new(
        Arc::new(memory_client.clone()),
        request.memory_source_id.clone(),
    )
    .map_err(|error| LocalAgentHostAssemblyError::MemoryEngine(error.to_string()))?;
    let memory_sync = MemorySynchronizer::new(
        Arc::new(memory_client),
        request.owner_user_id.clone(),
        request.memory_source_id.clone(),
        MemorySyncPolicy::default(),
    )
    .map_err(LocalAgentHostAssemblyError::MemoryEngine)?;
    let provider_key = ProviderContextEncryptionKey::new(request.provider_context_key()?);
    let context_runtime = Arc::new(StandardLocalAgentContextRuntime::new(
        &provider_key,
        memory_context,
        memory_sync,
        request.owner_user_id.clone(),
    )?);

    let main_context = Arc::new(StoredMainChatContextProvider::new(
        storage.clone(),
        scope.clone(),
        attachment_resolver.clone(),
    ));
    let task_context = Arc::new(StoredTaskRunnerContextProvider::new(
        storage.clone(),
        scope.clone(),
        attachment_resolver,
    ));
    let profiles = LocalAgentProfileRegistry::new([
        Arc::new(MainChatAgentProfile::new(main_context)) as Arc<dyn LocalAgentProfile>,
        Arc::new(TaskRunnerAgentProfile::new(task_context)) as Arc<dyn LocalAgentProfile>,
    ])
    .map_err(|error| LocalAgentHostAssemblyError::Profiles(error.to_string()))?;
    let task_planner = Arc::new(StoredLocalTaskCreationPlanner::new(
        storage.clone(),
        scope.clone(),
        dependencies.capability_runtime.clone(),
    ));
    let tool_runtime = Arc::new(FrozenCapabilityLocalToolRuntime::new(
        storage.clone(),
        scope.clone(),
        dependencies.capability_runtime.clone(),
    ));
    let (host, startup_report) = LocalAgentHost::start(
        storage.clone(),
        gateway,
        context_runtime,
        tool_runtime,
        task_planner,
        profiles,
        scope.clone(),
        request.device_id.clone(),
        LocalAgentHostPolicy::default(),
        Utc::now(),
    )
    .await
    .map_err(|error| LocalAgentHostAssemblyError::Host(error.to_string()))?;
    let host = Arc::new(host);
    let host_cancellation = CancellationToken::new();
    let session = LocalAgentExecutionSession::new(
        request.credentials.model_access_token.expose().to_string(),
        ModelGatewayCallbacks::default(),
        host_cancellation,
    )
    .map_err(|error| LocalAgentHostAssemblyError::Host(error.to_string()))?;
    let worker = Arc::new(
        LocalAgentHostWorker::new(host.clone(), session.clone(), request.worker_id.clone())
            .map_err(|error| LocalAgentHostAssemblyError::Host(error.to_string()))?,
    );
    let ipc_server = build_local_agent_ipc_server(
        storage,
        scope,
        host,
        session.clone(),
        dependencies.storage_platform,
        dependencies.terminal_mutation_executor,
    )?;
    let client_endpoint = request.ipc_endpoint.client_endpoint().to_string();
    let service =
        LocalAgentHostService::new(worker, bind_transport(&request.ipc_endpoint, ipc_server)?);
    Ok(AssembledLocalAgentHost {
        service,
        session,
        capability_runtime: dependencies.capability_runtime,
        startup_report,
        client_endpoint,
    })
}

fn bind_transport(
    endpoint: &LocalAgentHostIpcEndpoint,
    server: Arc<crate::LocalAgentIpcServer>,
) -> Result<Box<dyn crate::LocalAgentIpcTransport>, LocalAgentHostAssemblyError> {
    #[cfg(unix)]
    {
        let LocalAgentHostIpcEndpoint::UnixSocket { path } = endpoint else {
            return Err(LocalAgentHostAssemblyError::WrongPlatformTransport);
        };
        // SAFETY: geteuid has no preconditions and does not dereference memory.
        let expected_peer_uid = unsafe { libc::geteuid() };
        return crate::UnixLocalAgentIpcTransport::bind(path, server, expected_peer_uid)
            .map(|transport| Box::new(transport) as Box<dyn crate::LocalAgentIpcTransport>)
            .map_err(|error| LocalAgentHostAssemblyError::IpcTransport(error.to_string()));
    }
    #[cfg(windows)]
    {
        let LocalAgentHostIpcEndpoint::WindowsNamedPipe { pipe_name } = endpoint else {
            return Err(LocalAgentHostAssemblyError::WrongPlatformTransport);
        };
        return crate::WindowsLocalAgentIpcTransport::bind(pipe_name, server)
            .map(|transport| Box::new(transport) as Box<dyn crate::LocalAgentIpcTransport>)
            .map_err(|error| LocalAgentHostAssemblyError::IpcTransport(error.to_string()));
    }
    #[allow(unreachable_code)]
    Err(LocalAgentHostAssemblyError::WrongPlatformTransport)
}
