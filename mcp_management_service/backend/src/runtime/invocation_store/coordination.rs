// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl RuntimeInvocationStore {
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
            RuntimeInvocationStoreBackend::Mongo(collection) => collection
                .find(
                    doc! {
                        "status": { "$in": active_statuses
                            .iter()
                            .map(|status| status.as_str())
                            .collect::<Vec<_>>() },
                        "expires_at_unix": { "$gt": now },
                    },
                    None,
                )
                .await
                .map_err(|error| format!("list active Runtime Invocations failed: {error}"))?
                .try_collect::<Vec<_>>()
                .await
                .map_err(|error| format!("read active Runtime Invocations failed: {error}"))?,
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
            RuntimeInvocationStoreBackend::Mongo(collection) => collection
                .find_one_and_delete(
                    doc! {
                        "_id": invocation_id,
                        "session_id": session_id,
                        "status": RuntimeInvocationStatus::Queued.as_str(),
                    },
                    None,
                )
                .await
                .map_err(|error| {
                    format!("discard queued Runtime Invocation registration failed: {error}")
                })?,
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
                doc! { "session_id": session_id, "request_id_key": request_id_key },
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
                doc! { "_id": invocation_id, "caller_service": caller_service },
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
            RuntimeInvocationStoreBackend::Mongo(collection) => collection
                .find_one(
                    doc! { "_id": invocation_id, "session_id": session_id },
                    None,
                )
                .await
                .map_err(|error| {
                    format!("load Runtime Invocation for closed session failed: {error}")
                })?,
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
            RuntimeInvocationStoreBackend::Mongo(collection) => collection
                .find(
                    doc! {
                        "session_id": session_id,
                        "status": { "$in": active_statuses
                            .iter()
                            .map(|status| status.as_str())
                            .collect::<Vec<_>>() },
                    },
                    None,
                )
                .await
                .map_err(|error| {
                    format!("load active Runtime Invocations for session close failed: {error}")
                })?
                .try_collect::<Vec<_>>()
                .await
                .map_err(|error| {
                    format!("read active Runtime Invocations for session close failed: {error}")
                }),
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

    async fn request_cancel<F>(
        &self,
        mut identity_filter: mongodb::bson::Document,
        memory_matches: F,
    ) -> Result<Option<RuntimeInvocationRecord>, String>
    where
        F: Fn(&RuntimeInvocationRecord) -> bool,
    {
        let now = chrono::Utc::now().timestamp();
        identity_filter.insert("expires_at_unix", doc! { "$gt": now });
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
            RuntimeInvocationStoreBackend::Mongo(collection) => {
                let mut running_filter = identity_filter.clone();
                running_filter.insert(
                    "status",
                    doc! {
                        "$in": [
                            RuntimeInvocationStatus::Queued.as_str(),
                            RuntimeInvocationStatus::Running.as_str(),
                            RuntimeInvocationStatus::WaitingForUser.as_str(),
                        ]
                    },
                );
                let updated = collection
                    .find_one_and_update(
                        running_filter,
                        doc! { "$set": { "status": RuntimeInvocationStatus::CancelRequested.as_str() } },
                        FindOneAndUpdateOptions::builder()
                            .return_document(ReturnDocument::After)
                            .build(),
                    )
                    .await
                    .map_err(|error| format!("request Runtime Invocation cancellation failed: {error}"))?;
                if updated.is_some() {
                    return Ok(updated);
                }
                collection
                    .find_one(identity_filter, None)
                    .await
                    .map_err(|error| {
                        format!("load Runtime Invocation cancellation state failed: {error}")
                    })
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
            RuntimeInvocationStoreBackend::Mongo(collection) => collection
                .find_one(
                    doc! {
                        "_id": invocation_id,
                        "status": RuntimeInvocationStatus::CancelRequested.as_str(),
                        "expires_at_unix": { "$gt": now },
                    },
                    None,
                )
                .await
                .map(|record| record.is_some())
                .map_err(|error| {
                    format!("load Runtime Invocation cancellation state failed: {error}")
                }),
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
