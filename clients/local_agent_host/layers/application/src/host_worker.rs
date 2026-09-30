// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{LocalAgentRuntime, LocalAgentRuntimeError};
use chatos_local_agent_protocol::{LocalAgentRunRecord, LocalAgentToolInvocationRecord};

impl LocalAgentRuntime {
    /// Trusted in-process worker lookup. Native callers must use the
    /// owner-scoped `get_run` IPC command instead.
    #[doc(hidden)]
    pub async fn get_run_for_host_worker(
        &self,
        run_id: &str,
    ) -> Result<Option<LocalAgentRunRecord>, LocalAgentRuntimeError> {
        Ok(self.store.get_run(run_id).await?)
    }

    /// Trusted in-process worker lookup used to keep Host-owned tool claims
    /// and commits off the native IPC surface.
    #[doc(hidden)]
    pub async fn get_tool_invocation_for_host_worker(
        &self,
        invocation_id: &str,
    ) -> Result<Option<LocalAgentToolInvocationRecord>, LocalAgentRuntimeError> {
        Ok(self.store.get_tool_invocation(invocation_id).await?)
    }
}
