// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

mod post_process;

impl PostgresStore {
    pub(in crate::store) async fn run_execution_stats(&self) -> Result<RunExecutionStats, String> {
        let row = sqlx::query(
            "SELECT count(*) AS total, \
             count(*) FILTER (WHERE status IN ('queued','running')) AS active, \
             count(*) FILTER (WHERE status='queued') AS queued, \
             count(*) FILTER (WHERE status='running') AS running, \
             count(*) FILTER (WHERE status='succeeded') AS succeeded, \
             count(*) FILTER (WHERE status='failed') AS failed, \
             count(*) FILTER (WHERE status='cancelled') AS cancelled, \
             count(*) FILTER (WHERE status='blocked') AS blocked, \
             count(*) FILTER (WHERE dispatch_paused) AS dispatch_paused, \
             count(*) FILTER (WHERE data#>>'{chatos_started_callback_delivery,status}'='pending' \
                 OR data#>>'{chatos_callback_delivery,status}'='pending') AS callback_pending, \
             count(*) FILTER (WHERE data#>>'{chatos_started_callback_delivery,status}'='enqueued' \
                 OR data#>>'{chatos_callback_delivery,status}'='enqueued') AS callback_enqueued, \
             count(*) FILTER (WHERE dispatch_event_pending) AS dispatch_outbox_pending, \
             count(*) FILTER (WHERE cancel_event_pending) AS cancellation_outbox_pending, \
             count(*) FILTER (WHERE post_process_event_pending \
                 AND model_phase_status IN ('succeeded','failed','cancelled','blocked') \
                 AND (status IN ('succeeded','failed','cancelled','blocked') \
                     OR data#>>'{workspace_execution,integration_status}' IN ('pending','integrating','failed'))) \
                 AS post_process_outbox_pending, \
             count(*) FILTER (WHERE data#>>'{workspace_execution,integration_status}'='pending') AS integration_pending, \
             count(*) FILTER (WHERE data#>>'{workspace_execution,integration_status}'='integrating') AS integration_active, \
             count(*) FILTER (WHERE data#>>'{workspace_execution,integration_status}'='conflict') AS integration_conflicts, \
             count(*) FILTER (WHERE data#>>'{workspace_execution,integration_status}'='failed') AS integration_failed \
             FROM task_runs",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(db_error)?;
        Ok(RunExecutionStats {
            total: count_column(&row, "total")?,
            active: count_column(&row, "active")?,
            queued: count_column(&row, "queued")?,
            running: count_column(&row, "running")?,
            succeeded: count_column(&row, "succeeded")?,
            failed: count_column(&row, "failed")?,
            cancelled: count_column(&row, "cancelled")?,
            blocked: count_column(&row, "blocked")?,
            dispatch_paused: count_column(&row, "dispatch_paused")?,
            callback_pending: count_column(&row, "callback_pending")?,
            callback_enqueued: count_column(&row, "callback_enqueued")?,
            dispatch_outbox_pending: count_column(&row, "dispatch_outbox_pending")?,
            cancellation_outbox_pending: count_column(&row, "cancellation_outbox_pending")?,
            post_process_outbox_pending: count_column(&row, "post_process_outbox_pending")?,
            integration_pending: count_column(&row, "integration_pending")?,
            integration_active: count_column(&row, "integration_active")?,
            integration_conflicts: count_column(&row, "integration_conflicts")?,
            integration_failed: count_column(&row, "integration_failed")?,
        })
    }

    pub(in crate::store) async fn list_runs(
        &self,
        task_id: Option<&str>,
    ) -> Result<Vec<TaskRunRecord>, String> {
        let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT data FROM task_runs WHERE $1::text IS NULL OR task_id=$1 \
             ORDER BY created_at DESC,id",
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        rows.into_iter().map(decode_json).collect()
    }

    #[allow(dead_code)]
    pub(in crate::store) async fn latest_run_for_task_by_statuses(
        &self,
        task_id: &str,
        statuses: &[TaskRunStatus],
    ) -> Result<Option<TaskRunRecord>, String> {
        if statuses.is_empty() {
            return Ok(None);
        }
        let statuses = statuses
            .iter()
            .map(enum_text)
            .collect::<Result<Vec<_>, _>>()?;
        sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT data FROM task_runs WHERE task_id=$1 AND status=ANY($2) \
             ORDER BY created_at DESC,id DESC LIMIT 1",
        )
        .bind(task_id)
        .bind(statuses)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_error)?
        .map(decode_json)
        .transpose()
    }

    pub(in crate::store) async fn list_runs_filtered_scoped(
        &self,
        filters: &RunListFilters,
        owner_user_id: Option<&str>,
    ) -> Result<Vec<TaskRunRecord>, String> {
        load_filtered_runs(&self.pool, filters, owner_user_id).await
    }

    #[cfg(test)]
    pub(in crate::store) async fn list_runs_page(
        &self,
        filters: &RunListFilters,
    ) -> Result<PaginatedResponse<TaskRunRecord>, String> {
        self.list_runs_page_scoped(filters, None).await
    }

    pub(in crate::store) async fn list_runs_page_scoped(
        &self,
        filters: &RunListFilters,
        owner_user_id: Option<&str>,
    ) -> Result<PaginatedResponse<TaskRunRecord>, String> {
        let limit = filters.limit.unwrap_or(DEFAULT_PAGE_LIMIT);
        let offset = filters.offset.unwrap_or(0);
        let total = count_filtered_runs(&self.pool, filters, owner_user_id).await?;
        let mut page_filters = filters.clone();
        page_filters.limit = Some(limit);
        page_filters.offset = Some(offset);
        Ok(build_page_response(
            load_filtered_runs(&self.pool, &page_filters, owner_user_id).await?,
            total,
            limit,
            offset,
        ))
    }

    #[cfg(test)]
    pub(in crate::store) async fn list_run_summaries_filtered(
        &self,
        filters: &RunListFilters,
    ) -> Result<Vec<RunSummaryRecord>, String> {
        self.list_run_summaries_filtered_scoped(filters, None).await
    }

    pub(in crate::store) async fn list_run_summaries_filtered_scoped(
        &self,
        filters: &RunListFilters,
        owner_user_id: Option<&str>,
    ) -> Result<Vec<RunSummaryRecord>, String> {
        let status = filters.status.map(|value| enum_text(&value)).transpose()?;
        let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT jsonb_build_object('id',id,'task_id',task_id,'status',status, \
                 'model_config_id',model_config_id,'updated_at',data->'updated_at') \
             FROM task_runs WHERE ($1::text IS NULL OR task_id=$1) \
             AND ($2::text IS NULL OR status=$2) AND ($3::text IS NULL OR model_config_id=$3) \
             AND ($4::text IS NULL OR lower(id) LIKE '%'||$4||'%' \
                 OR lower(task_id) LIKE '%'||$4||'%' \
                 OR lower(model_config_id) LIKE '%'||$4||'%' \
                 OR lower(coalesce(data->>'result_summary','')) LIKE '%'||$4||'%' \
                 OR lower(coalesce(data->>'error_message','')) LIKE '%'||$4||'%') \
             AND ($5::text IS NULL OR EXISTS (SELECT 1 FROM tasks \
                 WHERE tasks.id=task_runs.task_id \
                 AND coalesce(nullif(btrim(tasks.owner_user_id),''),tasks.creator_user_id)=$5)) \
             ORDER BY created_at DESC,id LIMIT $6 OFFSET $7",
        )
        .bind(filters.task_id.as_deref())
        .bind(status)
        .bind(filters.model_config_id.as_deref())
        .bind(filters.keyword.as_deref())
        .bind(owner_user_id)
        .bind(optional_usize_as_i64(filters.limit))
        .bind(i64::try_from(filters.offset.unwrap_or(0)).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        rows.into_iter().map(decode_json).collect()
    }

    pub(in crate::store) async fn get_run_summaries_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<RunSummaryRecord>, String> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT jsonb_build_object('id',id,'task_id',task_id,'status',status, \
                 'model_config_id',model_config_id,'updated_at',data->'updated_at') \
             FROM task_runs WHERE id=ANY($1) ORDER BY updated_at DESC,id",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        rows.into_iter().map(decode_json).collect()
    }

    pub(in crate::store) async fn get_runs_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<TaskRunRecord>, String> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT data FROM task_runs WHERE id=ANY($1)",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        rows.into_iter().map(decode_json).collect()
    }

    pub(in crate::store) async fn get_run(
        &self,
        id: &str,
    ) -> Result<Option<TaskRunRecord>, String> {
        sqlx::query_scalar::<_, Json<serde_json::Value>>("SELECT data FROM task_runs WHERE id=$1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_error)?
            .map(decode_json)
            .transpose()
    }

    pub(in crate::store) async fn get_prior_active_run_for_execution_lane(
        &self,
        execution_lane_key: &str,
        created_at: &str,
        run_id: &str,
    ) -> Result<Option<TaskRunRecord>, String> {
        let value = sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT data FROM task_runs WHERE id<>$1 AND execution_lane_key=$2 \
             AND status IN ('queued','running') AND (status='running' OR NOT dispatch_paused) \
             AND (status='running' OR (created_at,id)<($3,$1)) ORDER BY created_at,id LIMIT 1",
        )
        .bind(run_id)
        .bind(execution_lane_key)
        .bind(timestamp(created_at)?)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_error)?;
        value.map(decode_json).transpose()
    }

    pub(in crate::store) async fn subscribe_run_terminal(
        &self,
        subscription: RunTerminalSubscriptionRecord,
    ) -> Result<TaskRunRecord, String> {
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        let value = sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT data FROM task_runs WHERE id=$1 FOR UPDATE",
        )
        .bind(&subscription.run_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_error)?
        .ok_or_else(|| format!("运行不存在: {}", subscription.run_id))?;
        let run: TaskRunRecord = decode_json(value)?;
        if !task_run_status_is_terminal(run.status) {
            sqlx::query(
                "INSERT INTO task_run_terminal_subscriptions(id,run_id,parent_run_id,worker_id,created_at,data) \
                 VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(id) DO NOTHING",
            )
            .bind(&subscription.id)
            .bind(&subscription.run_id)
            .bind(&subscription.parent_run_id)
            .bind(&subscription.worker_id)
            .bind(timestamp(&subscription.created_at)?)
            .bind(json(&subscription)?)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        }
        tx.commit().await.map_err(db_error)?;
        Ok(run)
    }

    pub(in crate::store) async fn list_pending_run_terminal_subscriptions(
        &self,
        limit: usize,
    ) -> Result<Vec<(TaskRunRecord, RunTerminalSubscriptionRecord)>, String> {
        let rows = sqlx::query_as::<
            _,
            (Json<serde_json::Value>, Json<serde_json::Value>),
        >(
            "SELECT r.data,s.data FROM task_run_terminal_subscriptions s JOIN task_runs r ON r.id=s.run_id \
             WHERE r.status IN ('succeeded','failed','cancelled','blocked') ORDER BY s.created_at,s.id LIMIT $1",
        )
        .bind(i64::try_from(limit.max(1)).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        rows.into_iter()
            .map(|(run, subscription)| Ok((decode_json(run)?, decode_json(subscription)?)))
            .collect()
    }

    pub(in crate::store) async fn acknowledge_run_terminal_subscription(
        &self,
        subscription_id: &str,
    ) -> Result<bool, String> {
        sqlx::query("DELETE FROM task_run_terminal_subscriptions WHERE id=$1")
            .bind(subscription_id)
            .execute(&self.pool)
            .await
            .map(|result| result.rows_affected() == 1)
            .map_err(db_error)
    }

    pub(in crate::store) async fn save_run(
        &self,
        mut run: TaskRunRecord,
    ) -> Result<TaskRunRecord, String> {
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        let current = Self::load_run_for_update(&mut tx, &run.id).await?;
        if let Some(current) = current.as_ref() {
            merge_run_async_progress(&mut run, current);
        }
        if let Some(claim_token) = run.claim_token.as_deref() {
            let current = current
                .as_ref()
                .ok_or_else(|| lost_run_claim_error(&run.id))?;
            if current.claim_token.as_deref() != Some(claim_token)
                || current.worker_id.as_deref() != run.worker_id.as_deref()
            {
                return Err(lost_run_claim_error(&run.id));
            }
            if current.cancel_requested {
                run.cancel_requested = true;
                run.cancel_event_pending |= current.cancel_event_pending;
            }
        }
        let run = prepare_run_for_claim_guarded_persist(run);
        Self::persist_run(&mut *tx, &run).await?;
        tx.commit().await.map_err(db_error)?;
        self.sync_cancel_cache(&run);
        Ok(run)
    }

    pub(in crate::store) async fn list_pending_chatos_callback_runs(
        &self,
        now: &str,
        limit: usize,
    ) -> Result<Vec<TaskRunRecord>, String> {
        let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT data FROM task_runs WHERE \
             (data#>>'{chatos_started_callback_delivery,status}'='pending' AND \
                (data#>>'{chatos_started_callback_delivery,next_attempt_at}' IS NULL OR \
                 (data#>>'{chatos_started_callback_delivery,next_attempt_at}')::timestamptz <= $1)) \
             OR (data#>>'{chatos_callback_delivery,status}'='pending' AND \
                (data#>>'{chatos_callback_delivery,next_attempt_at}' IS NULL OR \
                 (data#>>'{chatos_callback_delivery,next_attempt_at}')::timestamptz <= $1)) \
             ORDER BY updated_at,id LIMIT $2",
        )
        .bind(timestamp(now)?)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        rows.into_iter().map(decode_json).collect()
    }

    pub(in crate::store) async fn repair_stale_cancel_requested_runs(&self) -> Result<u64, String> {
        let ids = sqlx::query_scalar::<_, String>(
            "SELECT id FROM task_runs WHERE cancel_requested AND status NOT IN ('queued','running') ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        let mut repaired = 0_u64;
        for id in ids {
            if self
                .mutate_run(&id, |run| {
                    if !run.cancel_requested
                        || matches!(run.status, TaskRunStatus::Queued | TaskRunStatus::Running)
                    {
                        return false;
                    }
                    run.cancel_requested = false;
                    run.cancel_event_pending = false;
                    run.updated_at = now_rfc3339();
                    true
                })
                .await?
                .is_some()
            {
                self.cancel_requested_runs.write().remove(&id);
                repaired += 1;
            }
        }
        Ok(repaired)
    }

    pub(in crate::store) async fn mark_cancel_requested(
        &self,
        run_id: &str,
    ) -> Result<Option<TaskRunRecord>, String> {
        let result = self
            .mutate_run(run_id, |run| {
                run.cancel_requested = true;
                run.cancel_event_pending =
                    run.status == TaskRunStatus::Running && run.worker_id.is_some();
                run.updated_at = now_rfc3339();
                true
            })
            .await?;
        if result.is_some() {
            self.cancel_requested_runs
                .write()
                .insert(run_id.to_string());
        }
        Ok(result)
    }

    pub(in crate::store) async fn list_pending_run_cancel_events(
        &self,
        limit: usize,
    ) -> Result<Vec<TaskRunRecord>, String> {
        let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT data FROM task_runs WHERE status='running' AND cancel_requested AND cancel_event_pending \
             AND worker_id IS NOT NULL ORDER BY updated_at,id LIMIT $1",
        )
        .bind(i64::try_from(limit.max(1)).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        rows.into_iter().map(decode_json).collect()
    }

    pub(in crate::store) async fn acknowledge_run_cancel_event(
        &self,
        run_id: &str,
    ) -> Result<bool, String> {
        self.mutate_run(run_id, |run| {
            if !run.cancel_event_pending {
                return false;
            }
            run.cancel_event_pending = false;
            true
        })
        .await
        .map(|run| run.is_some())
    }

    pub(in crate::store) fn clear_cancel_requested(&self, run_id: &str) {
        self.cancel_requested_runs.write().remove(run_id);
        let store = self.clone();
        let run_id = run_id.to_string();
        tokio::spawn(async move {
            if let Err(error) = store
                .mutate_run(&run_id, |run| {
                    run.cancel_requested = false;
                    run.cancel_event_pending = false;
                    run.updated_at = now_rfc3339();
                    true
                })
                .await
            {
                tracing::warn!(run_id, error, "failed to clear cancel_requested flag");
            }
        });
    }

    pub(in crate::store) fn signal_local_run_abort(&self, run_id: &str) {
        self.cancel_requested_runs
            .write()
            .insert(run_id.to_string());
    }

    pub(in crate::store) fn is_cancel_requested(&self, run_id: &str) -> bool {
        self.cancel_requested_runs.read().contains(run_id)
    }

    pub(in crate::store) async fn has_active_run_for_task(
        &self,
        task_id: &str,
    ) -> Result<bool, String> {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM task_runs WHERE task_id=$1 AND status IN ('queued','running'))",
        )
        .bind(task_id)
        .fetch_one(&self.pool)
        .await
        .map_err(db_error)
    }

    async fn mutate_run<F>(&self, run_id: &str, mutate: F) -> Result<Option<TaskRunRecord>, String>
    where
        F: FnOnce(&mut TaskRunRecord) -> bool,
    {
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        let Some(mut run) = Self::load_run_for_update(&mut tx, run_id).await? else {
            return Ok(None);
        };
        if !mutate(&mut run) {
            return Ok(None);
        }
        Self::persist_run(&mut *tx, &run).await?;
        tx.commit().await.map_err(db_error)?;
        self.sync_cancel_cache(&run);
        Ok(Some(run))
    }
}

async fn load_filtered_runs(
    pool: &chatos_postgres::PgPool,
    filters: &RunListFilters,
    owner_user_id: Option<&str>,
) -> Result<Vec<TaskRunRecord>, String> {
    let status = filters.status.map(|value| enum_text(&value)).transpose()?;
    let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM task_runs WHERE ($1::text IS NULL OR task_id=$1) \
         AND ($2::text IS NULL OR status=$2) AND ($3::text IS NULL OR model_config_id=$3) \
         AND ($4::text IS NULL OR lower(id) LIKE '%'||$4||'%' \
             OR lower(task_id) LIKE '%'||$4||'%' \
             OR lower(model_config_id) LIKE '%'||$4||'%' \
             OR lower(coalesce(data->>'result_summary','')) LIKE '%'||$4||'%' \
             OR lower(coalesce(data->>'error_message','')) LIKE '%'||$4||'%') \
         AND ($5::text IS NULL OR EXISTS (SELECT 1 FROM tasks \
             WHERE tasks.id=task_runs.task_id \
             AND coalesce(nullif(btrim(tasks.owner_user_id),''),tasks.creator_user_id)=$5)) \
         ORDER BY created_at DESC,id LIMIT $6 OFFSET $7",
    )
    .bind(filters.task_id.as_deref())
    .bind(status)
    .bind(filters.model_config_id.as_deref())
    .bind(filters.keyword.as_deref())
    .bind(owner_user_id)
    .bind(optional_usize_as_i64(filters.limit))
    .bind(i64::try_from(filters.offset.unwrap_or(0)).unwrap_or(i64::MAX))
    .fetch_all(pool)
    .await
    .map_err(db_error)?;
    rows.into_iter().map(decode_json).collect()
}

async fn count_filtered_runs(
    pool: &chatos_postgres::PgPool,
    filters: &RunListFilters,
    owner_user_id: Option<&str>,
) -> Result<usize, String> {
    let status = filters.status.map(|value| enum_text(&value)).transpose()?;
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM task_runs WHERE ($1::text IS NULL OR task_id=$1) \
         AND ($2::text IS NULL OR status=$2) AND ($3::text IS NULL OR model_config_id=$3) \
         AND ($4::text IS NULL OR lower(id) LIKE '%'||$4||'%' \
             OR lower(task_id) LIKE '%'||$4||'%' \
             OR lower(model_config_id) LIKE '%'||$4||'%' \
             OR lower(coalesce(data->>'result_summary','')) LIKE '%'||$4||'%' \
             OR lower(coalesce(data->>'error_message','')) LIKE '%'||$4||'%') \
         AND ($5::text IS NULL OR EXISTS (SELECT 1 FROM tasks \
             WHERE tasks.id=task_runs.task_id \
             AND coalesce(nullif(btrim(tasks.owner_user_id),''),tasks.creator_user_id)=$5))",
    )
    .bind(filters.task_id.as_deref())
    .bind(status)
    .bind(filters.model_config_id.as_deref())
    .bind(filters.keyword.as_deref())
    .bind(owner_user_id)
    .fetch_one(pool)
    .await
    .map_err(db_error)?;
    usize::try_from(total).map_err(|_| "run count exceeds usize".to_string())
}

fn count_column(row: &sqlx::postgres::PgRow, name: &str) -> Result<usize, String> {
    let value = row.try_get::<i64, _>(name).map_err(db_error)?;
    usize::try_from(value).map_err(|_| format!("{name} count exceeds usize"))
}
