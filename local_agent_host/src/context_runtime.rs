// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_client_storage::{ClientStorage, RecordScope};
use chatos_local_agent_protocol::LocalAgentRun;
use chatos_local_agent_runtime::{
    DurableProviderContextCommit, ModelStepContext, ProviderNativeContextCommit,
};
use chrono::{DateTime, Utc};
use tokio_util::sync::CancellationToken;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LocalAgentContextRuntimeError {
    #[error("local Agent context preparation was cancelled")]
    Cancelled,
    #[error("local Agent context runtime failed: {0}")]
    Runtime(String),
}

/// Owns the strategy-specific context boundary used by the local Agent Host.
///
/// Implementations load and decrypt provider-native items or construct the
/// authoritative Memory Engine adapter and scope. Provider-native output is
/// sealed here before the Host gives it to durable storage. Profiles and UI
/// clients never load, merge, encrypt, or persist context themselves.
#[async_trait]
pub trait LocalAgentContextRuntime: Send + Sync {
    async fn prepare_model_step_context(
        &self,
        storage: &dyn ClientStorage,
        scope: &RecordScope,
        run: &LocalAgentRun,
        cancellation: &CancellationToken,
    ) -> Result<ModelStepContext, LocalAgentContextRuntimeError>;

    async fn seal_provider_context_commit(
        &self,
        run: &LocalAgentRun,
        commit: ProviderNativeContextCommit,
        now: DateTime<Utc>,
    ) -> Result<DurableProviderContextCommit, LocalAgentContextRuntimeError>;
}
