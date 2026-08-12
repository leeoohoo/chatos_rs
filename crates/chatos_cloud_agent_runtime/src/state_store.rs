// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chatos_cloud_agent_protocol::CloudAgentRunRecord;
use tokio::sync::Mutex;

use crate::{
    CloudAgentAtomicTransition, CloudAgentClaim, CloudAgentClaimResult, CloudAgentOutboxIntent,
    CloudAgentRunStore, MongoCloudAgentRunStore,
};

#[derive(Default)]
struct InMemoryCloudAgentState {
    runs: HashMap<String, CloudAgentRunRecord>,
    lanes: HashMap<String, InMemoryLane>,
    claims: HashMap<String, String>,
    outbox: HashMap<String, CloudAgentOutboxIntent>,
}

#[derive(Debug, Clone, Default)]
struct InMemoryLane {
    next_lane_seq: u64,
    active_lane_seq: u64,
}

#[derive(Clone, Default)]
pub struct InMemoryCloudAgentRunStore {
    state: Arc<Mutex<InMemoryCloudAgentState>>,
}

impl InMemoryCloudAgentRunStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn allocate_lane_seq(&self, ordering_lane_key: &str) -> Result<u64, String> {
        if ordering_lane_key.trim().is_empty() {
            return Err("ordering_lane_key must not be empty".to_string());
        }
        let mut state = self.state.lock().await;
        let lane = state
            .lanes
            .entry(ordering_lane_key.to_string())
            .or_default();
        lane.next_lane_seq = lane
            .next_lane_seq
            .checked_add(1)
            .ok_or_else(|| "lane_seq overflow".to_string())?;
        if lane.active_lane_seq == 0 {
            lane.active_lane_seq = 1;
        }
        Ok(lane.next_lane_seq)
    }

    pub async fn insert_run(&self, record: CloudAgentRunRecord) -> Result<(), String> {
        record.validate()?;
        let mut state = self.state.lock().await;
        let lane = state
            .lanes
            .get(record.ordering.ordering_lane_key.as_str())
            .ok_or_else(|| "Cloud Agent lane must be allocated before run insert".to_string())?;
        if record.ordering.lane_seq > lane.next_lane_seq {
            return Err("Cloud Agent run lane_seq was not allocated by the lane store".to_string());
        }
        if state.runs.values().any(|existing| {
            existing.ordering.ordering_lane_key == record.ordering.ordering_lane_key
                && existing.ordering.lane_seq == record.ordering.lane_seq
        }) {
            return Err("Cloud Agent lane sequence is already assigned".to_string());
        }
        if state
            .runs
            .insert(record.ordering.agent_run_id.clone(), record)
            .is_some()
        {
            return Err("Cloud Agent run id is already assigned".to_string());
        }
        Ok(())
    }

    pub async fn advance_lane_after_terminal(
        &self,
        ordering_lane_key: &str,
        completed_lane_seq: u64,
    ) -> Result<Option<u64>, String> {
        let mut state = self.state.lock().await;
        let Some(lane) = state.lanes.get_mut(ordering_lane_key) else {
            return Ok(None);
        };
        if lane.active_lane_seq != completed_lane_seq || lane.next_lane_seq <= completed_lane_seq {
            return Ok(None);
        }
        lane.active_lane_seq = completed_lane_seq
            .checked_add(1)
            .ok_or_else(|| "lane_seq overflow".to_string())?;
        Ok(Some(lane.active_lane_seq))
    }

    pub async fn list_ready_outbox(
        &self,
        limit: i64,
    ) -> Result<Vec<CloudAgentOutboxIntent>, String> {
        let now = chrono::Utc::now();
        let mut intents = self
            .state
            .lock()
            .await
            .outbox
            .values()
            .filter(|intent| intent.available_at <= now)
            .cloned()
            .collect::<Vec<_>>();
        intents.sort_by(|left, right| {
            left.available_at
                .cmp(&right.available_at)
                .then_with(|| left.event_id.cmp(&right.event_id))
        });
        intents.truncate(usize::try_from(limit.max(1)).unwrap_or(usize::MAX));
        Ok(intents)
    }

    pub async fn mark_outbox_published(&self, event_id: &str) -> Result<bool, String> {
        Ok(self.state.lock().await.outbox.remove(event_id).is_some())
    }
}

#[async_trait]
impl CloudAgentRunStore for InMemoryCloudAgentRunStore {
    async fn load_run(&self, agent_run_id: &str) -> Result<Option<CloudAgentRunRecord>, String> {
        Ok(self.state.lock().await.runs.get(agent_run_id).cloned())
    }

    async fn acquire_short_claim(
        &self,
        claim: &CloudAgentClaim,
    ) -> Result<CloudAgentClaimResult, String> {
        claim.validate()?;
        let mut state = self.state.lock().await;
        let active_lane_seq = state
            .lanes
            .get(claim.ordering.ordering_lane_key.as_str())
            .map(|lane| lane.active_lane_seq);
        if active_lane_seq != Some(claim.ordering.lane_seq) {
            return Ok(CloudAgentClaimResult::OutOfOrder);
        }
        let Some(run) = state.runs.get(claim.ordering.agent_run_id.as_str()) else {
            return Ok(CloudAgentClaimResult::Conflict);
        };
        if run.status.is_terminal() {
            return Ok(CloudAgentClaimResult::Terminal);
        }
        if run.ordering.generation > claim.ordering.generation
            || run.ordering.step_seq > claim.ordering.step_seq
            || run.version > claim.expected_version
        {
            return Ok(CloudAgentClaimResult::Duplicate);
        }
        if run.ordering != claim.ordering
            || run.status != claim.expected_status
            || run.phase != claim.expected_phase
            || run.version != claim.expected_version
        {
            return Ok(CloudAgentClaimResult::Conflict);
        }
        if state
            .claims
            .get(claim.ordering.agent_run_id.as_str())
            .is_some_and(|token| token != &claim.claim_token)
        {
            return Ok(CloudAgentClaimResult::Conflict);
        }
        state.claims.insert(
            claim.ordering.agent_run_id.clone(),
            claim.claim_token.clone(),
        );
        Ok(CloudAgentClaimResult::Acquired)
    }

    async fn commit_transition(
        &self,
        transition: CloudAgentAtomicTransition,
    ) -> Result<bool, String> {
        transition.validate()?;
        let mut state = self.state.lock().await;
        let claim = &transition.claim;
        if state.claims.get(claim.ordering.agent_run_id.as_str()) != Some(&claim.claim_token) {
            return Ok(false);
        }
        let Some(run) = state.runs.get_mut(claim.ordering.agent_run_id.as_str()) else {
            return Ok(false);
        };
        if run.ordering != claim.ordering
            || run.status != claim.expected_status
            || run.phase != claim.expected_phase
            || run.version != claim.expected_version
        {
            return Ok(false);
        }
        run.status = transition.next_status;
        run.phase = transition.next_phase;
        run.ordering.step_seq = transition.next_step_seq;
        run.iteration = transition.next_iteration;
        run.retry_count = transition.next_retry_count;
        run.pending_batch_id = transition.pending_batch_id;
        run.pending_tool_calls = transition.pending_tool_calls;
        run.pending_tool_results = transition.pending_tool_results;
        run.terminal_outcome = transition.terminal_outcome;
        run.version = run.version.saturating_add(1);
        run.updated_at = chrono::Utc::now();
        for intent in transition.outbox {
            state
                .outbox
                .entry(intent.event_id.clone())
                .or_insert(intent);
        }
        state.claims.remove(claim.ordering.agent_run_id.as_str());
        Ok(true)
    }

    async fn release_short_claim(&self, claim: &CloudAgentClaim) -> Result<(), String> {
        let mut state = self.state.lock().await;
        if state.claims.get(claim.ordering.agent_run_id.as_str()) == Some(&claim.claim_token) {
            state.claims.remove(claim.ordering.agent_run_id.as_str());
        }
        Ok(())
    }
}

#[derive(Clone)]
pub enum CloudAgentStateStore {
    Memory(InMemoryCloudAgentRunStore),
    Mongo(MongoCloudAgentRunStore),
}

impl CloudAgentStateStore {
    pub fn memory() -> Self {
        Self::Memory(InMemoryCloudAgentRunStore::new())
    }

    pub async fn connect(database_url: &str) -> Result<Self, String> {
        Ok(Self::Mongo(
            MongoCloudAgentRunStore::connect(database_url).await?,
        ))
    }

    pub async fn allocate_lane_seq(&self, ordering_lane_key: &str) -> Result<u64, String> {
        match self {
            Self::Memory(store) => store.allocate_lane_seq(ordering_lane_key).await,
            Self::Mongo(store) => store.allocate_lane_seq(ordering_lane_key).await,
        }
    }

    pub async fn insert_run(&self, record: CloudAgentRunRecord) -> Result<(), String> {
        match self {
            Self::Memory(store) => store.insert_run(record).await,
            Self::Mongo(store) => store.insert_run(record).await,
        }
    }

    pub async fn advance_lane_after_terminal(
        &self,
        ordering_lane_key: &str,
        completed_lane_seq: u64,
    ) -> Result<Option<u64>, String> {
        match self {
            Self::Memory(store) => {
                store
                    .advance_lane_after_terminal(ordering_lane_key, completed_lane_seq)
                    .await
            }
            Self::Mongo(store) => {
                store
                    .advance_lane_after_terminal(ordering_lane_key, completed_lane_seq)
                    .await
            }
        }
    }

    pub async fn list_ready_outbox(
        &self,
        limit: i64,
    ) -> Result<Vec<CloudAgentOutboxIntent>, String> {
        match self {
            Self::Memory(store) => store.list_ready_outbox(limit).await,
            Self::Mongo(store) => store.list_ready_outbox(limit).await,
        }
    }

    pub async fn mark_outbox_published(&self, event_id: &str) -> Result<bool, String> {
        match self {
            Self::Memory(store) => store.mark_outbox_published(event_id).await,
            Self::Mongo(store) => store.mark_outbox_published(event_id).await,
        }
    }
}

#[async_trait]
impl CloudAgentRunStore for CloudAgentStateStore {
    async fn load_run(&self, agent_run_id: &str) -> Result<Option<CloudAgentRunRecord>, String> {
        match self {
            Self::Memory(store) => store.load_run(agent_run_id).await,
            Self::Mongo(store) => store.load_run(agent_run_id).await,
        }
    }

    async fn acquire_short_claim(
        &self,
        claim: &CloudAgentClaim,
    ) -> Result<CloudAgentClaimResult, String> {
        match self {
            Self::Memory(store) => store.acquire_short_claim(claim).await,
            Self::Mongo(store) => store.acquire_short_claim(claim).await,
        }
    }

    async fn commit_transition(
        &self,
        transition: CloudAgentAtomicTransition,
    ) -> Result<bool, String> {
        match self {
            Self::Memory(store) => store.commit_transition(transition).await,
            Self::Mongo(store) => store.commit_transition(transition).await,
        }
    }

    async fn release_short_claim(&self, claim: &CloudAgentClaim) -> Result<(), String> {
        match self {
            Self::Memory(store) => store.release_short_claim(claim).await,
            Self::Mongo(store) => store.release_short_claim(claim).await,
        }
    }
}
