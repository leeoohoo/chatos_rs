// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_cloud_agent_protocol::CloudAgentRunRecord;
use chatos_cloud_agent_runtime::{
    apply_cloud_agent_transition, bounded_cloud_agent_outbox_error, cloud_agent_outbox_failure,
    validate_initial_cloud_agent_state, CloudAgentAtomicTransition, CloudAgentClaim,
    CloudAgentClaimResult, CloudAgentOutboxIntent, CloudAgentOutboxPublishFailure,
    CloudAgentPendingOutboxIntent, CloudAgentRunStore, CloudAgentStateRepository,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::types::Json;

use crate::db::Db;
use crate::repositories::postgres::json;

#[derive(Clone)]
pub struct CloudAgentPostgresStore {
    pool: Db,
}

impl CloudAgentPostgresStore {
    pub fn new(pool: Db) -> Self {
        Self { pool }
    }

    async fn insert_outbox<'e, E>(
        executor: E,
        intent: &CloudAgentOutboxIntent,
    ) -> Result<(), String>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        sqlx::query("INSERT INTO cloud_agent_outbox(event_id,agent_run_id,status,available_at,publish_attempts,created_at,updated_at,data) VALUES($1,$2,'pending',$3,0,now(),now(),$4) ON CONFLICT(event_id) DO NOTHING")
            .bind(&intent.event_id).bind(&intent.ordering.agent_run_id).bind(intent.available_at).bind(json(intent)?)
            .execute(executor).await.map(|_|()).map_err(db_error)
    }
}

fn db_error(error: sqlx::Error) -> String {
    error.to_string()
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

#[async_trait]
impl CloudAgentStateRepository for CloudAgentPostgresStore {
    async fn allocate_lane_seq(&self, key: &str) -> Result<u64, String> {
        if key.trim().is_empty() {
            return Err("ordering_lane_key must not be empty".to_string());
        }
        let value=sqlx::query_scalar::<_,i64>("INSERT INTO cloud_agent_lanes(ordering_lane_key,next_lane_seq,active_lane_seq,version,updated_at) VALUES($1,1,1,1,now()) ON CONFLICT(ordering_lane_key) DO UPDATE SET next_lane_seq=cloud_agent_lanes.next_lane_seq+1,version=cloud_agent_lanes.version+1,updated_at=now() RETURNING next_lane_seq")
            .bind(key).fetch_one(&self.pool).await.map_err(db_error)?;
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
        sqlx::query("INSERT INTO cloud_agent_runs(agent_run_id,ordering_lane_key,lane_seq,generation,step_seq,status,phase,version,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
            .bind(&record.ordering.agent_run_id).bind(&record.ordering.ordering_lane_key).bind(lane_seq)
            .bind(i64_value(record.ordering.generation,"generation")?).bind(i64_value(record.ordering.step_seq,"step_seq")?)
            .bind(enum_text(&record.status)?).bind(enum_text(&record.phase)?).bind(i64_value(record.version,"version")?).bind(record.updated_at)
            .execute(&mut *tx).await.map_err(db_error)?;
        sqlx::query("INSERT INTO engine_cloud_agent_run_states(agent_run_id,updated_at,data) VALUES($1,$2,$3)")
            .bind(&record.ordering.agent_run_id).bind(record.updated_at).bind(json(&record)?).execute(&mut *tx).await.map_err(db_error)?;
        for intent in &outbox {
            Self::insert_outbox(&mut *tx, intent).await?;
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
        sqlx::query_scalar::<_,i64>("UPDATE cloud_agent_lanes SET active_lane_seq=$1,version=version+1,updated_at=now() WHERE ordering_lane_key=$2 AND active_lane_seq=$3 RETURNING active_lane_seq")
            .bind(next).bind(key).bind(i64_value(seq,"lane_seq")?).fetch_optional(&self.pool).await.map_err(db_error)?.map(|v|u64_value(v,"active_lane_seq")).transpose()
    }

    async fn claim_ready_outbox_with_attempts(
        &self,
        limit: i64,
        claim_token: &str,
        claim_until: DateTime<Utc>,
    ) -> Result<Vec<CloudAgentPendingOutboxIntent>, String> {
        if claim_token.trim().is_empty() { return Err("Cloud Agent outbox claim token must not be empty".to_string()); }
        let rows=sqlx::query_as::<_,(Json<serde_json::Value>,i32)>("WITH candidates AS (SELECT event_id FROM cloud_agent_outbox WHERE (status='pending' AND available_at<=now()) OR (status='publishing' AND claim_until<=now()) ORDER BY available_at,event_id LIMIT $1 FOR UPDATE SKIP LOCKED) UPDATE cloud_agent_outbox o SET status='publishing',claim_token=$2,claim_until=$3,updated_at=now() FROM candidates c WHERE o.event_id=c.event_id RETURNING o.data,o.publish_attempts")
            .bind(limit.max(1)).bind(claim_token).bind(claim_until).fetch_all(&self.pool).await.map_err(db_error)?;
        rows.into_iter()
            .map(|(value, attempts)| {
                Ok(CloudAgentPendingOutboxIntent {
                    intent: serde_json::from_value(value.0).map_err(|e| e.to_string())?,
                    publish_attempts: u32::try_from(attempts).unwrap_or(u32::MAX),
                })
            })
            .collect()
    }

    async fn mark_claimed_outbox_published(&self, id: &str, claim_token: &str) -> Result<bool, String> {
        sqlx::query("UPDATE cloud_agent_outbox SET status='published',publish_attempts=publish_attempts+1,claim_token=NULL,claim_until=NULL,updated_at=now() WHERE event_id=$1 AND status='publishing' AND claim_token=$2")
            .bind(id).bind(claim_token).execute(&self.pool).await.map(|r|r.rows_affected()==1).map_err(db_error)
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
        let row=sqlx::query_as::<_,(i32,String)>("UPDATE cloud_agent_outbox SET publish_attempts=publish_attempts+1,last_error=$1,available_at=$2,status=CASE WHEN publish_attempts+1 >= $3 THEN 'dead_lettered' ELSE 'pending' END,claim_token=NULL,claim_until=NULL,updated_at=now() WHERE event_id=$4 AND status='publishing' AND claim_token=$5 RETURNING publish_attempts,status")
            .bind(bounded).bind(next).bind(i32::try_from(max.max(1)).unwrap_or(i32::MAX)).bind(id).bind(claim_token).fetch_optional(&self.pool).await.map_err(db_error)?;
        Ok(cloud_agent_outbox_failure(row, next))
    }
}

#[async_trait]
impl CloudAgentRunStore for CloudAgentPostgresStore {
    async fn load_run(&self, id: &str) -> Result<Option<CloudAgentRunRecord>, String> {
        sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT data FROM engine_cloud_agent_run_states WHERE agent_run_id=$1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_error)?
        .map(decode_run)
        .transpose()
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
            tx.commit().await.map_err(db_error)?;
            return Ok(CloudAgentClaimResult::OutOfOrder);
        }
        let row=sqlx::query_as::<_,(i64,i64,String,String,i64,Option<String>,Option<DateTime<Utc>>)>("SELECT generation,step_seq,status,phase,version,claim_token,claim_until FROM cloud_agent_runs WHERE agent_run_id=$1 FOR UPDATE")
            .bind(&claim.ordering.agent_run_id).fetch_optional(&mut *tx).await.map_err(db_error)?;
        let Some((generation, step, status, phase, version, token, until)) = row else {
            tx.commit().await.map_err(db_error)?;
            return Ok(CloudAgentClaimResult::Conflict);
        };
        let expected_status = enum_text(&claim.expected_status)?;
        let expected_phase = enum_text(&claim.expected_phase)?;
        let result = if status_is_terminal(&status) {
            CloudAgentClaimResult::Terminal
        } else if generation > i64_value(claim.ordering.generation, "generation")?
            || step > i64_value(claim.ordering.step_seq, "step_seq")?
            || version > i64_value(claim.expected_version, "version")?
        {
            CloudAgentClaimResult::Duplicate
        } else if generation != i64_value(claim.ordering.generation, "generation")?
            || step != i64_value(claim.ordering.step_seq, "step_seq")?
            || status != expected_status
            || phase != expected_phase
            || version != i64_value(claim.expected_version, "version")?
            || (token.as_deref().is_some_and(|v| v != claim.claim_token)
                && until.is_some_and(|v| v > Utc::now()))
        {
            CloudAgentClaimResult::Conflict
        } else {
            sqlx::query(
                "UPDATE cloud_agent_runs SET claim_token=$1,claim_until=$2 WHERE agent_run_id=$3",
            )
            .bind(&claim.claim_token)
            .bind(claim.claim_until)
            .bind(&claim.ordering.agent_run_id)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
            CloudAgentClaimResult::Acquired
        };
        tx.commit().await.map_err(db_error)?;
        Ok(result)
    }

    async fn renew_short_claim(&self, claim: &CloudAgentClaim) -> Result<bool, String> {
        claim.validate()?;
        let r=sqlx::query("UPDATE cloud_agent_runs SET claim_until=$1 WHERE agent_run_id=$2 AND claim_token=$3 AND ordering_lane_key=$4 AND lane_seq=$5 AND generation=$6 AND step_seq=$7 AND status=$8 AND phase=$9 AND version=$10")
            .bind(claim.claim_until).bind(&claim.ordering.agent_run_id).bind(&claim.claim_token).bind(&claim.ordering.ordering_lane_key)
            .bind(i64_value(claim.ordering.lane_seq,"lane_seq")?).bind(i64_value(claim.ordering.generation,"generation")?).bind(i64_value(claim.ordering.step_seq,"step_seq")?)
            .bind(enum_text(&claim.expected_status)?).bind(enum_text(&claim.expected_phase)?).bind(i64_value(claim.expected_version,"version")?)
            .execute(&self.pool).await.map_err(db_error)?;
        Ok(r.rows_affected() == 1)
    }

    async fn commit_transition(
        &self,
        transition: CloudAgentAtomicTransition,
    ) -> Result<bool, String> {
        transition.validate()?;
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        let claim = transition.claim.clone();
        let row=sqlx::query_as::<_,(Json<serde_json::Value>,Option<String>)>("SELECT s.data,r.claim_token FROM cloud_agent_runs r JOIN engine_cloud_agent_run_states s USING(agent_run_id) WHERE r.agent_run_id=$1 FOR UPDATE OF r,s")
            .bind(&claim.ordering.agent_run_id).fetch_optional(&mut *tx).await.map_err(db_error)?;
        let Some((value, token)) = row else {
            return Ok(false);
        };
        let mut run = decode_run(value)?;
        let Some(outbox) =
            apply_cloud_agent_transition(&mut run, transition, token.as_deref(), Utc::now())?
        else {
            return Ok(false);
        };
        sqlx::query("UPDATE cloud_agent_runs SET step_seq=$1,status=$2,phase=$3,version=$4,claim_token=NULL,claim_until=NULL,updated_at=$5 WHERE agent_run_id=$6")
            .bind(i64_value(run.ordering.step_seq,"step_seq")?).bind(enum_text(&run.status)?).bind(enum_text(&run.phase)?).bind(i64_value(run.version,"version")?)
            .bind(run.updated_at).bind(&run.ordering.agent_run_id).execute(&mut *tx).await.map_err(db_error)?;
        sqlx::query(
            "UPDATE engine_cloud_agent_run_states SET updated_at=$1,data=$2 WHERE agent_run_id=$3",
        )
        .bind(run.updated_at)
        .bind(json(&run)?)
        .bind(&run.ordering.agent_run_id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
        for intent in &outbox {
            Self::insert_outbox(&mut *tx, intent).await?;
        }
        if run.status.is_terminal() {
            let advanced=sqlx::query("UPDATE cloud_agent_lanes SET active_lane_seq=active_lane_seq+1,version=version+1,updated_at=now() WHERE ordering_lane_key=$1 AND active_lane_seq=$2")
            .bind(&run.ordering.ordering_lane_key).bind(i64_value(run.ordering.lane_seq,"lane_seq")?).execute(&mut *tx).await.map_err(db_error)?;
            if advanced.rows_affected() != 1 {
                return Err("claimed Cloud Agent lane changed before terminal commit".to_string());
            }
        }
        tx.commit().await.map_err(db_error)?;
        Ok(true)
    }

    async fn release_short_claim(&self, claim: &CloudAgentClaim) -> Result<(), String> {
        sqlx::query("UPDATE cloud_agent_runs SET claim_token=NULL,claim_until=NULL WHERE agent_run_id=$1 AND claim_token=$2").bind(&claim.ordering.agent_run_id).bind(&claim.claim_token)
            .execute(&self.pool).await.map(|_|()).map_err(db_error)
    }
}

fn status_is_terminal(status: &str) -> bool {
    matches!(status, "succeeded" | "failed" | "blocked" | "cancelled")
}
