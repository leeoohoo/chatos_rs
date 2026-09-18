// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_cloud_agent_protocol::CloudAgentRunRecord;
use chatos_cloud_agent_runtime::{
    apply_cloud_agent_transition, bounded_cloud_agent_outbox_error, classify_cloud_agent_claim,
    cloud_agent_outbox_failure, validate_initial_cloud_agent_state, CloudAgentAtomicTransition,
    CloudAgentClaim, CloudAgentClaimResult, CloudAgentOutboxIntent, CloudAgentOutboxPublishFailure,
    CloudAgentPendingOutboxIntent, CloudAgentRunStore, CloudAgentStateRepository,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::types::Json;

use super::db::{db_error, json};

#[derive(Clone)]
pub struct CloudAgentPostgresStore {
    pool: chatos_postgres::PgPool,
}

impl CloudAgentPostgresStore {
    pub fn new(pool: chatos_postgres::PgPool) -> Self {
        Self { pool }
    }
    async fn insert_outbox<'e, E>(
        executor: E,
        intent: &CloudAgentOutboxIntent,
    ) -> Result<(), String>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        sqlx::query("INSERT INTO cloud_agent_outbox(event_id,agent_run_id,status,available_at,publish_attempts,created_at,updated_at,data) VALUES($1,$2,'pending',$3,0,now(),now(),$4) ON CONFLICT(event_id) DO NOTHING").bind(&intent.event_id).bind(&intent.ordering.agent_run_id).bind(intent.available_at).bind(json(intent)?).execute(executor).await.map(|_|()).map_err(db_error)
    }
}

fn i64_value(value: u64, name: &str) -> Result<i64, String> {
    i64::try_from(value).map_err(|_| format!("{name} exceeds PostgreSQL BIGINT range"))
}
fn u64_value(value: i64, name: &str) -> Result<u64, String> {
    u64::try_from(value).map_err(|_| format!("stored {name} is negative"))
}
fn enum_text<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_value(value)
        .map_err(|e| e.to_string())?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "enum is not text".to_string())
}
fn decode_run(value: Json<serde_json::Value>) -> Result<CloudAgentRunRecord, String> {
    serde_json::from_value(value.0).map_err(|e| e.to_string())
}

async fn load_stored_run(
    pool: &chatos_postgres::PgPool,
    id: &str,
) -> Result<Option<CloudAgentRunRecord>, String> {
    sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM cloud_agent_runs WHERE agent_run_id=$1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(db_error)?
    .map(decode_run)
    .transpose()
}

#[async_trait]
impl CloudAgentStateRepository for CloudAgentPostgresStore {
    async fn allocate_lane_seq(&self, key: &str) -> Result<u64, String> {
        if key.trim().is_empty() {
            return Err("ordering_lane_key must not be empty".to_string());
        }
        let value=sqlx::query_scalar::<_,i64>("INSERT INTO cloud_agent_lanes(ordering_lane_key,next_lane_seq,active_lane_seq,version,updated_at) VALUES($1,1,1,1,now()) ON CONFLICT(ordering_lane_key) DO UPDATE SET next_lane_seq=cloud_agent_lanes.next_lane_seq+1,version=cloud_agent_lanes.version+1,updated_at=now() RETURNING next_lane_seq").bind(key).fetch_one(&self.pool).await.map_err(db_error)?;
        u64_value(value, "next_lane_seq")
    }
    async fn insert_run_with_outbox(
        &self,
        record: CloudAgentRunRecord,
        outbox: Vec<CloudAgentOutboxIntent>,
    ) -> Result<(), String> {
        validate_initial_cloud_agent_state(&record, &outbox)?;
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        let allocated = sqlx::query_scalar::<_, i64>(
            "SELECT next_lane_seq FROM cloud_agent_lanes WHERE ordering_lane_key=$1 FOR UPDATE",
        )
        .bind(&record.ordering.ordering_lane_key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_error)?
        .ok_or_else(|| "Cloud Agent lane must be allocated before run insert".to_string())?;
        let lane_seq = i64_value(record.ordering.lane_seq, "lane_seq")?;
        if lane_seq == 0 || lane_seq > allocated {
            return Err("Cloud Agent run lane_seq was not allocated by the lane store".to_string());
        }
        sqlx::query("INSERT INTO cloud_agent_runs(agent_run_id,ordering_lane_key,lane_seq,generation,step_seq,status,phase,version,updated_at,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(&record.ordering.agent_run_id).bind(&record.ordering.ordering_lane_key).bind(lane_seq).bind(i64_value(record.ordering.generation,"generation")?).bind(i64_value(record.ordering.step_seq,"step_seq")?).bind(enum_text(&record.status)?).bind(enum_text(&record.phase)?).bind(i64_value(record.version,"version")?).bind(record.updated_at).bind(json(&record)?).execute(&mut *tx).await.map_err(db_error)?;
        for intent in &outbox {
            Self::insert_outbox(&mut *tx, intent).await?
        }
        tx.commit().await.map_err(db_error)
    }
    async fn advance_lane_after_terminal(
        &self,
        key: &str,
        seq: u64,
    ) -> Result<Option<u64>, String> {
        let next = i64_value(
            seq.checked_add(1)
                .ok_or_else(|| "lane_seq overflow".to_string())?,
            "lane_seq",
        )?;
        let value=sqlx::query_scalar::<_,i64>("UPDATE cloud_agent_lanes SET active_lane_seq=$1,version=version+1,updated_at=now() WHERE ordering_lane_key=$2 AND active_lane_seq=$3 RETURNING active_lane_seq").bind(next).bind(key).bind(i64_value(seq,"lane_seq")?).fetch_optional(&self.pool).await.map_err(db_error)?;
        value.map(|v| u64_value(v, "active_lane_seq")).transpose()
    }
    async fn claim_ready_outbox_with_attempts(
        &self,
        limit: i64,
        claim_token: &str,
        claim_until: DateTime<Utc>,
    ) -> Result<Vec<CloudAgentPendingOutboxIntent>, String> {
        if claim_token.trim().is_empty() {
            return Err("Cloud Agent outbox claim token must not be empty".to_string());
        }
        let rows=sqlx::query_as::<_,(Json<serde_json::Value>,i32)>("WITH candidates AS (SELECT event_id FROM cloud_agent_outbox WHERE (status='pending' AND available_at<=now()) OR (status='publishing' AND claim_until<=now()) ORDER BY available_at,event_id LIMIT $1 FOR UPDATE SKIP LOCKED) UPDATE cloud_agent_outbox o SET status='publishing',claim_token=$2,claim_until=$3,updated_at=now() FROM candidates c WHERE o.event_id=c.event_id RETURNING o.data,o.publish_attempts").bind(limit.max(1)).bind(claim_token).bind(claim_until).fetch_all(&self.pool).await.map_err(db_error)?;
        rows.into_iter()
            .map(|(value, attempts)| {
                Ok(CloudAgentPendingOutboxIntent {
                    intent: serde_json::from_value(value.0).map_err(|e| e.to_string())?,
                    publish_attempts: u32::try_from(attempts).unwrap_or(u32::MAX),
                })
            })
            .collect()
    }
    async fn mark_claimed_outbox_published(
        &self,
        id: &str,
        claim_token: &str,
    ) -> Result<bool, String> {
        sqlx::query("UPDATE cloud_agent_outbox SET status='published',publish_attempts=publish_attempts+1,claim_token=NULL,claim_until=NULL,updated_at=now() WHERE event_id=$1 AND status='publishing' AND claim_token=$2").bind(id).bind(claim_token).execute(&self.pool).await.map(|r|r.rows_affected()==1).map_err(db_error)
    }
    async fn mark_claimed_outbox_publish_failed(
        &self,
        id: &str,
        claim_token: &str,
        error: &str,
        next: DateTime<Utc>,
        max: u32,
    ) -> Result<Option<CloudAgentOutboxPublishFailure>, String> {
        let bounded = bounded_cloud_agent_outbox_error(error);
        let row=sqlx::query_as::<_,(i32,String)>("UPDATE cloud_agent_outbox SET publish_attempts=publish_attempts+1,last_error=$1,available_at=$2,status=CASE WHEN publish_attempts+1>=$3 THEN 'dead_lettered' ELSE 'pending' END,claim_token=NULL,claim_until=NULL,updated_at=now() WHERE event_id=$4 AND status='publishing' AND claim_token=$5 RETURNING publish_attempts,status").bind(bounded).bind(next).bind(i32::try_from(max.max(1)).unwrap_or(i32::MAX)).bind(id).bind(claim_token).fetch_optional(&self.pool).await.map_err(db_error)?;
        Ok(cloud_agent_outbox_failure(row, next))
    }
}

#[async_trait]
impl CloudAgentRunStore for CloudAgentPostgresStore {
    async fn load_run(&self, id: &str) -> Result<Option<CloudAgentRunRecord>, String> {
        load_stored_run(&self.pool, id).await
    }
    async fn acquire_short_claim(
        &self,
        claim: &CloudAgentClaim,
    ) -> Result<CloudAgentClaimResult, String> {
        claim.validate()?;
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        let active = sqlx::query_scalar::<_, i64>(
            "SELECT active_lane_seq FROM cloud_agent_lanes WHERE ordering_lane_key=$1 FOR UPDATE",
        )
        .bind(&claim.ordering.ordering_lane_key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_error)?;
        if active != Some(i64_value(claim.ordering.lane_seq, "lane_seq")?) {
            tx.rollback().await.map_err(db_error)?;
            return Ok(CloudAgentClaimResult::OutOfOrder);
        }
        let row=sqlx::query_as::<_,(Json<serde_json::Value>,Option<String>,Option<DateTime<Utc>>)>("SELECT data,claim_token,claim_until FROM cloud_agent_runs WHERE agent_run_id=$1 FOR UPDATE").bind(&claim.ordering.agent_run_id).fetch_optional(&mut *tx).await.map_err(db_error)?;
        let Some((value, token, until)) = row else {
            tx.rollback().await.map_err(db_error)?;
            return Ok(CloudAgentClaimResult::Conflict);
        };
        let run = decode_run(value)?;
        let result = classify_cloud_agent_claim(&run, claim, token.as_deref(), until, Utc::now());
        if result == CloudAgentClaimResult::Acquired {
            sqlx::query(
                "UPDATE cloud_agent_runs SET claim_token=$1,claim_until=$2 WHERE agent_run_id=$3",
            )
            .bind(&claim.claim_token)
            .bind(claim.claim_until)
            .bind(&claim.ordering.agent_run_id)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        }
        tx.commit().await.map_err(db_error)?;
        Ok(result)
    }
    async fn renew_short_claim(&self, claim: &CloudAgentClaim) -> Result<bool, String> {
        claim.validate()?;
        let result=sqlx::query("UPDATE cloud_agent_runs SET claim_until=$1 WHERE agent_run_id=$2 AND claim_token=$3 AND ordering_lane_key=$4 AND lane_seq=$5 AND generation=$6 AND step_seq=$7 AND status=$8 AND phase=$9 AND version=$10").bind(claim.claim_until).bind(&claim.ordering.agent_run_id).bind(&claim.claim_token).bind(&claim.ordering.ordering_lane_key).bind(i64_value(claim.ordering.lane_seq,"lane_seq")?).bind(i64_value(claim.ordering.generation,"generation")?).bind(i64_value(claim.ordering.step_seq,"step_seq")?).bind(enum_text(&claim.expected_status)?).bind(enum_text(&claim.expected_phase)?).bind(i64_value(claim.expected_version,"version")?).execute(&self.pool).await.map_err(db_error)?;
        Ok(result.rows_affected() == 1)
    }
    async fn commit_transition(
        &self,
        transition: CloudAgentAtomicTransition,
    ) -> Result<bool, String> {
        transition.validate()?;
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        let claim = transition.claim.clone();
        let row = sqlx::query_as::<_, (Json<serde_json::Value>, Option<String>)>(
            "SELECT data,claim_token FROM cloud_agent_runs WHERE agent_run_id=$1 FOR UPDATE",
        )
        .bind(&claim.ordering.agent_run_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_error)?;
        let Some((value, token)) = row else {
            tx.rollback().await.map_err(db_error)?;
            return Ok(false);
        };
        let mut run = decode_run(value)?;
        let Some(outbox) =
            apply_cloud_agent_transition(&mut run, transition, token.as_deref(), Utc::now())?
        else {
            tx.rollback().await.map_err(db_error)?;
            return Ok(false);
        };
        sqlx::query("UPDATE cloud_agent_runs SET step_seq=$1,status=$2,phase=$3,version=$4,claim_token=NULL,claim_until=NULL,updated_at=$5,data=$6 WHERE agent_run_id=$7").bind(i64_value(run.ordering.step_seq,"step_seq")?).bind(enum_text(&run.status)?).bind(enum_text(&run.phase)?).bind(i64_value(run.version,"version")?).bind(run.updated_at).bind(json(&run)?).bind(&run.ordering.agent_run_id).execute(&mut *tx).await.map_err(db_error)?;
        for intent in &outbox {
            Self::insert_outbox(&mut *tx, intent).await?
        }
        if run.status.is_terminal() {
            let advanced=sqlx::query("UPDATE cloud_agent_lanes SET active_lane_seq=active_lane_seq+1,version=version+1,updated_at=now() WHERE ordering_lane_key=$1 AND active_lane_seq=$2").bind(&run.ordering.ordering_lane_key).bind(i64_value(run.ordering.lane_seq,"lane_seq")?).execute(&mut *tx).await.map_err(db_error)?;
            if advanced.rows_affected() != 1 {
                return Err("claimed Cloud Agent lane changed before terminal commit".to_string());
            }
        }
        tx.commit().await.map_err(db_error)?;
        Ok(true)
    }
    async fn release_short_claim(&self, claim: &CloudAgentClaim) -> Result<(), String> {
        sqlx::query("UPDATE cloud_agent_runs SET claim_token=NULL,claim_until=NULL WHERE agent_run_id=$1 AND claim_token=$2").bind(&claim.ordering.agent_run_id).bind(&claim.claim_token).execute(&self.pool).await.map(|_|()).map_err(db_error)
    }
}

#[cfg(test)]
mod postgres_contract_tests {
    use super::*;
    use chatos_cloud_agent_protocol::{
        CloudAgentOrdering, CloudAgentRunPhase, CloudAgentRunStatus,
    };
    use serde_json::Value;

    fn run_record(lane_key: &str, run_id: &str, lane_seq: u64) -> CloudAgentRunRecord {
        let now = Utc::now();
        CloudAgentRunRecord {
            ordering: CloudAgentOrdering {
                ordering_lane_key: lane_key.to_string(),
                lane_seq,
                agent_run_id: run_id.to_string(),
                generation: 1,
                step_seq: 1,
            },
            owner_service: "chatos".to_string(),
            owner_entity_type: "conversation_turn".to_string(),
            owner_entity_id: run_id.to_string(),
            owner_user_id: "postgres-contract-user".to_string(),
            agent_key: "chatos_cloud_agent".to_string(),
            input: Value::Null,
            status: CloudAgentRunStatus::ModelReady,
            phase: CloudAgentRunPhase::Ready,
            iteration: 0,
            model_config_ref: "model-contract".to_string(),
            model_runtime_snapshot_ref: "snapshot-contract".to_string(),
            agent_prompt_revision: "1".to_string(),
            agent_prompt_checksum: "checksum-contract".to_string(),
            capability_policy_revision: "policy-contract".to_string(),
            mcp_runtime_session_ref: None,
            previous_response_id: None,
            continuation_mode: None,
            pending_batch_id: None,
            pending_tool_calls: Vec::new(),
            pending_tool_results: Vec::new(),
            response_input_items: Vec::new(),
            current_input_items_ref: format!("conversation_turn:{run_id}:input"),
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
            event_id: format!("{}:started", run.ordering.agent_run_id),
            topic: "run_started".to_string(),
            routing_key: "cloud_agent.chatos.runtime".to_string(),
            ordering: run.ordering.clone(),
            causation_id: "postgres-contract-cause".to_string(),
            correlation_id: "postgres-contract-correlation".to_string(),
            available_at: Utc::now() - chrono::Duration::seconds(1),
            payload: serde_json::json!({"contract": true}),
        }
    }

    #[tokio::test]
    #[ignore = "requires CHATOS_TEST_DATABASE_URL and a migrated PostgreSQL database"]
    async fn state_store_preserves_lane_claim_transition_and_outbox_contract() {
        let database_url = std::env::var("CHATOS_TEST_DATABASE_URL")
            .expect("CHATOS_TEST_DATABASE_URL must be set");
        let config = chatos_postgres::PostgresConfig::new(database_url).expect("test config");
        let pool = chatos_postgres::connect(&config).await.expect("test pool");
        let store = CloudAgentPostgresStore::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let lane_key = format!("postgres-contract:{suffix}");
        let first_id = format!("postgres-contract-first:{suffix}");
        let second_id = format!("postgres-contract-second:{suffix}");

        let first_seq = store
            .allocate_lane_seq(&lane_key)
            .await
            .expect("first lane");
        let second_seq = store
            .allocate_lane_seq(&lane_key)
            .await
            .expect("second lane");
        assert_eq!((first_seq, second_seq), (1, 2));

        let first = run_record(&lane_key, &first_id, first_seq);
        let second = run_record(&lane_key, &second_id, second_seq);
        let initial_outbox = outbox_intent(&first);
        store
            .insert_run_with_outbox(first.clone(), vec![initial_outbox.clone()])
            .await
            .expect("insert first run and outbox");
        store
            .insert_run_with_outbox(second.clone(), Vec::new())
            .await
            .expect("insert second run");

        let second_claim = CloudAgentClaim {
            ordering: second.ordering.clone(),
            expected_status: second.status,
            expected_phase: second.phase,
            expected_version: second.version,
            claim_token: "second-token".to_string(),
            claim_until: Utc::now() + chrono::Duration::seconds(30),
        };
        assert_eq!(
            store
                .acquire_short_claim(&second_claim)
                .await
                .expect("out-of-order claim"),
            CloudAgentClaimResult::OutOfOrder
        );

        let first_claim = CloudAgentClaim {
            ordering: first.ordering.clone(),
            expected_status: first.status,
            expected_phase: first.phase,
            expected_version: first.version,
            claim_token: "first-token".to_string(),
            claim_until: Utc::now() + chrono::Duration::seconds(30),
        };
        assert_eq!(
            store
                .acquire_short_claim(&first_claim)
                .await
                .expect("first claim"),
            CloudAgentClaimResult::Acquired
        );
        let mut wrong_token = first_claim.clone();
        wrong_token.claim_token = "wrong-token".to_string();
        assert!(!store
            .renew_short_claim(&wrong_token)
            .await
            .expect("wrong-token renewal"));

        assert!(store
            .commit_transition(CloudAgentAtomicTransition {
                claim: first_claim.clone(),
                next_input: Value::Null,
                next_status: CloudAgentRunStatus::Succeeded,
                next_phase: CloudAgentRunPhase::Terminal,
                next_step_seq: 2,
                next_iteration: 1,
                next_retry_count: 0,
                previous_response_id: None,
                continuation_mode: None,
                current_input_items_ref: format!("conversation_turn:{first_id}:terminal"),
                mcp_runtime_session_ref: None,
                pending_batch_id: None,
                pending_tool_calls: Vec::new(),
                pending_tool_results: Vec::new(),
                response_input_items: Vec::new(),
                usage_accumulator: Value::Null,
                terminal_outcome: Some(serde_json::json!({"ok": true})),
                // Reusing an event id must remain idempotent inside the transition transaction.
                outbox: vec![initial_outbox.clone()],
            })
            .await
            .expect("terminal transition"));
        assert!(!store
            .renew_short_claim(&first_claim)
            .await
            .expect("stale renewal"));
        assert_eq!(
            store
                .acquire_short_claim(&second_claim)
                .await
                .expect("second claim after lane advance"),
            CloudAgentClaimResult::Acquired
        );

        let first_publish_token = format!("publisher-first:{suffix}");
        let pending = store
            .claim_ready_outbox_with_attempts(
                10,
                &first_publish_token,
                Utc::now() + chrono::Duration::seconds(30),
            )
            .await
            .expect("pending outbox");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].intent.event_id, initial_outbox.event_id);
        assert!(store
            .claim_ready_outbox_with_attempts(
                10,
                "publisher-blocked",
                Utc::now() + chrono::Duration::seconds(30),
            )
            .await
            .expect("active claim blocks a second publisher")
            .is_empty());
        assert!(!store
            .mark_claimed_outbox_published(&initial_outbox.event_id, "publisher-wrong")
            .await
            .expect("wrong publisher token"));

        let retry_at = Utc::now() - chrono::Duration::seconds(1);
        let failed = store
            .mark_claimed_outbox_publish_failed(
                &initial_outbox.event_id,
                &first_publish_token,
                "transient",
                retry_at,
                2,
            )
            .await
            .expect("record publish failure")
            .expect("pending outbox failure");
        assert_eq!(failed.publish_attempts, 1);
        assert!(!failed.dead_lettered);

        let publisher_a = format!("publisher-a:{suffix}");
        let publisher_b = format!("publisher-b:{suffix}");
        let claim_until = Utc::now() + chrono::Duration::seconds(30);
        let (claimed_a, claimed_b) = tokio::join!(
            store.claim_ready_outbox_with_attempts(10, &publisher_a, claim_until),
            store.claim_ready_outbox_with_attempts(10, &publisher_b, claim_until),
        );
        let claimed_a = claimed_a.expect("publisher A claim");
        let claimed_b = claimed_b.expect("publisher B claim");
        assert_eq!(claimed_a.len() + claimed_b.len(), 1);
        let winning_token = if claimed_a.is_empty() {
            publisher_b
        } else {
            publisher_a
        };

        sqlx::query(
            "UPDATE cloud_agent_outbox SET claim_until=now()-interval '1 second' WHERE event_id=$1",
        )
        .bind(&initial_outbox.event_id)
        .execute(&pool)
        .await
        .expect("expire publisher lease");
        let recovery_token = format!("publisher-recovery:{suffix}");
        assert_eq!(
            store
                .claim_ready_outbox_with_attempts(
                    10,
                    &recovery_token,
                    Utc::now() + chrono::Duration::seconds(30),
                )
                .await
                .expect("recover expired publisher lease")
                .len(),
            1
        );
        assert!(!store
            .mark_claimed_outbox_published(&initial_outbox.event_id, &winning_token)
            .await
            .expect("stale publisher token"));
        assert!(store
            .mark_claimed_outbox_published(&initial_outbox.event_id, &recovery_token)
            .await
            .expect("publish outbox"));

        let active_lane: i64 = sqlx::query_scalar(
            "SELECT active_lane_seq FROM cloud_agent_lanes WHERE ordering_lane_key=$1",
        )
        .bind(&lane_key)
        .fetch_one(&pool)
        .await
        .expect("active lane");
        assert_eq!(active_lane, 2);
        let outbox_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM cloud_agent_outbox WHERE event_id=$1")
                .bind(&initial_outbox.event_id)
                .fetch_one(&pool)
                .await
                .expect("outbox count");
        assert_eq!(outbox_count, 1);

        store
            .release_short_claim(&second_claim)
            .await
            .expect("release second claim");
        sqlx::query("DELETE FROM cloud_agent_runs WHERE ordering_lane_key=$1")
            .bind(&lane_key)
            .execute(&pool)
            .await
            .expect("delete contract runs");
        sqlx::query("DELETE FROM cloud_agent_lanes WHERE ordering_lane_key=$1")
            .bind(&lane_key)
            .execute(&pool)
            .await
            .expect("delete contract lane");
    }
}
