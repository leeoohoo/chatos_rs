// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_cloud_agent_protocol::CloudAgentRunRecord;
use mongodb::bson::{self, doc, DateTime};
use mongodb::options::{FindOneAndUpdateOptions, IndexOptions, ReturnDocument};
use mongodb::{Client, Collection, Database, IndexModel};
use serde::{Deserialize, Serialize};

use crate::{
    CloudAgentAtomicTransition, CloudAgentClaim, CloudAgentClaimResult, CloudAgentOutboxIntent,
    CloudAgentRunStore,
};

const RUN_COLLECTION: &str = "cloud_agent_runs";
const LANE_COLLECTION: &str = "cloud_agent_lanes";
const OUTBOX_COLLECTION: &str = "cloud_agent_outbox";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudAgentLaneRecord {
    #[serde(rename = "_id")]
    pub ordering_lane_key: String,
    pub next_lane_seq: u64,
    pub active_lane_seq: u64,
    pub version: u64,
    pub updated_at: DateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CloudAgentRunDocument {
    #[serde(rename = "_id")]
    id: String,
    record: CloudAgentRunRecord,
    #[serde(default)]
    claim_token: Option<String>,
    #[serde(default)]
    claim_until: Option<DateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CloudAgentOutboxDocument {
    #[serde(rename = "_id")]
    event_id: String,
    intent: CloudAgentOutboxIntent,
    available_at: DateTime,
    status: String,
    publish_attempts: u32,
    created_at: DateTime,
    updated_at: DateTime,
}

#[derive(Clone)]
pub struct MongoCloudAgentRunStore {
    client: Client,
    runs: Collection<CloudAgentRunDocument>,
    lanes: Collection<CloudAgentLaneRecord>,
    outbox: Collection<CloudAgentOutboxDocument>,
}

impl MongoCloudAgentRunStore {
    pub async fn connect(database_url: &str) -> Result<Self, String> {
        let client = Client::with_uri_str(database_url)
            .await
            .map_err(|error| format!("connect Cloud Agent MongoDB failed: {error}"))?;
        let database = client
            .default_database()
            .ok_or_else(|| "Cloud Agent database URL must include a database name".to_string())?;
        Self::from_database(client, database).await
    }

    pub async fn from_database(client: Client, database: Database) -> Result<Self, String> {
        let store = Self {
            client,
            runs: database.collection(RUN_COLLECTION),
            lanes: database.collection(LANE_COLLECTION),
            outbox: database.collection(OUTBOX_COLLECTION),
        };
        store.initialize_indexes().await?;
        Ok(store)
    }

    async fn initialize_indexes(&self) -> Result<(), String> {
        self.runs
            .create_index(
                IndexModel::builder()
                    .keys(doc! {
                        "record.ordering.ordering_lane_key": 1_i32,
                        "record.ordering.lane_seq": 1_i32,
                    })
                    .options(
                        IndexOptions::builder()
                            .unique(true)
                            .name("cloud_agent_lane_sequence_unique".to_string())
                            .build(),
                    )
                    .build(),
                None,
            )
            .await
            .map_err(|error| format!("create Cloud Agent lane sequence index failed: {error}"))?;
        self.outbox
            .create_index(
                IndexModel::builder()
                    .keys(doc! {
                        "status": 1_i32,
                        "available_at": 1_i32,
                    })
                    .options(
                        IndexOptions::builder()
                            .name("cloud_agent_outbox_available_at_ready".to_string())
                            .build(),
                    )
                    .build(),
                None,
            )
            .await
            .map_err(|error| format!("create Cloud Agent outbox index failed: {error}"))?;
        Ok(())
    }

    /// Allocates a monotonically increasing sequence for one business lane.
    /// Sequence 1 becomes active immediately; later sequences wait until the
    /// terminal transition advances the lane.
    pub async fn allocate_lane_seq(&self, ordering_lane_key: &str) -> Result<u64, String> {
        if ordering_lane_key.trim().is_empty() {
            return Err("ordering_lane_key must not be empty".to_string());
        }
        let updated = self
            .lanes
            .find_one_and_update(
                doc! { "_id": ordering_lane_key },
                vec![doc! {
                    "$set": {
                        "next_lane_seq": {
                            "$add": [{ "$ifNull": ["$next_lane_seq", 0_i64] }, 1_i64]
                        },
                        "active_lane_seq": {
                            "$ifNull": ["$active_lane_seq", 1_i64]
                        },
                        "version": {
                            "$add": [{ "$ifNull": ["$version", 0_i64] }, 1_i64]
                        },
                        "updated_at": DateTime::now(),
                    }
                }],
                FindOneAndUpdateOptions::builder()
                    .upsert(true)
                    .return_document(ReturnDocument::After)
                    .build(),
            )
            .await
            .map_err(|error| format!("allocate Cloud Agent lane sequence failed: {error}"))?
            .ok_or_else(|| "allocated Cloud Agent lane record is missing".to_string())?;
        Ok(updated.next_lane_seq)
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
        for intent in &outbox {
            intent.validate()?;
            if intent.ordering != record.ordering {
                return Err("initial outbox ordering does not match Cloud Agent run".to_string());
            }
        }
        let lane = self
            .lanes
            .find_one(
                doc! { "_id": record.ordering.ordering_lane_key.as_str() },
                None,
            )
            .await
            .map_err(|error| format!("load Cloud Agent lane before insert failed: {error}"))?
            .ok_or_else(|| "Cloud Agent lane must be allocated before run insert".to_string())?;
        if record.ordering.lane_seq == 0 || record.ordering.lane_seq > lane.next_lane_seq {
            return Err("Cloud Agent run lane_seq was not allocated by the lane store".to_string());
        }
        let mut session =
            self.client.start_session(None).await.map_err(|error| {
                format!("start Cloud Agent creation transaction failed: {error}")
            })?;
        session
            .start_transaction(None)
            .await
            .map_err(|error| format!("begin Cloud Agent creation transaction failed: {error}"))?;
        let result = async {
            self.runs
                .insert_one_with_session(
                    CloudAgentRunDocument {
                        id: record.ordering.agent_run_id.clone(),
                        record,
                        claim_token: None,
                        claim_until: None,
                    },
                    None,
                    &mut session,
                )
                .await
                .map_err(|error| format!("insert Cloud Agent run failed: {error}"))?;
            for intent in &outbox {
                self.outbox
                    .insert_one_with_session(
                        CloudAgentOutboxDocument {
                            event_id: intent.event_id.clone(),
                            intent: intent.clone(),
                            available_at: DateTime::from_millis(
                                intent.available_at.timestamp_millis(),
                            ),
                            status: "pending".to_string(),
                            publish_attempts: 0,
                            created_at: DateTime::now(),
                            updated_at: DateTime::now(),
                        },
                        None,
                        &mut session,
                    )
                    .await
                    .map_err(|error| {
                        format!("insert initial Cloud Agent outbox intent failed: {error}")
                    })?;
            }
            Ok::<(), String>(())
        }
        .await;
        match result {
            Ok(()) => session
                .commit_transaction()
                .await
                .map_err(|error| format!("commit Cloud Agent creation failed: {error}")),
            Err(error) => {
                let _ = session.abort_transaction().await;
                Err(error)
            }
        }
    }

    pub async fn advance_lane_after_terminal(
        &self,
        ordering_lane_key: &str,
        completed_lane_seq: u64,
    ) -> Result<Option<u64>, String> {
        let completed = i64::try_from(completed_lane_seq)
            .map_err(|_| "completed lane_seq exceeds MongoDB integer range".to_string())?;
        let next = completed
            .checked_add(1)
            .ok_or_else(|| "lane_seq overflow".to_string())?;
        let updated = self
            .lanes
            .find_one_and_update(
                doc! {
                    "_id": ordering_lane_key,
                    "active_lane_seq": completed,
                },
                vec![doc! {
                    "$set": {
                        "active_lane_seq": next,
                        "version": { "$add": ["$version", 1_i64] },
                        "updated_at": DateTime::now(),
                    }
                }],
                FindOneAndUpdateOptions::builder()
                    .return_document(ReturnDocument::After)
                    .build(),
            )
            .await
            .map_err(|error| format!("advance Cloud Agent lane failed: {error}"))?;
        Ok(updated.and_then(|lane| {
            (lane.active_lane_seq == completed_lane_seq.saturating_add(1))
                .then_some(lane.active_lane_seq)
        }))
    }

    pub async fn list_ready_outbox(
        &self,
        limit: i64,
    ) -> Result<Vec<CloudAgentOutboxIntent>, String> {
        use futures_util::TryStreamExt;

        self.outbox
            .find(
                doc! {
                    "status": "pending",
                    "available_at": { "$lte": DateTime::now() },
                },
                mongodb::options::FindOptions::builder()
                    .sort(doc! { "intent.available_at": 1_i32, "_id": 1_i32 })
                    .limit(limit.max(1))
                    .build(),
            )
            .await
            .map_err(|error| format!("load Cloud Agent outbox failed: {error}"))?
            .map_ok(|document| document.intent)
            .try_collect()
            .await
            .map_err(|error| format!("read Cloud Agent outbox failed: {error}"))
    }

    pub async fn mark_outbox_published(&self, event_id: &str) -> Result<bool, String> {
        self.outbox
            .update_one(
                doc! { "_id": event_id, "status": "pending" },
                doc! {
                    "$set": { "status": "published", "updated_at": DateTime::now() },
                    "$inc": { "publish_attempts": 1_i32 },
                },
                None,
            )
            .await
            .map(|result| result.modified_count == 1)
            .map_err(|error| format!("mark Cloud Agent outbox published failed: {error}"))
    }
}

#[async_trait]
impl CloudAgentRunStore for MongoCloudAgentRunStore {
    async fn load_run(&self, agent_run_id: &str) -> Result<Option<CloudAgentRunRecord>, String> {
        self.runs
            .find_one(doc! { "_id": agent_run_id }, None)
            .await
            .map(|document| document.map(|document| document.record))
            .map_err(|error| format!("load Cloud Agent run failed: {error}"))
    }

    async fn acquire_short_claim(
        &self,
        claim: &CloudAgentClaim,
    ) -> Result<CloudAgentClaimResult, String> {
        claim.validate()?;
        let lane = self
            .lanes
            .find_one(
                doc! { "_id": claim.ordering.ordering_lane_key.as_str() },
                None,
            )
            .await
            .map_err(|error| format!("load Cloud Agent lane for claim failed: {error}"))?;
        let Some(lane) = lane else {
            return Ok(CloudAgentClaimResult::OutOfOrder);
        };
        if lane.active_lane_seq != claim.ordering.lane_seq {
            return Ok(CloudAgentClaimResult::OutOfOrder);
        }
        let now = DateTime::now();
        let record_updated_at = chrono::Utc::now().to_rfc3339();
        let claim_until = DateTime::from_millis(claim.claim_until.timestamp_millis());
        let updated = self
            .runs
            .find_one_and_update(
                doc! {
                    "_id": claim.ordering.agent_run_id.as_str(),
                    "record.ordering.ordering_lane_key": claim.ordering.ordering_lane_key.as_str(),
                    "record.ordering.lane_seq": i64::try_from(claim.ordering.lane_seq).unwrap_or(i64::MAX),
                    "record.ordering.generation": i64::try_from(claim.ordering.generation).unwrap_or(i64::MAX),
                    "record.ordering.step_seq": i64::try_from(claim.ordering.step_seq).unwrap_or(i64::MAX),
                    "record.status": bson::to_bson(&claim.expected_status).map_err(|error| error.to_string())?,
                    "record.phase": bson::to_bson(&claim.expected_phase).map_err(|error| error.to_string())?,
                    "record.version": i64::try_from(claim.expected_version).unwrap_or(i64::MAX),
                    "$or": [
                        { "claim_token": bson::Bson::Null },
                        { "claim_token": { "$exists": false } },
                        { "claim_until": { "$lte": now } },
                        { "claim_token": claim.claim_token.as_str() },
                    ],
                },
                doc! {
                    "$set": {
                        "claim_token": claim.claim_token.as_str(),
                        "claim_until": claim_until,
                        "record.updated_at": record_updated_at,
                    }
                },
                FindOneAndUpdateOptions::builder()
                    .return_document(ReturnDocument::After)
                    .build(),
            )
            .await
            .map_err(|error| format!("acquire Cloud Agent short claim failed: {error}"))?;
        if updated.is_some() {
            return Ok(CloudAgentClaimResult::Acquired);
        }
        let Some(run) = self.load_run(claim.ordering.agent_run_id.as_str()).await? else {
            return Ok(CloudAgentClaimResult::Conflict);
        };
        if run.status.is_terminal() {
            Ok(CloudAgentClaimResult::Terminal)
        } else if run.ordering.generation > claim.ordering.generation
            || run.ordering.step_seq > claim.ordering.step_seq
            || run.version > claim.expected_version
        {
            Ok(CloudAgentClaimResult::Duplicate)
        } else if run.ordering.generation < claim.ordering.generation
            || run.ordering.step_seq < claim.ordering.step_seq
        {
            Ok(CloudAgentClaimResult::OutOfOrder)
        } else {
            Ok(CloudAgentClaimResult::Conflict)
        }
    }

    async fn commit_transition(
        &self,
        transition: CloudAgentAtomicTransition,
    ) -> Result<bool, String> {
        transition.validate()?;
        let mut session = self
            .client
            .start_session(None)
            .await
            .map_err(|error| format!("start Cloud Agent transaction failed: {error}"))?;
        session
            .start_transaction(None)
            .await
            .map_err(|error| format!("begin Cloud Agent transaction failed: {error}"))?;
        let result = self
            .commit_transition_in_session(&mut session, &transition)
            .await;
        match result {
            Ok(updated) => {
                session
                    .commit_transaction()
                    .await
                    .map_err(|error| format!("commit Cloud Agent transaction failed: {error}"))?;
                Ok(updated)
            }
            Err(error) => {
                let _ = session.abort_transaction().await;
                Err(error)
            }
        }
    }

    async fn release_short_claim(&self, claim: &CloudAgentClaim) -> Result<(), String> {
        self.runs
            .update_one(
                doc! {
                    "_id": claim.ordering.agent_run_id.as_str(),
                    "claim_token": claim.claim_token.as_str(),
                },
                doc! {
                    "$set": {
                        "claim_token": bson::Bson::Null,
                        "claim_until": bson::Bson::Null,
                        "record.updated_at": chrono::Utc::now().to_rfc3339(),
                    }
                },
                None,
            )
            .await
            .map(|_| ())
            .map_err(|error| format!("release Cloud Agent short claim failed: {error}"))
    }
}

impl MongoCloudAgentRunStore {
    async fn commit_transition_in_session(
        &self,
        session: &mut mongodb::ClientSession,
        transition: &CloudAgentAtomicTransition,
    ) -> Result<bool, String> {
        let claim = &transition.claim;
        let status = bson::to_bson(&transition.next_status).map_err(|error| error.to_string())?;
        let phase = bson::to_bson(&transition.next_phase).map_err(|error| error.to_string())?;
        let result = self
            .runs
            .update_one_with_session(
                doc! {
                    "_id": claim.ordering.agent_run_id.as_str(),
                    "record.ordering.ordering_lane_key": claim.ordering.ordering_lane_key.as_str(),
                    "record.ordering.lane_seq": i64::try_from(claim.ordering.lane_seq).unwrap_or(i64::MAX),
                    "record.ordering.generation": i64::try_from(claim.ordering.generation).unwrap_or(i64::MAX),
                    "record.ordering.step_seq": i64::try_from(claim.ordering.step_seq).unwrap_or(i64::MAX),
                    "record.status": bson::to_bson(&claim.expected_status).map_err(|error| error.to_string())?,
                    "record.phase": bson::to_bson(&claim.expected_phase).map_err(|error| error.to_string())?,
                    "record.version": i64::try_from(claim.expected_version).unwrap_or(i64::MAX),
                    "claim_token": claim.claim_token.as_str(),
                },
                doc! {
                    "$set": {
                        "record.input": bson::to_bson(&transition.next_input).map_err(|error| error.to_string())?,
                        "record.status": status,
                        "record.phase": phase,
                        "record.ordering.step_seq": i64::try_from(transition.next_step_seq).unwrap_or(i64::MAX),
                        "record.iteration": i64::from(transition.next_iteration),
                        "record.retry_count": i64::from(transition.next_retry_count),
                        "record.previous_response_id": bson::to_bson(&transition.previous_response_id).map_err(|error| error.to_string())?,
                        "record.continuation_mode": bson::to_bson(&transition.continuation_mode).map_err(|error| error.to_string())?,
                        "record.current_input_items_ref": transition.current_input_items_ref.as_str(),
                        "record.mcp_runtime_session_ref": bson::to_bson(&transition.mcp_runtime_session_ref).map_err(|error| error.to_string())?,
                        "record.pending_batch_id": bson::to_bson(&transition.pending_batch_id).map_err(|error| error.to_string())?,
                        "record.pending_tool_calls": bson::to_bson(&transition.pending_tool_calls).map_err(|error| error.to_string())?,
                        "record.pending_tool_results": bson::to_bson(&transition.pending_tool_results).map_err(|error| error.to_string())?,
                        "record.response_input_items": bson::to_bson(&transition.response_input_items).map_err(|error| error.to_string())?,
                        "record.terminal_outcome": bson::to_bson(&transition.terminal_outcome).map_err(|error| error.to_string())?,
                        "record.updated_at": chrono::Utc::now().to_rfc3339(),
                        "claim_token": bson::Bson::Null,
                        "claim_until": bson::Bson::Null,
                    },
                    "$inc": { "record.version": 1_i64 },
                },
                None,
                session,
            )
            .await
            .map_err(|error| format!("persist Cloud Agent transition failed: {error}"))?;
        if result.modified_count != 1 {
            return Ok(false);
        }
        for intent in &transition.outbox {
            self.outbox
                .update_one_with_session(
                    doc! { "_id": intent.event_id.as_str() },
                    doc! {
                        "$setOnInsert": {
                            "intent": bson::to_bson(intent).map_err(|error| error.to_string())?,
                            "available_at": DateTime::from_millis(intent.available_at.timestamp_millis()),
                            "status": "pending",
                            "publish_attempts": 0_i32,
                            "created_at": DateTime::now(),
                            "updated_at": DateTime::now(),
                        }
                    },
                    mongodb::options::UpdateOptions::builder()
                        .upsert(true)
                        .build(),
                    session,
                )
                .await
                .map_err(|error| format!("persist Cloud Agent outbox intent failed: {error}"))?;
        }
        if transition.next_status.is_terminal() {
            let completed = i64::try_from(claim.ordering.lane_seq)
                .map_err(|_| "completed lane_seq exceeds MongoDB integer range".to_string())?;
            let next = completed
                .checked_add(1)
                .ok_or_else(|| "lane_seq overflow".to_string())?;
            let lane_result = self
                .lanes
                .update_one_with_session(
                    doc! {
                        "_id": claim.ordering.ordering_lane_key.as_str(),
                        "active_lane_seq": completed,
                    },
                    doc! {
                        "$set": {
                            "active_lane_seq": next,
                            "updated_at": DateTime::now(),
                        },
                        "$inc": { "version": 1_i64 },
                    },
                    None,
                    session,
                )
                .await
                .map_err(|error| format!("advance terminal Cloud Agent lane failed: {error}"))?;
            if lane_result.modified_count != 1 {
                return Err("claimed Cloud Agent lane changed before terminal commit".to_string());
            }
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_cloud_agent_protocol::{CloudAgentRunPhase, CloudAgentRunStatus};

    #[test]
    fn collection_names_are_stable() {
        assert_eq!(RUN_COLLECTION, "cloud_agent_runs");
        assert_eq!(LANE_COLLECTION, "cloud_agent_lanes");
        assert_eq!(OUTBOX_COLLECTION, "cloud_agent_outbox");
    }

    #[test]
    fn terminal_status_serializes_for_cas_filters() {
        assert_eq!(
            bson::to_bson(&CloudAgentRunStatus::WaitingToolResult).unwrap(),
            bson::Bson::String("waiting_tool_result".to_string())
        );
        assert_eq!(
            bson::to_bson(&CloudAgentRunPhase::ToolBatch).unwrap(),
            bson::Bson::String("tool_batch".to_string())
        );
    }

    #[test]
    fn outbox_available_at_is_stored_as_mongodb_datetime() {
        let available_at = chrono::Utc::now();
        let document = CloudAgentOutboxDocument {
            event_id: "event-1".to_string(),
            intent: CloudAgentOutboxIntent {
                event_id: "event-1".to_string(),
                topic: "run_started".to_string(),
                routing_key: "cloud_agent.test.runtime".to_string(),
                ordering: chatos_cloud_agent_protocol::CloudAgentOrdering {
                    ordering_lane_key: "lane-1".to_string(),
                    lane_seq: 1,
                    agent_run_id: "run-1".to_string(),
                    generation: 1,
                    step_seq: 1,
                },
                causation_id: "cause-1".to_string(),
                correlation_id: "correlation-1".to_string(),
                available_at,
                payload: serde_json::json!({}),
            },
            available_at: DateTime::from_millis(available_at.timestamp_millis()),
            status: "pending".to_string(),
            publish_attempts: 0,
            created_at: DateTime::now(),
            updated_at: DateTime::now(),
        };

        let serialized = bson::to_document(&document).unwrap();
        assert!(matches!(
            serialized.get("available_at"),
            Some(bson::Bson::DateTime(_))
        ));
    }
}
