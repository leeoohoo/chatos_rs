// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl RuntimeInvocationStore {
    pub(crate) async fn claim_expired_active(
        &self,
        limit: usize,
        lease: std::time::Duration,
    ) -> Result<Vec<ExpiredRuntimeInvocationClaim>, String> {
        let limit = limit.clamp(1, 10_000);
        let lease_seconds = i64::try_from(lease.as_secs())
            .map_err(|_| "Runtime Invocation recovery lease is too large".to_string())?;
        if lease_seconds == 0 {
            return Err("Runtime Invocation recovery lease must be positive".to_string());
        }
        match self.backend.as_ref() {
            RuntimeInvocationStoreBackend::Memory(invocations) => {
                let now = chrono::Utc::now();
                let mut invocations = invocations.write().await;
                let mut candidates = invocations
                    .values_mut()
                    .filter(|record| {
                        record.expires_at <= now
                            && active_runtime_invocation_statuses().contains(&record.status)
                    })
                    .collect::<Vec<_>>();
                candidates.sort_by(|left, right| {
                    left.expires_at
                        .cmp(&right.expires_at)
                        .then_with(|| left.invocation_id.cmp(&right.invocation_id))
                });
                Ok(candidates
                    .into_iter()
                    .take(limit)
                    .map(|record| {
                        let cancellation_required =
                            record.status != RuntimeInvocationStatus::Queued;
                        record.status = RuntimeInvocationStatus::CancelRequested;
                        ExpiredRuntimeInvocationClaim {
                            record: record.clone(),
                            claim_token: uuid::Uuid::new_v4().to_string(),
                            cancellation_required,
                        }
                    })
                    .collect())
            }
            RuntimeInvocationStoreBackend::Postgres(pool) => {
                let claim_token = uuid::Uuid::new_v4().to_string();
                let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
                let values = sqlx::query_scalar::<_, Json<serde_json::Value>>(
                    "SELECT data FROM mcp_management_runtime_invocations \
                     WHERE expires_at<=now() \
                       AND status IN ('queued','running','waiting_for_user','cancel_requested') \
                       AND (recovery_claim_until IS NULL OR recovery_claim_until<=now()) \
                     ORDER BY expires_at,invocation_id LIMIT $1 FOR UPDATE SKIP LOCKED",
                )
                .bind(i64::try_from(limit).unwrap_or(i64::MAX))
                .fetch_all(&mut *tx)
                .await
                .map_err(|error| format!("claim expired Runtime Invocations failed: {error}"))?;
                let mut claims = Vec::with_capacity(values.len());
                for value in values {
                    let mut record = decode_invocation(value)?;
                    let cancellation_required = record.status != RuntimeInvocationStatus::Queued;
                    record.status = RuntimeInvocationStatus::CancelRequested;
                    persist_invocation(&mut *tx, &record).await?;
                    sqlx::query(
                        "UPDATE mcp_management_runtime_invocations \
                         SET recovery_claim_token=$2, \
                             recovery_claim_until=now()+make_interval(secs => $3::double precision) \
                         WHERE invocation_id=$1",
                    )
                    .bind(record.invocation_id.as_str())
                    .bind(claim_token.as_str())
                    .bind(lease_seconds as f64)
                    .execute(&mut *tx)
                    .await
                    .map_err(|error| {
                        format!("persist Runtime Invocation recovery claim failed: {error}")
                    })?;
                    claims.push(ExpiredRuntimeInvocationClaim {
                        record,
                        claim_token: claim_token.clone(),
                        cancellation_required,
                    });
                }
                tx.commit().await.map_err(|error| error.to_string())?;
                Ok(claims)
            }
        }
    }

    pub async fn list_active(&self, limit: usize) -> Result<Vec<RuntimeInvocationRecord>, String> {
        let now = chrono::Utc::now().timestamp();
        let active_statuses = active_runtime_invocation_statuses();
        let limit = limit.clamp(1, 10_000);
        let mut records = match self.backend.as_ref() {
            RuntimeInvocationStoreBackend::Memory(invocations) => {
                let mut invocations = invocations.write().await;
                invocations.retain(|_, record| record.expires_at_unix > now);
                invocations
                    .values()
                    .filter(|record| active_statuses.contains(&record.status))
                    .cloned()
                    .collect::<Vec<_>>()
            }
            RuntimeInvocationStoreBackend::Postgres(pool) => {
                sqlx::query_scalar::<_, Json<serde_json::Value>>(
                    "SELECT data FROM mcp_management_runtime_invocations WHERE \
                 status IN ('queued','running','waiting_for_user','cancel_requested') \
                 AND expires_at_unix>$1 ORDER BY created_at_unix_ms,invocation_id LIMIT $2",
                )
                .bind(now)
                .bind(i64::try_from(limit).unwrap_or(i64::MAX))
                .fetch_all(pool)
                .await
                .map_err(|error| format!("list active Runtime Invocations failed: {error}"))?
                .into_iter()
                .map(decode_invocation)
                .collect::<Result<Vec<_>, _>>()?
            }
        };
        records.sort_by(|left, right| {
            left.created_at_unix_ms
                .cmp(&right.created_at_unix_ms)
                .then_with(|| left.invocation_id.cmp(&right.invocation_id))
        });
        records.truncate(limit);
        Ok(records)
    }

    pub async fn discard_queued_registration(
        &self,
        invocation_id: &str,
        session_id: &str,
    ) -> Result<bool, String> {
        let record = match self.backend.as_ref() {
            RuntimeInvocationStoreBackend::Memory(invocations) => {
                let mut invocations = invocations.write().await;
                let removable = invocations.get(invocation_id).is_some_and(|record| {
                    record.session_id == session_id
                        && record.status == RuntimeInvocationStatus::Queued
                });
                removable
                    .then(|| invocations.remove(invocation_id))
                    .flatten()
            }
            RuntimeInvocationStoreBackend::Postgres(pool) => {
                sqlx::query_scalar::<_, Json<serde_json::Value>>(
                    "DELETE FROM mcp_management_runtime_invocations WHERE invocation_id=$1 \
                 AND session_id=$2 AND status='queued' RETURNING data",
                )
                .bind(invocation_id)
                .bind(session_id)
                .fetch_optional(pool)
                .await
                .map_err(|error| {
                    format!("discard queued Runtime Invocation registration failed: {error}")
                })?
                .map(decode_invocation)
                .transpose()?
            }
        };
        let Some(record) = record else {
            return Ok(false);
        };
        self.quota.release(&record).await?;
        Ok(true)
    }

    pub async fn request_cancel_by_request(
        &self,
        session_id: &str,
        request_id_key: &str,
    ) -> Result<Option<RuntimeInvocationRecord>, String> {
        let record = self
            .request_cancel(
                CancelIdentity::Request {
                    session_id,
                    request_id_key,
                },
                |record| record.session_id == session_id && record.request_id_key == request_id_key,
            )
            .await?;
        self.signal_cancelled_record(record.as_ref())?;
        Ok(record)
    }

    pub async fn request_cancel_by_invocation(
        &self,
        invocation_id: &str,
        caller_service: &str,
    ) -> Result<Option<RuntimeInvocationRecord>, String> {
        let record = self
            .request_cancel(
                CancelIdentity::Invocation {
                    invocation_id,
                    caller_service,
                },
                |record| {
                    record.invocation_id == invocation_id && record.caller_service == caller_service
                },
            )
            .await?;
        self.signal_cancelled_record(record.as_ref())?;
        Ok(record)
    }

    pub async fn close_session(&self, session_id: &str) -> Result<usize, String> {
        let records = self.active_session_invocations(session_id).await?;
        let mut reclaimed = 0usize;
        for record in records {
            if self.close_registered_invocation_record(&record).await? {
                reclaimed = reclaimed.saturating_add(1);
            }
        }
        self.diagnostics
            .session_closed_reclaimed
            .fetch_add(reclaimed as u64, Ordering::Relaxed);
        Ok(reclaimed)
    }

    pub async fn close_registered_invocation(
        &self,
        invocation_id: &str,
        session_id: &str,
    ) -> Result<bool, String> {
        let record = match self.backend.as_ref() {
            RuntimeInvocationStoreBackend::Memory(invocations) => invocations
                .read()
                .await
                .get(invocation_id)
                .filter(|record| record.session_id == session_id)
                .cloned(),
            RuntimeInvocationStoreBackend::Postgres(pool) => {
                load_invocation_by(
                    pool,
                    "SELECT data FROM mcp_management_runtime_invocations \
                 WHERE invocation_id=$1 AND session_id=$2 AND expires_at_unix>$3",
                    invocation_id,
                    session_id,
                    chrono::Utc::now().timestamp(),
                )
                .await?
            }
        };
        let Some(record) = record else {
            return Ok(false);
        };
        let reclaimed = self.close_registered_invocation_record(&record).await?;
        if reclaimed {
            self.diagnostics
                .session_closed_reclaimed
                .fetch_add(1, Ordering::Relaxed);
        }
        Ok(reclaimed)
    }

    async fn active_session_invocations(
        &self,
        session_id: &str,
    ) -> Result<Vec<RuntimeInvocationRecord>, String> {
        let active_statuses = active_runtime_invocation_statuses();
        match self.backend.as_ref() {
            RuntimeInvocationStoreBackend::Memory(invocations) => Ok(invocations
                .read()
                .await
                .values()
                .filter(|record| {
                    record.session_id == session_id && active_statuses.contains(&record.status)
                })
                .cloned()
                .collect()),
            RuntimeInvocationStoreBackend::Postgres(pool) => {
                sqlx::query_scalar::<_, Json<serde_json::Value>>(
                    "SELECT data FROM mcp_management_runtime_invocations WHERE session_id=$1 \
                 AND status IN ('queued','running','waiting_for_user','cancel_requested') \
                 ORDER BY created_at_unix_ms,invocation_id",
                )
                .bind(session_id)
                .fetch_all(pool)
                .await
                .map_err(|error| {
                    format!("load active Runtime Invocations for session close failed: {error}")
                })?
                .into_iter()
                .map(decode_invocation)
                .collect()
            }
        }
    }

    async fn close_registered_invocation_record(
        &self,
        record: &RuntimeInvocationRecord,
    ) -> Result<bool, String> {
        if !active_runtime_invocation_statuses().contains(&record.status) {
            return Ok(false);
        }
        self.signal_cancellation(record.invocation_id.as_str())?;
        let terminal_status = if record.status != RuntimeInvocationStatus::Queued
            && record.mutation_may_have_started
        {
            RuntimeInvocationStatus::UnknownExecutionState
        } else {
            RuntimeInvocationStatus::Cancelled
        };
        self.transition_terminal(
            record.invocation_id.as_str(),
            active_runtime_invocation_statuses(),
            terminal_status,
            None,
            None,
            Some("runtime_session_closed".to_string()),
        )
        .await
    }

    async fn request_cancel<'a, F>(
        &self,
        identity: CancelIdentity<'a>,
        memory_matches: F,
    ) -> Result<Option<RuntimeInvocationRecord>, String>
    where
        F: Fn(&RuntimeInvocationRecord) -> bool,
    {
        let now = chrono::Utc::now().timestamp();
        match self.backend.as_ref() {
            RuntimeInvocationStoreBackend::Memory(invocations) => {
                let mut invocations = invocations.write().await;
                invocations.retain(|_, record| record.expires_at_unix > now);
                let record = invocations
                    .values_mut()
                    .find(|record| memory_matches(record));
                if let Some(record) = record {
                    if matches!(
                        record.status,
                        RuntimeInvocationStatus::Queued
                            | RuntimeInvocationStatus::Running
                            | RuntimeInvocationStatus::WaitingForUser
                    ) {
                        record.status = RuntimeInvocationStatus::CancelRequested;
                    }
                    return Ok(Some(record.clone()));
                }
                Ok(None)
            }
            RuntimeInvocationStoreBackend::Postgres(pool) => {
                let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
                let value = match identity {
                    CancelIdentity::Request {
                        session_id,
                        request_id_key,
                    } => sqlx::query_scalar::<_, Json<serde_json::Value>>(
                        "SELECT data FROM mcp_management_runtime_invocations WHERE session_id=$1 \
                         AND request_id_key=$2 AND expires_at_unix>$3 FOR UPDATE",
                    )
                    .bind(session_id)
                    .bind(request_id_key)
                    .bind(now)
                    .fetch_optional(&mut *tx)
                    .await,
                    CancelIdentity::Invocation {
                        invocation_id,
                        caller_service,
                    } => sqlx::query_scalar::<_, Json<serde_json::Value>>(
                        "SELECT data FROM mcp_management_runtime_invocations WHERE invocation_id=$1 \
                         AND caller_service=$2 AND expires_at_unix>$3 FOR UPDATE",
                    )
                    .bind(invocation_id)
                    .bind(caller_service)
                    .bind(now)
                    .fetch_optional(&mut *tx)
                    .await,
                }
                .map_err(|error| format!("load Runtime Invocation cancellation state failed: {error}"))?;
                let Some(value) = value else {
                    return Ok(None);
                };
                let mut record = decode_invocation(value)?;
                if matches!(
                    record.status,
                    RuntimeInvocationStatus::Queued
                        | RuntimeInvocationStatus::Running
                        | RuntimeInvocationStatus::WaitingForUser
                ) {
                    record.status = RuntimeInvocationStatus::CancelRequested;
                    persist_invocation(&mut *tx, &record).await?;
                }
                tx.commit().await.map_err(|error| error.to_string())?;
                Ok(Some(record))
            }
        }
    }

    pub async fn cancellation_requested(&self, invocation_id: &str) -> Result<bool, String> {
        let now = chrono::Utc::now().timestamp();
        match self.backend.as_ref() {
            RuntimeInvocationStoreBackend::Memory(invocations) => {
                let mut invocations = invocations.write().await;
                invocations.retain(|_, record| record.expires_at_unix > now);
                Ok(invocations.get(invocation_id).is_some_and(|record| {
                    record.status == RuntimeInvocationStatus::CancelRequested
                }))
            }
            RuntimeInvocationStoreBackend::Postgres(pool) => sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM mcp_management_runtime_invocations WHERE \
                 invocation_id=$1 AND status='cancel_requested' AND expires_at_unix>$2)",
            )
            .bind(invocation_id)
            .bind(now)
            .fetch_one(pool)
            .await
            .map_err(|error| format!("load Runtime Invocation cancellation state failed: {error}")),
        }
    }

    pub async fn wait_for_cancellation(&self, invocation_id: &str) -> Result<(), String> {
        let notify = Arc::new(Notify::new());
        {
            let mut waiters = self
                .cancellation_waiters
                .lock()
                .map_err(|_| "Runtime Invocation cancellation waiter registry is poisoned")?;
            waiters.insert(invocation_id.to_string(), Arc::downgrade(&notify));
        }
        let result = match self.cancellation_requested(invocation_id).await {
            Ok(true) => Ok(()),
            Ok(false) => {
                notify.notified().await;
                Ok(())
            }
            Err(error) => Err(error),
        };
        self.remove_cancellation_waiter(invocation_id, &notify)?;
        result
    }

    pub fn signal_cancellation(&self, invocation_id: &str) -> Result<(), String> {
        let mut waiters = self
            .cancellation_waiters
            .lock()
            .map_err(|_| "Runtime Invocation cancellation waiter registry is poisoned")?;
        let Some(waiter) = waiters.get(invocation_id) else {
            return Ok(());
        };
        let Some(notify) = waiter.upgrade() else {
            waiters.remove(invocation_id);
            return Ok(());
        };
        notify.notify_one();
        Ok(())
    }

    pub async fn reconcile_cancellation_waiters(&self) -> Result<(), String> {
        let invocation_ids = self
            .cancellation_waiters
            .lock()
            .map_err(|_| "Runtime Invocation cancellation waiter registry is poisoned")?
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for invocation_id in invocation_ids {
            if self.cancellation_requested(invocation_id.as_str()).await? {
                self.signal_cancellation(invocation_id.as_str())?;
            }
        }
        Ok(())
    }

    fn signal_cancelled_record(
        &self,
        record: Option<&RuntimeInvocationRecord>,
    ) -> Result<(), String> {
        if let Some(record) =
            record.filter(|record| record.status == RuntimeInvocationStatus::CancelRequested)
        {
            self.signal_cancellation(record.invocation_id.as_str())?;
        }
        Ok(())
    }

    fn remove_cancellation_waiter(
        &self,
        invocation_id: &str,
        notify: &Arc<Notify>,
    ) -> Result<(), String> {
        let mut waiters = self
            .cancellation_waiters
            .lock()
            .map_err(|_| "Runtime Invocation cancellation waiter registry is poisoned")?;
        if waiters
            .get(invocation_id)
            .and_then(Weak::upgrade)
            .is_some_and(|current| Arc::ptr_eq(&current, notify))
        {
            waiters.remove(invocation_id);
        }
        Ok(())
    }
}

enum CancelIdentity<'a> {
    Request {
        session_id: &'a str,
        request_id_key: &'a str,
    },
    Invocation {
        invocation_id: &'a str,
        caller_service: &'a str,
    },
}
