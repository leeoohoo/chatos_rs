// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{LocalAgentRuntime, LocalAgentRuntimeError};
use chatos_local_agent_ports::{
    LocalHookEventRecord, LocalHookEventSpec, LocalHookInvocationCompletion,
    LocalHookInvocationRecord, LocalHookInvocationSpec, LocalHookOutputBlobRecord,
};
use chatos_local_agent_protocol::LocalPluginHookBundleRecord;

impl LocalAgentRuntime {
    #[doc(hidden)]
    pub async fn has_hook_event_for_host_worker(
        &self,
        owner_user_id: &str,
        event_type: &str,
        subject_id: &str,
    ) -> Result<bool, LocalAgentRuntimeError> {
        Ok(self
            .store
            .has_hook_event(owner_user_id, event_type, subject_id)
            .await?)
    }

    #[doc(hidden)]
    pub async fn get_hook_event_for_host_worker(
        &self,
        owner_user_id: &str,
        event_type: &str,
        subject_id: &str,
        sequence: u64,
    ) -> Result<Option<LocalHookEventRecord>, LocalAgentRuntimeError> {
        Ok(self
            .store
            .get_hook_event(owner_user_id, event_type, subject_id, sequence)
            .await?)
    }

    #[doc(hidden)]
    pub async fn list_pending_hook_events_for_host_worker(
        &self,
        owner_user_id: &str,
        limit: u32,
    ) -> Result<Vec<LocalHookEventRecord>, LocalAgentRuntimeError> {
        Ok(self
            .store
            .list_pending_hook_events(owner_user_id, limit)
            .await?)
    }

    #[doc(hidden)]
    pub async fn active_plugin_hook_bundles_for_host_worker(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<LocalPluginHookBundleRecord>, LocalAgentRuntimeError> {
        Ok(self.store.list_plugin_hook_bundles(owner_user_id).await?)
    }

    #[doc(hidden)]
    pub async fn plugin_hook_bundles_for_host_worker(
        &self,
        owner_user_id: &str,
        bundle_ids: &[String],
    ) -> Result<Vec<LocalPluginHookBundleRecord>, LocalAgentRuntimeError> {
        Ok(self
            .store
            .get_plugin_hook_bundles(owner_user_id, bundle_ids)
            .await?)
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
    pub async fn list_hook_output_blobs_for_host_worker(
        &self,
        invocation_id: &str,
    ) -> Result<Vec<LocalHookOutputBlobRecord>, LocalAgentRuntimeError> {
        Ok(self.store.list_hook_output_blobs(invocation_id).await?)
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
