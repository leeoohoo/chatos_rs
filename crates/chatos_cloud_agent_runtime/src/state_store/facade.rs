// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_cloud_agent_protocol::CloudAgentRunRecord;

use super::InMemoryCloudAgentRunStore;
use crate::{
    CloudAgentAtomicTransition, CloudAgentClaim, CloudAgentClaimResult, CloudAgentOutboxIntent,
    CloudAgentOutboxPublishFailure, CloudAgentPendingOutboxIntent, CloudAgentRunStore,
    CloudAgentStateRepository,
};

#[derive(Clone)]
pub enum CloudAgentStateStore {
    Memory(InMemoryCloudAgentRunStore),
    Repository(Arc<dyn CloudAgentStateRepository>),
}

impl CloudAgentStateStore {
    pub fn memory() -> Self {
        Self::Memory(InMemoryCloudAgentRunStore::new())
    }

    pub fn from_repository<S>(store: S) -> Self
    where
        S: CloudAgentStateRepository + 'static,
    {
        Self::Repository(Arc::new(store))
    }

    fn repository(&self) -> &dyn CloudAgentStateRepository {
        match self {
            Self::Memory(store) => store,
            Self::Repository(store) => store.as_ref(),
        }
    }

    pub async fn allocate_lane_seq(&self, ordering_lane_key: &str) -> Result<u64, String> {
        self.repository().allocate_lane_seq(ordering_lane_key).await
    }

    pub async fn insert_run(&self, record: CloudAgentRunRecord) -> Result<(), String> {
        self.insert_run_with_outbox(record, Vec::new()).await
    }

    pub async fn insert_run_with_outbox(
        &self,
        record: CloudAgentRunRecord,
        outbox: Vec<CloudAgentOutboxIntent>,
    ) -> Result<(), String> {
        self.repository()
            .insert_run_with_outbox(record, outbox)
            .await
    }

    pub async fn advance_lane_after_terminal(
        &self,
        ordering_lane_key: &str,
        completed_lane_seq: u64,
    ) -> Result<Option<u64>, String> {
        self.repository()
            .advance_lane_after_terminal(ordering_lane_key, completed_lane_seq)
            .await
    }

    pub async fn list_ready_outbox(
        &self,
        limit: i64,
    ) -> Result<Vec<CloudAgentOutboxIntent>, String> {
        match self {
            Self::Memory(store) => store.list_ready_outbox(limit).await,
            Self::Repository(_) => Err(
                "unclaimed Cloud Agent outbox inspection is only available for the in-memory store"
                    .to_string(),
            ),
        }
    }

    pub(crate) async fn claim_ready_outbox_with_attempts(
        &self,
        limit: i64,
        claim_token: &str,
        claim_until: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<CloudAgentPendingOutboxIntent>, String> {
        self.repository()
            .claim_ready_outbox_with_attempts(limit, claim_token, claim_until)
            .await
    }

    pub(crate) async fn mark_claimed_outbox_published(
        &self,
        event_id: &str,
        claim_token: &str,
    ) -> Result<bool, String> {
        self.repository()
            .mark_claimed_outbox_published(event_id, claim_token)
            .await
    }

    pub(crate) async fn mark_claimed_outbox_publish_failed(
        &self,
        event_id: &str,
        claim_token: &str,
        error: &str,
        next_available_at: chrono::DateTime<chrono::Utc>,
        max_attempts: u32,
    ) -> Result<Option<CloudAgentOutboxPublishFailure>, String> {
        self.repository()
            .mark_claimed_outbox_publish_failed(
                event_id,
                claim_token,
                error,
                next_available_at,
                max_attempts,
            )
            .await
    }

    pub async fn mark_outbox_published(&self, event_id: &str) -> Result<bool, String> {
        match self {
            Self::Memory(store) => store.mark_outbox_published(event_id).await,
            Self::Repository(_) => Err(
                "unclaimed Cloud Agent outbox acknowledgement is only available for the in-memory store"
                    .to_string(),
            ),
        }
    }

    pub async fn mark_outbox_publish_failed(
        &self,
        event_id: &str,
        error: &str,
        next_available_at: chrono::DateTime<chrono::Utc>,
        max_attempts: u32,
    ) -> Result<Option<CloudAgentOutboxPublishFailure>, String> {
        match self {
            Self::Memory(store) => {
                store
                    .mark_outbox_publish_failed(event_id, error, next_available_at, max_attempts)
                    .await
            }
            Self::Repository(_) => Err(
                "unclaimed Cloud Agent outbox failure is only available for the in-memory store"
                    .to_string(),
            ),
        }
    }
}

#[async_trait]
impl CloudAgentRunStore for CloudAgentStateStore {
    async fn load_run(&self, agent_run_id: &str) -> Result<Option<CloudAgentRunRecord>, String> {
        self.repository().load_run(agent_run_id).await
    }

    async fn acquire_short_claim(
        &self,
        claim: &CloudAgentClaim,
    ) -> Result<CloudAgentClaimResult, String> {
        self.repository().acquire_short_claim(claim).await
    }

    async fn renew_short_claim(&self, claim: &CloudAgentClaim) -> Result<bool, String> {
        self.repository().renew_short_claim(claim).await
    }

    async fn commit_transition(
        &self,
        transition: CloudAgentAtomicTransition,
    ) -> Result<bool, String> {
        self.repository().commit_transition(transition).await
    }

    async fn release_short_claim(&self, claim: &CloudAgentClaim) -> Result<(), String> {
        self.repository().release_short_claim(claim).await
    }
}
