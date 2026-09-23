// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chatos_cloud_agent_protocol::CloudAgentRunRecord;
use tokio::sync::Mutex;

use crate::{
    validate_initial_cloud_agent_state, CloudAgentAtomicTransition, CloudAgentClaim,
    CloudAgentClaimResult, CloudAgentOutboxIntent, CloudAgentOutboxPublishFailure,
    CloudAgentPendingOutboxIntent, CloudAgentRunStore, CloudAgentStateRepository,
};

#[derive(Default)]
struct InMemoryCloudAgentState {
    runs: HashMap<String, CloudAgentRunRecord>,
    lanes: HashMap<String, InMemoryLane>,
    claims: HashMap<String, InMemoryClaim>,
    outbox: HashMap<String, InMemoryCloudAgentOutboxRecord>,
}

#[derive(Debug, Clone)]
struct InMemoryClaim {
    token: String,
    until: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Default)]
struct InMemoryLane {
    next_lane_seq: u64,
    active_lane_seq: u64,
}

#[derive(Debug, Clone)]
struct InMemoryCloudAgentOutboxRecord {
    intent: CloudAgentOutboxIntent,
    available_at: chrono::DateTime<chrono::Utc>,
    publish_attempts: u32,
    last_error: Option<String>,
    dead_lettered: bool,
    publish_claim: Option<InMemoryClaim>,
}

impl InMemoryCloudAgentOutboxRecord {
    fn pending(intent: CloudAgentOutboxIntent) -> Self {
        Self {
            available_at: intent.available_at,
            intent,
            publish_attempts: 0,
            last_error: None,
            dead_lettered: false,
            publish_claim: None,
        }
    }
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
        self.insert_run_with_outbox(record, Vec::new()).await
    }

    pub async fn insert_run_with_outbox(
        &self,
        record: CloudAgentRunRecord,
        outbox: Vec<CloudAgentOutboxIntent>,
    ) -> Result<(), String> {
        validate_initial_cloud_agent_state(&record, &outbox)?;
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
            .contains_key(record.ordering.agent_run_id.as_str())
        {
            return Err("Cloud Agent run id is already assigned".to_string());
        }
        state
            .runs
            .insert(record.ordering.agent_run_id.clone(), record);
        for intent in outbox {
            state
                .outbox
                .entry(intent.event_id.clone())
                .or_insert_with(|| InMemoryCloudAgentOutboxRecord::pending(intent));
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
        if lane.active_lane_seq != completed_lane_seq {
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
        Ok(self
            .list_ready_outbox_with_attempts(limit)
            .await?
            .into_iter()
            .map(|record| record.intent)
            .collect())
    }

    pub(crate) async fn list_ready_outbox_with_attempts(
        &self,
        limit: i64,
    ) -> Result<Vec<CloudAgentPendingOutboxIntent>, String> {
        let now = chrono::Utc::now();
        let mut intents = self
            .state
            .lock()
            .await
            .outbox
            .values()
            .filter(|record| !record.dead_lettered && record.available_at <= now)
            .map(|record| {
                let mut intent = record.intent.clone();
                intent.available_at = record.available_at;
                CloudAgentPendingOutboxIntent {
                    intent,
                    publish_attempts: record.publish_attempts,
                }
            })
            .collect::<Vec<_>>();
        intents.sort_by(|left, right| {
            left.intent
                .available_at
                .cmp(&right.intent.available_at)
                .then_with(|| left.intent.event_id.cmp(&right.intent.event_id))
        });
        intents.truncate(usize::try_from(limit.max(1)).unwrap_or(usize::MAX));
        Ok(intents)
    }

    async fn claim_ready_outbox_with_attempts(
        &self,
        limit: i64,
        claim_token: &str,
        claim_until: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<CloudAgentPendingOutboxIntent>, String> {
        if claim_token.trim().is_empty() {
            return Err("Cloud Agent outbox claim token must not be empty".to_string());
        }
        let now = chrono::Utc::now();
        if claim_until <= now {
            return Err("Cloud Agent outbox claim deadline must be in the future".to_string());
        }
        let mut state = self.state.lock().await;
        let mut event_ids = state
            .outbox
            .iter()
            .filter(|(_, record)| {
                !record.dead_lettered
                    && record.available_at <= now
                    && record
                        .publish_claim
                        .as_ref()
                        .is_none_or(|claim| claim.until <= now)
            })
            .map(|(event_id, record)| (event_id.clone(), record.available_at))
            .collect::<Vec<_>>();
        event_ids.sort_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)));
        event_ids.truncate(usize::try_from(limit.max(1)).unwrap_or(usize::MAX));

        let mut claimed = Vec::with_capacity(event_ids.len());
        for (event_id, _) in event_ids {
            let record = state
                .outbox
                .get_mut(event_id.as_str())
                .ok_or_else(|| "Cloud Agent outbox candidate disappeared".to_string())?;
            record.publish_claim = Some(InMemoryClaim {
                token: claim_token.to_string(),
                until: claim_until,
            });
            let mut intent = record.intent.clone();
            intent.available_at = record.available_at;
            claimed.push(CloudAgentPendingOutboxIntent {
                intent,
                publish_attempts: record.publish_attempts,
            });
        }
        Ok(claimed)
    }

    async fn mark_claimed_outbox_published(
        &self,
        event_id: &str,
        claim_token: &str,
    ) -> Result<bool, String> {
        let mut state = self.state.lock().await;
        let claimed = state
            .outbox
            .get(event_id)
            .and_then(|record| record.publish_claim.as_ref())
            .is_some_and(|claim| claim.token == claim_token);
        if !claimed {
            return Ok(false);
        }
        state.outbox.remove(event_id);
        Ok(true)
    }

    async fn mark_claimed_outbox_publish_failed(
        &self,
        event_id: &str,
        claim_token: &str,
        error: &str,
        next_available_at: chrono::DateTime<chrono::Utc>,
        max_attempts: u32,
    ) -> Result<Option<CloudAgentOutboxPublishFailure>, String> {
        let mut state = self.state.lock().await;
        let Some(record) = state.outbox.get_mut(event_id) else {
            return Ok(None);
        };
        if record.dead_lettered
            || record
                .publish_claim
                .as_ref()
                .is_none_or(|claim| claim.token != claim_token)
        {
            return Ok(None);
        }
        record.publish_attempts = record.publish_attempts.saturating_add(1);
        record.last_error = Some(bounded_outbox_publish_error(error));
        record.dead_lettered = record.publish_attempts >= max_attempts.max(1);
        record.available_at = next_available_at;
        record.publish_claim = None;
        Ok(Some(CloudAgentOutboxPublishFailure {
            publish_attempts: record.publish_attempts,
            dead_lettered: record.dead_lettered,
            available_at: record.available_at,
        }))
    }

    pub async fn mark_outbox_published(&self, event_id: &str) -> Result<bool, String> {
        Ok(self.state.lock().await.outbox.remove(event_id).is_some())
    }

    pub async fn mark_outbox_publish_failed(
        &self,
        event_id: &str,
        error: &str,
        next_available_at: chrono::DateTime<chrono::Utc>,
        max_attempts: u32,
    ) -> Result<Option<CloudAgentOutboxPublishFailure>, String> {
        let mut state = self.state.lock().await;
        let Some(record) = state.outbox.get_mut(event_id) else {
            return Ok(None);
        };
        if record.dead_lettered {
            return Ok(None);
        }
        record.publish_attempts = record.publish_attempts.saturating_add(1);
        record.last_error = Some(bounded_outbox_publish_error(error));
        record.dead_lettered = record.publish_attempts >= max_attempts.max(1);
        record.available_at = next_available_at;
        Ok(Some(CloudAgentOutboxPublishFailure {
            publish_attempts: record.publish_attempts,
            dead_lettered: record.dead_lettered,
            available_at: record.available_at,
        }))
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
        if let Some(existing) = state.claims.get(claim.ordering.agent_run_id.as_str()) {
            if existing.token != claim.claim_token && existing.until > chrono::Utc::now() {
                return Ok(CloudAgentClaimResult::Conflict);
            }
        }
        state.claims.insert(
            claim.ordering.agent_run_id.clone(),
            InMemoryClaim {
                token: claim.claim_token.clone(),
                until: claim.claim_until,
            },
        );
        Ok(CloudAgentClaimResult::Acquired)
    }

    async fn renew_short_claim(&self, claim: &CloudAgentClaim) -> Result<bool, String> {
        claim.validate()?;
        let mut state = self.state.lock().await;
        let Some(existing) = state.claims.get(claim.ordering.agent_run_id.as_str()) else {
            return Ok(false);
        };
        if existing.token != claim.claim_token {
            return Ok(false);
        }
        let Some(run) = state.runs.get(claim.ordering.agent_run_id.as_str()) else {
            return Ok(false);
        };
        if run.ordering != claim.ordering
            || run.status != claim.expected_status
            || run.phase != claim.expected_phase
            || run.version != claim.expected_version
            || run.status.is_terminal()
        {
            return Ok(false);
        }
        if let Some(existing) = state.claims.get_mut(claim.ordering.agent_run_id.as_str()) {
            existing.until = claim.claim_until;
        }
        Ok(true)
    }

    async fn commit_transition(
        &self,
        transition: CloudAgentAtomicTransition,
    ) -> Result<bool, String> {
        transition.validate()?;
        let mut state = self.state.lock().await;
        let claim = &transition.claim;
        if state
            .claims
            .get(claim.ordering.agent_run_id.as_str())
            .is_none_or(|existing| existing.token != claim.claim_token)
        {
            return Ok(false);
        }
        {
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
            run.input = transition.next_input;
            run.status = transition.next_status;
            run.phase = transition.next_phase;
            run.ordering.step_seq = transition.next_step_seq;
            run.iteration = transition.next_iteration;
            run.retry_count = transition.next_retry_count;
            run.previous_response_id = transition.previous_response_id;
            run.continuation_mode = transition.continuation_mode;
            run.current_input_items_ref = transition.current_input_items_ref;
            run.mcp_runtime_session_ref = transition.mcp_runtime_session_ref;
            run.pending_batch_id = transition.pending_batch_id;
            run.pending_tool_calls = transition.pending_tool_calls;
            run.pending_tool_results = transition.pending_tool_results;
            run.response_input_items = transition.response_input_items;
            run.usage_accumulator = transition.usage_accumulator;
            run.terminal_outcome = transition.terminal_outcome;
            run.version = run.version.saturating_add(1);
            run.updated_at = chrono::Utc::now();
        }
        for intent in transition.outbox {
            state
                .outbox
                .entry(intent.event_id.clone())
                .or_insert_with(|| InMemoryCloudAgentOutboxRecord::pending(intent));
        }
        if transition.next_status.is_terminal() {
            let lane = state
                .lanes
                .get_mut(claim.ordering.ordering_lane_key.as_str())
                .ok_or_else(|| "claimed Cloud Agent lane is missing".to_string())?;
            if lane.active_lane_seq != claim.ordering.lane_seq {
                return Err("claimed Cloud Agent lane changed before terminal commit".to_string());
            }
            lane.active_lane_seq = claim
                .ordering
                .lane_seq
                .checked_add(1)
                .ok_or_else(|| "lane_seq overflow".to_string())?;
        }
        state.claims.remove(claim.ordering.agent_run_id.as_str());
        Ok(true)
    }

    async fn release_short_claim(&self, claim: &CloudAgentClaim) -> Result<(), String> {
        let mut state = self.state.lock().await;
        if state
            .claims
            .get(claim.ordering.agent_run_id.as_str())
            .is_some_and(|existing| existing.token == claim.claim_token)
        {
            state.claims.remove(claim.ordering.agent_run_id.as_str());
        }
        Ok(())
    }
}

#[async_trait]
impl CloudAgentStateRepository for InMemoryCloudAgentRunStore {
    async fn allocate_lane_seq(&self, key: &str) -> Result<u64, String> {
        InMemoryCloudAgentRunStore::allocate_lane_seq(self, key).await
    }
    async fn insert_run_with_outbox(
        &self,
        record: CloudAgentRunRecord,
        outbox: Vec<CloudAgentOutboxIntent>,
    ) -> Result<(), String> {
        InMemoryCloudAgentRunStore::insert_run_with_outbox(self, record, outbox).await
    }
    async fn advance_lane_after_terminal(
        &self,
        key: &str,
        seq: u64,
    ) -> Result<Option<u64>, String> {
        InMemoryCloudAgentRunStore::advance_lane_after_terminal(self, key, seq).await
    }
    async fn claim_ready_outbox_with_attempts(
        &self,
        limit: i64,
        claim_token: &str,
        claim_until: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<CloudAgentPendingOutboxIntent>, String> {
        InMemoryCloudAgentRunStore::claim_ready_outbox_with_attempts(
            self,
            limit,
            claim_token,
            claim_until,
        )
        .await
    }
    async fn mark_claimed_outbox_published(
        &self,
        id: &str,
        claim_token: &str,
    ) -> Result<bool, String> {
        InMemoryCloudAgentRunStore::mark_claimed_outbox_published(self, id, claim_token).await
    }
    async fn mark_claimed_outbox_publish_failed(
        &self,
        id: &str,
        claim_token: &str,
        error: &str,
        next: chrono::DateTime<chrono::Utc>,
        max: u32,
    ) -> Result<Option<CloudAgentOutboxPublishFailure>, String> {
        InMemoryCloudAgentRunStore::mark_claimed_outbox_publish_failed(
            self,
            id,
            claim_token,
            error,
            next,
            max,
        )
        .await
    }
}

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

    pub async fn allocate_lane_seq(&self, ordering_lane_key: &str) -> Result<u64, String> {
        match self {
            Self::Memory(store) => store.allocate_lane_seq(ordering_lane_key).await,
            Self::Repository(store) => store.allocate_lane_seq(ordering_lane_key).await,
        }
    }

    pub async fn insert_run(&self, record: CloudAgentRunRecord) -> Result<(), String> {
        self.insert_run_with_outbox(record, Vec::new()).await
    }

    pub async fn insert_run_with_outbox(
        &self,
        record: CloudAgentRunRecord,
        outbox: Vec<CloudAgentOutboxIntent>,
    ) -> Result<(), String> {
        match self {
            Self::Memory(store) => store.insert_run_with_outbox(record, outbox).await,
            Self::Repository(store) => store.insert_run_with_outbox(record, outbox).await,
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
            Self::Repository(store) => {
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
        match self {
            Self::Memory(store) => {
                store
                    .claim_ready_outbox_with_attempts(limit, claim_token, claim_until)
                    .await
            }
            Self::Repository(store) => {
                store
                    .claim_ready_outbox_with_attempts(limit, claim_token, claim_until)
                    .await
            }
        }
    }

    pub(crate) async fn mark_claimed_outbox_published(
        &self,
        event_id: &str,
        claim_token: &str,
    ) -> Result<bool, String> {
        match self {
            Self::Memory(store) => {
                store
                    .mark_claimed_outbox_published(event_id, claim_token)
                    .await
            }
            Self::Repository(store) => {
                store
                    .mark_claimed_outbox_published(event_id, claim_token)
                    .await
            }
        }
    }

    pub(crate) async fn mark_claimed_outbox_publish_failed(
        &self,
        event_id: &str,
        claim_token: &str,
        error: &str,
        next_available_at: chrono::DateTime<chrono::Utc>,
        max_attempts: u32,
    ) -> Result<Option<CloudAgentOutboxPublishFailure>, String> {
        match self {
            Self::Memory(store) => {
                store
                    .mark_claimed_outbox_publish_failed(
                        event_id,
                        claim_token,
                        error,
                        next_available_at,
                        max_attempts,
                    )
                    .await
            }
            Self::Repository(store) => {
                store
                    .mark_claimed_outbox_publish_failed(
                        event_id,
                        claim_token,
                        error,
                        next_available_at,
                        max_attempts,
                    )
                    .await
            }
        }
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
            Self::Repository(store) => {
                let _ = store;
                Err(
                    "unclaimed Cloud Agent outbox failure is only available for the in-memory store"
                        .to_string(),
                )
            }
        }
    }
}

pub(super) fn bounded_outbox_publish_error(error: &str) -> String {
    const MAX_ERROR_CHARS: usize = 2_000;
    error.chars().take(MAX_ERROR_CHARS).collect()
}

#[async_trait]
impl CloudAgentRunStore for CloudAgentStateStore {
    async fn load_run(&self, agent_run_id: &str) -> Result<Option<CloudAgentRunRecord>, String> {
        match self {
            Self::Memory(store) => store.load_run(agent_run_id).await,
            Self::Repository(store) => store.load_run(agent_run_id).await,
        }
    }

    async fn acquire_short_claim(
        &self,
        claim: &CloudAgentClaim,
    ) -> Result<CloudAgentClaimResult, String> {
        match self {
            Self::Memory(store) => store.acquire_short_claim(claim).await,
            Self::Repository(store) => store.acquire_short_claim(claim).await,
        }
    }

    async fn renew_short_claim(&self, claim: &CloudAgentClaim) -> Result<bool, String> {
        match self {
            Self::Memory(store) => store.renew_short_claim(claim).await,
            Self::Repository(store) => store.renew_short_claim(claim).await,
        }
    }

    async fn commit_transition(
        &self,
        transition: CloudAgentAtomicTransition,
    ) -> Result<bool, String> {
        match self {
            Self::Memory(store) => store.commit_transition(transition).await,
            Self::Repository(store) => store.commit_transition(transition).await,
        }
    }

    async fn release_short_claim(&self, claim: &CloudAgentClaim) -> Result<(), String> {
        match self {
            Self::Memory(store) => store.release_short_claim(claim).await,
            Self::Repository(store) => store.release_short_claim(claim).await,
        }
    }
}

#[cfg(test)]
mod tests;
