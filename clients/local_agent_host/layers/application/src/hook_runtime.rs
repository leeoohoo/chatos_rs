// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{LocalAgentRuntime, LocalAgentRuntimeError};
use chatos_local_agent_ports::{
    LocalHookEventRecord, LocalHookEventSpec, LocalHookInvocationCompletion,
    LocalHookInvocationRecord, LocalHookInvocationSpec,
};
use chatos_local_agent_protocol::LocalPluginHookBundleRecord;

impl LocalAgentRuntime {
    #[doc(hidden)]
    pub async fn active_plugin_hook_bundles_for_host_worker(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<LocalPluginHookBundleRecord>, LocalAgentRuntimeError> {
        Ok(self.store.list_plugin_hook_bundles(owner_user_id).await?)
    }

    #[doc(hidden)]
    pub async fn prepare_hook_event_for_host_worker(
        &self,
        event: &LocalHookEventSpec,
    ) -> Result<LocalHookEventRecord, LocalAgentRuntimeError> {
        Ok(self.store.prepare_hook_event(event, self.now()?).await?)
    }

    #[doc(hidden)]
    pub async fn begin_hook_invocation_for_host_worker(
        &self,
        invocation: &LocalHookInvocationSpec,
    ) -> Result<LocalHookInvocationRecord, LocalAgentRuntimeError> {
        Ok(self
            .store
            .begin_hook_invocation(invocation, self.now()?)
            .await?)
    }

    #[doc(hidden)]
    pub async fn complete_hook_invocation_for_host_worker(
        &self,
        invocation_id: &str,
        expected_attempt: u32,
        completion: &LocalHookInvocationCompletion,
    ) -> Result<LocalHookInvocationRecord, LocalAgentRuntimeError> {
        Ok(self
            .store
            .complete_hook_invocation(invocation_id, expected_attempt, completion, self.now()?)
            .await?)
    }

    #[doc(hidden)]
    pub async fn complete_hook_event_for_host_worker(
        &self,
        event_id: &str,
        additional_context: Option<&str>,
    ) -> Result<LocalHookEventRecord, LocalAgentRuntimeError> {
        Ok(self
            .store
            .complete_hook_event(event_id, additional_context, self.now()?)
            .await?)
    }
}
