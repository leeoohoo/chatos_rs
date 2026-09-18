// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tokio::task::JoinHandle;

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct UserDataRetentionStats {
    pub successful_runs_total: u64,
    pub failed_runs_total: u64,
    pub deleted_rows_total: u64,
    pub last_success_unix: Option<i64>,
}

#[derive(Default)]
struct UserDataRetentionCounters {
    successful_runs_total: AtomicU64,
    failed_runs_total: AtomicU64,
    deleted_rows_total: AtomicU64,
    last_success_unix: AtomicI64,
}

#[derive(Clone)]
pub struct UserDataRetention {
    pool: sqlx::PgPool,
    interval: Duration,
    batch_size: i64,
    counters: Arc<UserDataRetentionCounters>,
}

impl UserDataRetention {
    pub fn new(
        pool: sqlx::PgPool,
        interval: Duration,
        batch_size: usize,
    ) -> Result<Self, String> {
        if interval.is_zero() {
            return Err("user data retention interval must be positive".to_string());
        }
        let batch_size = i64::try_from(batch_size)
            .map_err(|_| "user data retention batch size is too large".to_string())?;
        if batch_size == 0 {
            return Err("user data retention batch size must be positive".to_string());
        }
        Ok(Self {
            pool,
            interval,
            batch_size,
            counters: Arc::new(UserDataRetentionCounters::default()),
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
                    Ok(deleted) if deleted > 0 => {
                        tracing::info!(deleted_rows = deleted, "pruned expired user data");
                    }
                    Ok(_) => {}
                    Err(error) => tracing::warn!(
                        error = error.as_str(),
                        "failed to prune expired user data"
                    ),
                }
            }
        })
    }

    pub fn stats(&self) -> UserDataRetentionStats {
        let last_success_unix = self.counters.last_success_unix.load(Ordering::Relaxed);
        UserDataRetentionStats {
            successful_runs_total: self
                .counters
                .successful_runs_total
                .load(Ordering::Relaxed),
            failed_runs_total: self.counters.failed_runs_total.load(Ordering::Relaxed),
            deleted_rows_total: self.counters.deleted_rows_total.load(Ordering::Relaxed),
            last_success_unix: (last_success_unix > 0).then_some(last_success_unix),
        }
    }

    async fn prune_once(&self) -> Result<u64, String> {
        match prune_expired_user_data(&self.pool, self.batch_size).await {
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

async fn prune_expired_user_data(pool: &sqlx::PgPool, batch_size: i64) -> Result<u64, String> {
    let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;
    let unix_expiry_tables = [
        ("revoked_tokens", "jti"),
        ("registration_email_codes", "email"),
        ("local_connector_auth_tickets", "id"),
        ("wechat_bind_tickets", "id"),
        ("client_sessions", "id"),
    ];
    let timestamp_expiry_tables = [
        ("device_proof_nonces", "id"),
        ("login_throttle", "key"),
    ];
    let mut deleted = 0_u64;
    for (table, primary_key) in unix_expiry_tables {
        let statement = format!(
            "WITH expired AS (SELECT ctid FROM {table} \
             WHERE expires_at<=extract(epoch FROM now())::bigint \
             ORDER BY expires_at,{primary_key} LIMIT $1 FOR UPDATE SKIP LOCKED) \
             DELETE FROM {table} target USING expired WHERE target.ctid=expired.ctid"
        );
        deleted = deleted.saturating_add(
            sqlx::query(&statement)
                .bind(batch_size)
                .execute(&mut *transaction)
                .await
                .map_err(|error| error.to_string())?
                .rows_affected(),
        );
    }
    for (table, primary_key) in timestamp_expiry_tables {
        let statement = format!(
            "WITH expired AS (SELECT ctid FROM {table} WHERE expires_at<=now() \
             ORDER BY expires_at,{primary_key} LIMIT $1 FOR UPDATE SKIP LOCKED) \
             DELETE FROM {table} target USING expired WHERE target.ctid=expired.ctid"
        );
        deleted = deleted.saturating_add(
            sqlx::query(&statement)
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
#[path = "retention/tests.rs"]
mod tests;
