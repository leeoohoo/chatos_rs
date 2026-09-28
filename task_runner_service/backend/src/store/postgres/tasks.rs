// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

mod queries;

use self::queries::{
    count_filtered_tasks, load_filtered_task_summaries, load_filtered_tasks, task_stats_query,
};

impl PostgresStore {
    pub(in crate::store) async fn list_tasks(&self) -> Result<Vec<TaskRecord>, String> {
        let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT data FROM tasks ORDER BY updated_at DESC,id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        rows.into_iter().map(decode_json).collect()
    }

    pub(in crate::store) async fn list_tasks_filtered(
        &self,
        filters: &TaskListFilters,
    ) -> Result<Vec<TaskRecord>, String> {
        load_filtered_tasks(&self.pool, filters).await
    }

    pub(in crate::store) async fn list_tasks_page(
        &self,
        filters: &TaskListFilters,
    ) -> Result<PaginatedResponse<TaskRecord>, String> {
        let limit = filters.limit.unwrap_or(DEFAULT_PAGE_LIMIT);
        let offset = filters.offset.unwrap_or(0);
        let cursor_mode = filters.cursor()?.is_some();
        let total = count_filtered_tasks(&self.pool, filters).await?;
        let mut page_filters = filters.clone();
        page_filters.limit = Some(if cursor_mode {
            limit.saturating_add(1)
        } else {
            limit
        });
        page_filters.offset = Some(offset);
        let mut items = load_filtered_tasks(&self.pool, &page_filters).await?;
        if cursor_mode {
            let has_more = items.len() > limit;
            items.truncate(limit);
            return Ok(PaginatedResponse {
                items,
                total,
                limit,
                offset,
                has_more,
            });
        }
        Ok(build_page_response(items, total, limit, offset))
    }

    pub(in crate::store) async fn get_task(&self, id: &str) -> Result<Option<TaskRecord>, String> {
        sqlx::query_scalar::<_, Json<serde_json::Value>>("SELECT data FROM tasks WHERE id=$1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_error)?
            .map(decode_json)
            .transpose()
    }

    pub(in crate::store) async fn list_task_summaries_filtered(
        &self,
        filters: &TaskListFilters,
    ) -> Result<Vec<TaskSummaryRecord>, String> {
        load_filtered_task_summaries(&self.pool, filters).await
    }

    pub(in crate::store) async fn get_task_summaries_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<TaskSummaryRecord>, String> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT jsonb_build_object('id',id,'title',data->'title','status',status, \
                 'default_model_config_id',default_model_config_id,'project_id',project_id, \
                 'creator_user_id',creator_user_id,'creator_username',data->'creator_username', \
                 'creator_display_name',data->'creator_display_name','owner_user_id',owner_user_id, \
                 'owner_username',data->'owner_username','owner_display_name',data->'owner_display_name', \
                 'last_run_id',data->'last_run_id','updated_at',data->'updated_at') \
             FROM tasks WHERE id=ANY($1) ORDER BY updated_at DESC,id",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        rows.into_iter().map(decode_json).collect()
    }

    pub(in crate::store) async fn list_task_tags(&self) -> Result<Vec<String>, String> {
        sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT unnest(tags) AS tag FROM tasks ORDER BY tag",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)
    }

    pub(in crate::store) async fn task_stats(&self) -> Result<TaskStatsResponse, String> {
        self.task_stats_filtered(&TaskListFilters::default()).await
    }

    pub(in crate::store) async fn task_stats_filtered(
        &self,
        filters: &TaskListFilters,
    ) -> Result<TaskStatsResponse, String> {
        task_stats_query(&self.pool, filters).await
    }

    pub(in crate::store) async fn claim_due_scheduled_tasks(
        &self,
        now: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<TaskRecord>, String> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        let values = sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT data FROM tasks WHERE schedule_mode <> 'manual' AND schedule_due_at <= $1 \
             AND status NOT IN ('archived','cancelled','queued','running') ORDER BY schedule_due_at,id \
             LIMIT $2 FOR UPDATE SKIP LOCKED",
        )
        .bind(now)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&mut *tx)
        .await
        .map_err(db_error)?;
        let mut claimed = Vec::with_capacity(values.len());
        for value in values {
            let mut task: TaskRecord = decode_json(value)?;
            task.schedule =
                crate::services::advance_task_schedule_after_dispatch(&task.schedule, now)?;
            task.updated_at = now_rfc3339();
            persist_task(&mut *tx, &task).await?;
            claimed.push(task);
        }
        tx.commit().await.map_err(db_error)?;
        Ok(claimed)
    }

    pub(in crate::store) async fn save_task(&self, task: TaskRecord) -> Result<TaskRecord, String> {
        persist_task(&self.pool, &task).await?;
        Ok(task)
    }

    pub(in crate::store) async fn update_tasks_batch(
        &self,
        tasks: &[TaskRecord],
    ) -> Result<(), String> {
        if tasks.is_empty() {
            return Ok(());
        }
        let rows = tasks
            .iter()
            .map(|task| {
                Ok((
                    task.id.clone(),
                    enum_text(&task.status)?,
                    timestamp(&task.updated_at)?,
                    json(task)?,
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut query = sqlx::QueryBuilder::new(
            "UPDATE tasks AS target SET status=batch.status,updated_at=batch.updated_at,data=batch.data FROM (",
        );
        query.push_values(&rows, |mut values, row| {
            values
                .push_bind(&row.0)
                .push_bind(&row.1)
                .push_bind(row.2)
                .push_bind(&row.3);
        });
        query.push(") AS batch(id,status,updated_at,data) WHERE target.id=batch.id");

        let mut tx = self.pool.begin().await.map_err(db_error)?;
        let updated = query
            .build()
            .execute(&mut *tx)
            .await
            .map_err(db_error)?
            .rows_affected();
        if updated != u64::try_from(rows.len()).unwrap_or(u64::MAX) {
            tx.rollback().await.map_err(db_error)?;
            return Err("one or more tasks disappeared during batch update".to_string());
        }
        tx.commit().await.map_err(db_error)
    }

    pub(in crate::store) async fn save_task_and_set_prerequisites_if_revision(
        &self,
        task: TaskRecord,
        prerequisite_task_ids: Vec<String>,
        expected_revision: i64,
    ) -> Result<Option<TaskRecord>, String> {
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        let revision: i64 = sqlx::query_scalar(
            "SELECT revision FROM task_dependency_graph_revisions WHERE scope='global' FOR UPDATE",
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(db_error)?;
        if revision != expected_revision {
            return Ok(None);
        }
        persist_task(&mut *tx, &task).await?;
        sqlx::query("DELETE FROM task_prerequisites WHERE task_id=$1")
            .bind(&task.id)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        let now = now_rfc3339();
        let mut ids = prerequisite_task_ids.into_iter().collect::<BTreeSet<_>>();
        ids.remove(&task.id);
        for prerequisite_task_id in ids {
            let record = TaskPrerequisiteRecord {
                task_id: task.id.clone(),
                prerequisite_task_id,
                created_at: now.clone(),
            };
            sqlx::query(
                "INSERT INTO task_prerequisites(task_id,prerequisite_task_id,created_at,data) \
                 VALUES($1,$2,$3,$4)",
            )
            .bind(&record.task_id)
            .bind(&record.prerequisite_task_id)
            .bind(timestamp(&record.created_at)?)
            .bind(json(&record)?)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        }
        sqlx::query(
            "UPDATE task_dependency_graph_revisions SET revision=revision+1,updated_at=now() \
             WHERE scope='global'",
        )
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
        tx.commit().await.map_err(db_error)?;
        Ok(Some(task))
    }

    pub(in crate::store) async fn update_task_schedule_if_next_run_at(
        &self,
        task_id: &str,
        expected_next_run_at: &str,
        schedule: TaskScheduleConfig,
        updated_at: &str,
    ) -> Result<Option<TaskRecord>, String> {
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        let value = sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT data FROM tasks WHERE id=$1 FOR UPDATE",
        )
        .bind(task_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_error)?;
        let Some(value) = value else {
            return Ok(None);
        };
        let mut task: TaskRecord = decode_json(value)?;
        if task.schedule.next_run_at.as_deref() != Some(expected_next_run_at) {
            return Ok(None);
        }
        task.schedule = schedule;
        task.updated_at = updated_at.to_string();
        persist_task(&mut *tx, &task).await?;
        tx.commit().await.map_err(db_error)?;
        Ok(Some(task))
    }

    pub(in crate::store) async fn list_task_prerequisites(
        &self,
        task_id: &str,
    ) -> Result<Vec<TaskPrerequisiteRecord>, String> {
        load_prerequisites(&self.pool, "WHERE task_id=$1", &[task_id]).await
    }

    pub(in crate::store) async fn list_task_prerequisites_for_tasks(
        &self,
        task_ids: &[String],
    ) -> Result<Vec<TaskPrerequisiteRecord>, String> {
        if task_ids.is_empty() {
            return Ok(Vec::new());
        }
        let values = sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT data FROM task_prerequisites WHERE task_id = ANY($1) ORDER BY task_id,prerequisite_task_id",
        )
        .bind(task_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        values.into_iter().map(decode_json).collect()
    }

    pub(in crate::store) async fn list_task_dependents(
        &self,
        prerequisite_task_id: &str,
    ) -> Result<Vec<TaskPrerequisiteRecord>, String> {
        let values = sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT data FROM task_prerequisites WHERE prerequisite_task_id=$1 ORDER BY task_id",
        )
        .bind(prerequisite_task_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        values.into_iter().map(decode_json).collect()
    }

    pub(in crate::store) async fn set_task_prerequisites(
        &self,
        task_id: &str,
        prerequisite_task_ids: Vec<String>,
    ) -> Result<Vec<TaskPrerequisiteRecord>, String> {
        self.set_prerequisites_transaction(task_id, prerequisite_task_ids, None)
            .await?
            .ok_or_else(|| "dependency graph revision changed unexpectedly".to_string())
    }

    pub(in crate::store) async fn dependency_graph_revision(&self) -> Result<i64, String> {
        sqlx::query_scalar(
            "SELECT revision FROM task_dependency_graph_revisions WHERE scope='global'",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(db_error)
    }

    pub(in crate::store) async fn set_task_prerequisites_if_revision(
        &self,
        task_id: &str,
        prerequisite_task_ids: Vec<String>,
        expected_revision: i64,
    ) -> Result<Option<Vec<TaskPrerequisiteRecord>>, String> {
        self.set_prerequisites_transaction(task_id, prerequisite_task_ids, Some(expected_revision))
            .await
    }

    async fn set_prerequisites_transaction(
        &self,
        task_id: &str,
        prerequisite_task_ids: Vec<String>,
        expected_revision: Option<i64>,
    ) -> Result<Option<Vec<TaskPrerequisiteRecord>>, String> {
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        let revision: i64 = sqlx::query_scalar(
            "SELECT revision FROM task_dependency_graph_revisions WHERE scope='global' FOR UPDATE",
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(db_error)?;
        if expected_revision.is_some_and(|expected| expected != revision) {
            return Ok(None);
        }
        let value = sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT data FROM tasks WHERE id=$1 FOR UPDATE",
        )
        .bind(task_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_error)?
        .ok_or_else(|| format!("task does not exist: {task_id}"))?;
        let mut task: TaskRecord = decode_json(value)?;
        task.prerequisite_task_ids = prerequisite_task_ids.clone();
        task.updated_at = now_rfc3339();
        persist_task(&mut *tx, &task).await?;
        sqlx::query("DELETE FROM task_prerequisites WHERE task_id=$1")
            .bind(task_id)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        let now = now_rfc3339();
        let mut ids = prerequisite_task_ids.into_iter().collect::<BTreeSet<_>>();
        ids.remove(task_id);
        let mut records = Vec::with_capacity(ids.len());
        for prerequisite_task_id in ids {
            let record = TaskPrerequisiteRecord {
                task_id: task_id.to_string(),
                prerequisite_task_id,
                created_at: now.clone(),
            };
            sqlx::query("INSERT INTO task_prerequisites(task_id,prerequisite_task_id,created_at,data) VALUES($1,$2,$3,$4)")
                .bind(&record.task_id)
                .bind(&record.prerequisite_task_id)
                .bind(timestamp(&record.created_at)?)
                .bind(json(&record)?)
                .execute(&mut *tx)
                .await
                .map_err(db_error)?;
            records.push(record);
        }
        sqlx::query("UPDATE task_dependency_graph_revisions SET revision=revision+1,updated_at=now() WHERE scope='global'")
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        tx.commit().await.map_err(db_error)?;
        Ok(Some(records))
    }

    pub(in crate::store) async fn delete_task(&self, id: &str) -> Result<bool, String> {
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        let run_ids = sqlx::query_scalar::<_, String>("SELECT id FROM task_runs WHERE task_id=$1")
            .bind(id)
            .fetch_all(&mut *tx)
            .await
            .map_err(db_error)?;
        let deleted = sqlx::query("DELETE FROM tasks WHERE id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        if deleted.rows_affected() == 0 {
            return Ok(false);
        }
        sqlx::query("UPDATE task_dependency_graph_revisions SET revision=revision+1,updated_at=now() WHERE scope='global'")
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        tx.commit().await.map_err(db_error)?;
        let mut cache = self.cancel_requested_runs.write();
        for run_id in run_ids {
            cache.remove(&run_id);
        }
        Ok(true)
    }
}

async fn persist_task<'e, E>(executor: E, task: &TaskRecord) -> Result<(), String>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query(
        "INSERT INTO tasks(id,status,priority,tags,default_model_config_id,project_id,task_profile,creator_user_id,owner_user_id,parent_task_id,source_run_id,source_session_id,source_turn_id,source_user_message_id,schedule_mode,schedule_due_at,created_at,updated_at,deleted_at,data) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20) \
         ON CONFLICT(id) DO UPDATE SET status=EXCLUDED.status,priority=EXCLUDED.priority,tags=EXCLUDED.tags,default_model_config_id=EXCLUDED.default_model_config_id,project_id=EXCLUDED.project_id,task_profile=EXCLUDED.task_profile,creator_user_id=EXCLUDED.creator_user_id,owner_user_id=EXCLUDED.owner_user_id,parent_task_id=EXCLUDED.parent_task_id,source_run_id=EXCLUDED.source_run_id,source_session_id=EXCLUDED.source_session_id,source_turn_id=EXCLUDED.source_turn_id,source_user_message_id=EXCLUDED.source_user_message_id,schedule_mode=EXCLUDED.schedule_mode,schedule_due_at=EXCLUDED.schedule_due_at,updated_at=EXCLUDED.updated_at,deleted_at=EXCLUDED.deleted_at,data=EXCLUDED.data",
    )
    .bind(&task.id)
    .bind(enum_text(&task.status)?)
    .bind(task.priority)
    .bind(&task.tags)
    .bind(&task.default_model_config_id)
    .bind(&task.project_id)
    .bind(&task.task_profile)
    .bind(&task.creator_user_id)
    .bind(&task.owner_user_id)
    .bind(&task.parent_task_id)
    .bind(&task.source_run_id)
    .bind(&task.source_session_id)
    .bind(&task.source_turn_id)
    .bind(&task.source_user_message_id)
    .bind(enum_text(&task.schedule.mode)?)
    .bind(optional_timestamp(task.schedule.next_run_at.as_deref())?)
    .bind(timestamp(&task.created_at)?)
    .bind(timestamp(&task.updated_at)?)
    .bind(optional_timestamp(task.deleted_at.as_deref())?)
    .bind(json(task)?)
    .execute(executor)
    .await
    .map(|_| ())
    .map_err(db_error)
}

async fn load_prerequisites(
    pool: &chatos_postgres::PgPool,
    clause: &str,
    values: &[&str],
) -> Result<Vec<TaskPrerequisiteRecord>, String> {
    debug_assert_eq!(clause, "WHERE task_id=$1");
    let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM task_prerequisites WHERE task_id=$1 ORDER BY prerequisite_task_id",
    )
    .bind(values[0])
    .fetch_all(pool)
    .await
    .map_err(db_error)?;
    rows.into_iter().map(decode_json).collect()
}
