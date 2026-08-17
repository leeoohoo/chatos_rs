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
}

impl InMemoryCloudAgentOutboxRecord {
    fn pending(intent: CloudAgentOutboxIntent) -> Self {
        Self {
            available_at: intent.available_at,
            intent,
            publish_attempts: 0,
            last_error: None,
            dead_lettered: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudAgentOutboxPublishFailure {
    pub publish_attempts: u32,
    pub dead_lettered: bool,
    pub available_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub(crate) struct CloudAgentPendingOutboxIntent {
    pub intent: CloudAgentOutboxIntent,
    pub publish_attempts: u32,
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
        record.validate()?;
        validate_initial_outbox(&record, &outbox)?;
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

    pub async fn connect_to_database(
        database_url: &str,
        database_name: &str,
    ) -> Result<Self, String> {
        let client = mongodb::Client::with_uri_str(database_url)
            .await
            .map_err(|error| format!("connect Cloud Agent MongoDB failed: {error}"))?;
        let database_name = database_name.trim();
        if database_name.is_empty() {
            return Err("Cloud Agent database name must not be empty".to_string());
        }
        let database = client.database(database_name);
        Ok(Self::Mongo(
            MongoCloudAgentRunStore::from_database(client, database).await?,
        ))
    }

    pub async fn from_mongodb_database(
        client: mongodb::Client,
        database: mongodb::Database,
    ) -> Result<Self, String> {
        Ok(Self::Mongo(
            MongoCloudAgentRunStore::from_database(client, database).await?,
        ))
    }

    pub async fn allocate_lane_seq(&self, ordering_lane_key: &str) -> Result<u64, String> {
        match self {
            Self::Memory(store) => store.allocate_lane_seq(ordering_lane_key).await,
            Self::Mongo(store) => store.allocate_lane_seq(ordering_lane_key).await,
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
            Self::Mongo(store) => store.insert_run_with_outbox(record, outbox).await,
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

    pub(crate) async fn list_ready_outbox_with_attempts(
        &self,
        limit: i64,
    ) -> Result<Vec<CloudAgentPendingOutboxIntent>, String> {
        match self {
            Self::Memory(store) => store.list_ready_outbox_with_attempts(limit).await,
            Self::Mongo(store) => store.list_ready_outbox_with_attempts(limit).await,
        }
    }

    pub async fn mark_outbox_published(&self, event_id: &str) -> Result<bool, String> {
        match self {
            Self::Memory(store) => store.mark_outbox_published(event_id).await,
            Self::Mongo(store) => store.mark_outbox_published(event_id).await,
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
            Self::Mongo(store) => {
                store
                    .mark_outbox_publish_failed(event_id, error, next_available_at, max_attempts)
                    .await
            }
        }
    }
}

pub(super) fn bounded_outbox_publish_error(error: &str) -> String {
    const MAX_ERROR_CHARS: usize = 2_000;
    error.chars().take(MAX_ERROR_CHARS).collect()
}

fn validate_initial_outbox(
    record: &CloudAgentRunRecord,
    outbox: &[CloudAgentOutboxIntent],
) -> Result<(), String> {
    for intent in outbox {
        intent.validate()?;
        if intent.ordering != record.ordering {
            return Err("initial outbox ordering does not match Cloud Agent run".to_string());
        }
    }
    Ok(())
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

    async fn renew_short_claim(&self, claim: &CloudAgentClaim) -> Result<bool, String> {
        match self {
            Self::Memory(store) => store.renew_short_claim(claim).await,
            Self::Mongo(store) => store.renew_short_claim(claim).await,
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

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_cloud_agent_protocol::{
        CloudAgentOrdering, CloudAgentRunPhase, CloudAgentRunStatus,
    };
    use chrono::Utc;
    use serde_json::Value;

    fn run_record(run_id: &str, lane_seq: u64) -> CloudAgentRunRecord {
        let now = Utc::now();
        CloudAgentRunRecord {
            ordering: CloudAgentOrdering {
                ordering_lane_key: "task:task-1".to_string(),
                lane_seq,
                agent_run_id: run_id.to_string(),
                generation: 1,
                step_seq: 1,
            },
            owner_service: "task-runner".to_string(),
            owner_entity_type: "task_run".to_string(),
            owner_entity_id: run_id.to_string(),
            owner_user_id: "user-1".to_string(),
            agent_key: "task_runner_run_phase".to_string(),
            input: Value::Null,
            status: CloudAgentRunStatus::ModelReady,
            phase: CloudAgentRunPhase::Ready,
            iteration: 0,
            model_config_ref: "model-1".to_string(),
            model_runtime_snapshot_ref: "snapshot-1".to_string(),
            agent_prompt_revision: "1".to_string(),
            agent_prompt_checksum: "checksum-1".to_string(),
            capability_policy_revision: "policy-1".to_string(),
            mcp_runtime_session_ref: None,
            previous_response_id: None,
            continuation_mode: None,
            pending_batch_id: None,
            pending_tool_calls: Vec::new(),
            pending_tool_results: Vec::new(),
            response_input_items: Vec::new(),
            current_input_items_ref: format!("task_run:{run_id}:input"),
            usage_accumulator: Value::Null,
            max_iterations: 10,
            retry_count: 0,
            deadline_at: None,
            cancel_requested: false,
            terminal_outcome: None,
            version: 1,
            created_at: now,
            updated_at: now,
        }
    }

    fn outbox_intent(run: &CloudAgentRunRecord) -> CloudAgentOutboxIntent {
        CloudAgentOutboxIntent {
            event_id: format!("{}:event", run.ordering.agent_run_id),
            topic: "run_started".to_string(),
            routing_key: "cloud_agent.test.runtime".to_string(),
            ordering: run.ordering.clone(),
            causation_id: "cause-1".to_string(),
            correlation_id: "correlation-1".to_string(),
            available_at: Utc::now() - chrono::Duration::seconds(1),
            payload: serde_json::json!({}),
        }
    }

    #[tokio::test]
    async fn terminal_commit_advances_an_empty_lane_for_the_next_future_run() {
        let store = InMemoryCloudAgentRunStore::new();
        let first_seq = store.allocate_lane_seq("task:task-1").await.unwrap();
        let first = run_record("run-1", first_seq);
        store.insert_run(first.clone()).await.unwrap();
        let claim = CloudAgentClaim {
            ordering: first.ordering.clone(),
            expected_status: first.status,
            expected_phase: first.phase,
            expected_version: first.version,
            claim_token: "claim-1".to_string(),
            claim_until: Utc::now() + chrono::Duration::seconds(30),
        };
        assert_eq!(
            store.acquire_short_claim(&claim).await.unwrap(),
            CloudAgentClaimResult::Acquired
        );
        assert!(store
            .commit_transition(CloudAgentAtomicTransition {
                claim,
                next_input: Value::Null,
                next_status: CloudAgentRunStatus::Succeeded,
                next_phase: CloudAgentRunPhase::Terminal,
                next_step_seq: 2,
                next_iteration: 1,
                next_retry_count: 0,
                previous_response_id: None,
                continuation_mode: None,
                current_input_items_ref: "task_run:run-1:terminal".to_string(),
                mcp_runtime_session_ref: None,
                pending_batch_id: None,
                pending_tool_calls: Vec::new(),
                pending_tool_results: Vec::new(),
                response_input_items: Vec::new(),
                terminal_outcome: Some(serde_json::json!({"ok": true})),
                outbox: Vec::new(),
            })
            .await
            .unwrap());

        let second_seq = store.allocate_lane_seq("task:task-1").await.unwrap();
        assert_eq!(second_seq, 2);
        let second = run_record("run-2", second_seq);
        store.insert_run(second.clone()).await.unwrap();
        let second_claim = CloudAgentClaim {
            ordering: second.ordering.clone(),
            expected_status: second.status,
            expected_phase: second.phase,
            expected_version: second.version,
            claim_token: "claim-2".to_string(),
            claim_until: Utc::now() + chrono::Duration::seconds(30),
        };
        assert_eq!(
            store.acquire_short_claim(&second_claim).await.unwrap(),
            CloudAgentClaimResult::Acquired
        );
    }

    #[tokio::test]
    async fn outbox_publish_failures_back_off_and_eventually_dead_letter() {
        let store = InMemoryCloudAgentRunStore::new();
        let lane_seq = store.allocate_lane_seq("task:task-1").await.unwrap();
        let run = run_record("run-outbox", lane_seq);
        let intent = outbox_intent(&run);
        store
            .insert_run_with_outbox(run, vec![intent.clone()])
            .await
            .unwrap();

        assert_eq!(store.list_ready_outbox(10).await.unwrap().len(), 1);
        let retry_at = Utc::now() + chrono::Duration::minutes(1);
        let first = store
            .mark_outbox_publish_failed(intent.event_id.as_str(), "publish failed", retry_at, 8)
            .await
            .unwrap()
            .expect("pending outbox failure");
        assert_eq!(first.publish_attempts, 1);
        assert!(!first.dead_lettered);
        assert!(store.list_ready_outbox(10).await.unwrap().is_empty());

        let mut latest = first;
        for _ in 2..=8 {
            latest = store
                .mark_outbox_publish_failed(
                    intent.event_id.as_str(),
                    "publish failed again",
                    Utc::now() - chrono::Duration::seconds(1),
                    8,
                )
                .await
                .unwrap()
                .expect("pending outbox failure");
        }
        assert_eq!(latest.publish_attempts, 8);
        assert!(latest.dead_lettered);
        assert!(store.list_ready_outbox(10).await.unwrap().is_empty());
    }

    #[test]
    fn outbox_publish_errors_are_bounded() {
        assert_eq!(
            bounded_outbox_publish_error(&"x".repeat(3_000)).len(),
            2_000
        );
    }
}
