// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tokio::task::JoinHandle;

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct RuntimeRetentionStats {
    pub successful_runs_total: u64,
    pub failed_runs_total: u64,
    pub deleted_rows_total: u64,
    pub last_success_unix: Option<i64>,
}

#[derive(Default)]
struct RuntimeRetentionCounters {
    successful_runs_total: AtomicU64,
    failed_runs_total: AtomicU64,
    deleted_rows_total: AtomicU64,
    last_success_unix: AtomicI64,
}

#[derive(Clone)]
pub struct RuntimeRetention {
    pool: chatos_postgres::PgPool,
    interval: Duration,
    batch_size: i64,
    counters: Arc<RuntimeRetentionCounters>,
}

impl RuntimeRetention {
    pub fn new(
        pool: chatos_postgres::PgPool,
        interval: Duration,
        batch_size: usize,
    ) -> Result<Self, String> {
        if interval.is_zero() {
            return Err("runtime session retention interval must be positive".to_string());
        }
        let batch_size = i64::try_from(batch_size)
            .map_err(|_| "runtime session retention batch size is too large".to_string())?;
        if batch_size == 0 {
            return Err("runtime session retention batch size must be positive".to_string());
        }
        Ok(Self {
            pool,
            interval,
            batch_size,
            counters: Arc::new(RuntimeRetentionCounters::default()),
        })
    }

    pub fn spawn(&self) -> JoinHandle<()> {
        let retention = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(retention.interval);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                match retention.prune_once().await {
                    Ok(deleted) => {
                        if deleted > 0 {
                            tracing::info!(
                                deleted_rows = deleted,
                                "pruned expired MCP runtime artifacts"
                            );
                        }
                    }
                    Err(error) => tracing::warn!(
                        error = error.as_str(),
                        "failed to prune expired MCP runtime artifacts"
                    ),
                }
            }
        })
    }

    pub fn stats(&self) -> RuntimeRetentionStats {
        let last_success_unix = self.counters.last_success_unix.load(Ordering::Relaxed);
        RuntimeRetentionStats {
            successful_runs_total: self.counters.successful_runs_total.load(Ordering::Relaxed),
            failed_runs_total: self.counters.failed_runs_total.load(Ordering::Relaxed),
            deleted_rows_total: self.counters.deleted_rows_total.load(Ordering::Relaxed),
            last_success_unix: (last_success_unix > 0).then_some(last_success_unix),
        }
    }

    async fn prune_once(&self) -> Result<u64, String> {
        match prune_expired_session_artifacts(&self.pool, self.batch_size).await {
            Ok(deleted) => {
                self.counters
                    .successful_runs_total
                    .fetch_add(1, Ordering::Relaxed);
                self.counters
                    .deleted_rows_total
                    .fetch_add(deleted, Ordering::Relaxed);
                self.counters
                    .last_success_unix
                    .store(chrono::Utc::now().timestamp(), Ordering::Relaxed);
                Ok(deleted)
            }
            Err(error) => {
                self.counters
                    .failed_runs_total
                    .fetch_add(1, Ordering::Relaxed);
                Err(error)
            }
        }
    }
}

async fn prune_expired_session_artifacts(
    pool: &chatos_postgres::PgPool,
    batch_size: i64,
) -> Result<u64, String> {
    let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;
    let mut deleted = 0_u64;
    for statement in [
        "WITH expired AS (SELECT ctid FROM mcp_management_runtime_tool_batches \
         WHERE expires_at<=now() AND status='completed' AND pending_event_type IS NULL \
         ORDER BY expires_at,batch_id LIMIT $1 FOR UPDATE SKIP LOCKED) \
         DELETE FROM mcp_management_runtime_tool_batches target \
         USING expired WHERE target.ctid=expired.ctid",
        "WITH expired AS (SELECT invocation.ctid FROM mcp_management_runtime_invocations invocation \
         WHERE invocation.expires_at<=now() \
         AND invocation.status IN ('completed','failed','cancelled','unknown_execution_state') \
         AND NOT EXISTS (SELECT 1 FROM mcp_management_runtime_tool_batches batch \
         WHERE batch.invocation_ids @> ARRAY[invocation.invocation_id]) \
         ORDER BY invocation.expires_at,invocation.invocation_id LIMIT $1 \
         FOR UPDATE OF invocation SKIP LOCKED) DELETE FROM mcp_management_runtime_invocations target \
         USING expired WHERE target.ctid=expired.ctid",
        "WITH expired AS (SELECT ctid FROM mcp_management_skill_activations \
         WHERE expires_at<=now() ORDER BY expires_at,activation_ref LIMIT $1 \
         FOR UPDATE SKIP LOCKED) DELETE FROM mcp_management_skill_activations target \
         USING expired WHERE target.ctid=expired.ctid",
        "WITH expired AS (SELECT ctid FROM mcp_management_runtime_session_close_results \
         WHERE expires_at<=now() ORDER BY expires_at,session_id LIMIT $1 \
         FOR UPDATE SKIP LOCKED) DELETE FROM mcp_management_runtime_session_close_results target \
         USING expired WHERE target.ctid=expired.ctid",
        "WITH expired AS (SELECT scope.ctid FROM mcp_management_runtime_execution_scopes scope \
         WHERE scope.expires_at<=now() AND scope.running_invocation_id IS NULL \
         AND NOT EXISTS (SELECT 1 FROM mcp_management_runtime_execution_scope_queue_items queued \
         WHERE queued.scope_id=scope.id) ORDER BY scope.expires_at,scope.id LIMIT $1 \
         FOR UPDATE OF scope SKIP LOCKED) DELETE FROM mcp_management_runtime_execution_scopes target \
         USING expired WHERE target.ctid=expired.ctid",
        "WITH expired AS (SELECT ctid FROM mcp_management_runtime_session_snapshots \
         WHERE expires_at<=now() ORDER BY expires_at,session_id LIMIT $1 \
         FOR UPDATE SKIP LOCKED) DELETE FROM mcp_management_runtime_session_snapshots target \
         USING expired WHERE target.ctid=expired.ctid",
    ] {
        deleted = deleted.saturating_add(
            sqlx::query(statement)
                .bind(batch_size)
                .execute(&mut *transaction)
                .await
                .map_err(|error| error.to_string())?
                .rows_affected(),
        );
    }
    transaction
        .commit()
        .await
        .map_err(|error| error.to_string())?;
    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn retention_configuration_rejects_zero_values() {
        let pool =
            chatos_postgres::PgPool::connect_lazy("postgresql://unused:unused@localhost/unused")
                .expect("lazy pool");
        assert!(RuntimeRetention::new(pool.clone(), Duration::ZERO, 1).is_err());
        assert!(RuntimeRetention::new(pool, Duration::from_secs(1), 0).is_err());
    }

    #[tokio::test]
    #[ignore = "requires MCP_MANAGEMENT_TEST_DATABASE_URL and migrated PostgreSQL"]
    async fn postgres_retention_prunes_only_safe_expired_runtime_artifacts() {
        let database_url = std::env::var("MCP_MANAGEMENT_TEST_DATABASE_URL")
            .expect("MCP_MANAGEMENT_TEST_DATABASE_URL must be set");
        let pool = crate::postgres::connect(&database_url)
            .await
            .expect("test database");
        let suffix = uuid::Uuid::new_v4().to_string();
        let expired = format!("retention-expired-{suffix}");
        let live = format!("retention-live-{suffix}");
        let busy_scope = format!("retention-busy-scope-{suffix}");
        let pending = format!("retention-pending-{suffix}");
        let expired_invocation = format!("invocation-{expired}");
        let live_invocation = format!("invocation-{live}");
        let pending_invocation = format!("invocation-{pending}");

        for (id, expiry) in [
            (&expired, "now()-interval '1 second'"),
            (&live, "now()+interval '1 hour'"),
        ] {
            sqlx::query(&format!(
                "INSERT INTO mcp_management_runtime_session_snapshots \
                 (session_id,schema_version,expires_at,expires_at_unix,nonce,encrypted_snapshot) \
                 VALUES($1,1,{expiry},extract(epoch FROM {expiry})::bigint,$2,$3)"
            ))
            .bind(id)
            .bind(vec![1_u8])
            .bind(vec![2_u8])
            .execute(&pool)
            .await
            .expect("session snapshot");
            sqlx::query(&format!(
                "INSERT INTO mcp_management_runtime_session_close_results \
                 (session_id,caller_service,expires_at,expires_at_unix,data) \
                 VALUES($1,'contract',{expiry},extract(epoch FROM {expiry})::bigint,'{{}}'::jsonb)"
            ))
            .bind(id)
            .execute(&pool)
            .await
            .expect("session close result");
            sqlx::query(&format!(
                "INSERT INTO mcp_management_skill_activations \
                 (activation_ref,runtime_session_id,equivalence_sha256,expires_at,expires_at_unix,nonce,encrypted_activation) \
                 VALUES($1,$2,'contract',{expiry},extract(epoch FROM {expiry})::bigint,$3,$4)"
            ))
            .bind(format!("activation-{id}"))
            .bind(id)
            .bind(vec![3_u8])
            .bind(vec![4_u8])
            .execute(&pool)
            .await
            .expect("skill activation");
        }

        for (id, expiry) in [
            (&expired, "now()-interval '1 second'"),
            (&live, "now()+interval '1 hour'"),
            (&busy_scope, "now()-interval '1 second'"),
        ] {
            sqlx::query(&format!(
                "INSERT INTO mcp_management_runtime_execution_scopes \
                 (id,owner_user_id,scope_kind,project_id,run_id,provider,generation,status, \
                  terminal_status,next_invocation_sequence,running_invocation_id,session_refs, \
                  updated_at,expires_at,expires_at_unix) \
                 VALUES($1,'contract','project','contract-project',$2,'local_connector',1,'active', \
                        NULL,1,NULL,'{{}}'::jsonb,now(),{expiry},extract(epoch FROM {expiry})::bigint)"
            ))
            .bind(id)
            .bind(format!("run-{id}"))
            .execute(&pool)
            .await
            .expect("execution scope");
        }
        sqlx::query(
            "INSERT INTO mcp_management_runtime_execution_scope_queue_items \
             (scope_id,invocation_id,sequence,status) VALUES($1,$2,1,'queued')",
        )
        .bind(&busy_scope)
        .bind(format!("invocation-{busy_scope}"))
        .execute(&pool)
        .await
        .expect("busy execution scope queue item");

        for (invocation_id, session_id, expiry) in [
            (&expired_invocation, &expired, "now()-interval '1 second'"),
            (&live_invocation, &live, "now()+interval '1 hour'"),
            (&pending_invocation, &pending, "now()-interval '1 second'"),
        ] {
            sqlx::query(&format!(
                "INSERT INTO mcp_management_runtime_invocations \
                 (invocation_id,session_id,request_id_key,caller_service,tenant_id,owner_user_id, \
                  resource_id,status,created_at_unix_ms,completed_at_unix_ms,expires_at,expires_at_unix,data) \
                 VALUES($1,$2,$1,'contract','tenant','owner','resource','completed',1,2, \
                        {expiry},extract(epoch FROM {expiry})::bigint,'{{}}'::jsonb)"
            ))
            .bind(invocation_id)
            .bind(session_id)
            .execute(&pool)
            .await
            .expect("terminal invocation");
        }
        for (batch_id, session_id, invocation_id, expiry, pending_event) in [
            (
                &expired,
                &expired,
                &expired_invocation,
                "now()-interval '1 second'",
                None,
            ),
            (
                &live,
                &live,
                &live_invocation,
                "now()+interval '1 hour'",
                None,
            ),
            (
                &pending,
                &pending,
                &pending_invocation,
                "now()-interval '1 second'",
                Some("aggregate_result"),
            ),
        ] {
            sqlx::query(&format!(
                "INSERT INTO mcp_management_runtime_tool_batches \
                 (batch_id,session_id,status,next_call_index,pending_event_type,revision,invocation_ids, \
                  created_at_unix_ms,updated_at_unix_ms,expires_at,expires_at_unix,data) \
                 VALUES($1,$2,'completed',1,$3,1,ARRAY[$4],1,2, \
                        {expiry},extract(epoch FROM {expiry})::bigint,'{{}}'::jsonb)"
            ))
            .bind(batch_id)
            .bind(session_id)
            .bind(pending_event)
            .bind(invocation_id)
            .execute(&pool)
            .await
            .expect("completed tool batch");
        }

        assert_eq!(prune_expired_session_artifacts(&pool, 10).await.unwrap(), 6);
        for table in [
            "mcp_management_runtime_session_snapshots",
            "mcp_management_runtime_session_close_results",
        ] {
            let count: i64 = sqlx::query_scalar(&format!(
                "SELECT count(*) FROM {table} WHERE session_id=ANY($1)"
            ))
            .bind(vec![expired.clone(), live.clone()])
            .fetch_one(&pool)
            .await
            .expect("remaining session artifacts");
            assert_eq!(count, 1);
        }
        let activation_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM mcp_management_skill_activations WHERE runtime_session_id=ANY($1)",
        )
        .bind(vec![expired.clone(), live.clone()])
        .fetch_one(&pool)
        .await
        .expect("remaining activations");
        assert_eq!(activation_count, 1);
        let retained_scopes: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM mcp_management_runtime_execution_scopes WHERE id=ANY($1) ORDER BY id",
        )
        .bind(vec![expired.clone(), live.clone(), busy_scope.clone()])
        .fetch_all(&pool)
        .await
        .expect("remaining execution scopes");
        assert_eq!(retained_scopes, vec![busy_scope.clone(), live.clone()]);
        let retained_batches: Vec<String> = sqlx::query_scalar(
            "SELECT batch_id FROM mcp_management_runtime_tool_batches WHERE batch_id=ANY($1) ORDER BY batch_id",
        )
        .bind(vec![expired.clone(), live.clone(), pending.clone()])
        .fetch_all(&pool)
        .await
        .expect("remaining tool batches");
        assert_eq!(retained_batches, vec![live.clone(), pending.clone()]);
        let retained_invocations: Vec<String> = sqlx::query_scalar(
            "SELECT invocation_id FROM mcp_management_runtime_invocations WHERE invocation_id=ANY($1) ORDER BY invocation_id",
        )
        .bind(vec![
            expired_invocation.clone(),
            live_invocation.clone(),
            pending_invocation.clone(),
        ])
        .fetch_all(&pool)
        .await
        .expect("remaining invocations");
        assert_eq!(
            retained_invocations,
            vec![live_invocation.clone(), pending_invocation.clone()]
        );

        sqlx::query(
            "DELETE FROM mcp_management_skill_activations WHERE runtime_session_id=ANY($1)",
        )
        .bind(vec![expired.clone(), live.clone()])
        .execute(&pool)
        .await
        .expect("cleanup activations");
        for table in [
            "mcp_management_runtime_session_close_results",
            "mcp_management_runtime_session_snapshots",
        ] {
            sqlx::query(&format!("DELETE FROM {table} WHERE session_id=ANY($1)"))
                .bind(vec![expired.clone(), live.clone()])
                .execute(&pool)
                .await
                .expect("cleanup session artifacts");
        }
        sqlx::query("DELETE FROM mcp_management_runtime_execution_scopes WHERE id=ANY($1)")
            .bind(vec![expired.clone(), live.clone(), busy_scope])
            .execute(&pool)
            .await
            .expect("cleanup execution scopes");
        sqlx::query("DELETE FROM mcp_management_runtime_tool_batches WHERE batch_id=ANY($1)")
            .bind(vec![expired.clone(), live.clone(), pending.clone()])
            .execute(&pool)
            .await
            .expect("cleanup tool batches");
        sqlx::query("DELETE FROM mcp_management_runtime_invocations WHERE invocation_id=ANY($1)")
            .bind(vec![
                expired_invocation,
                live_invocation,
                pending_invocation,
            ])
            .execute(&pool)
            .await
            .expect("cleanup invocations");
    }
}
