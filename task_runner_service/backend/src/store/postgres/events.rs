// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl PostgresStore {
    pub(in crate::store) fn enqueue_run_event(
        &self,
        event: TaskRunEventRecord,
    ) -> Result<(), String> {
        self.run_event_persist_sender
            .send(event)
            .map_err(|_| "run event persistence queue is closed".to_string())
    }

    pub(in crate::store) async fn claim_pending_run_events(
        &self,
        limit: usize,
    ) -> Result<Vec<(TaskRunEventRecord, String)>, String> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let claim_token = uuid::Uuid::new_v4().to_string();
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        let event_ids = sqlx::query_scalar::<_, String>(
            "WITH candidates AS (\
                 SELECT event_id FROM task_run_event_outbox \
                 WHERE (status='pending' AND available_at <= now()) \
                    OR (status='publishing' AND claim_until <= now()) \
                 ORDER BY available_at,event_id LIMIT $1 FOR UPDATE SKIP LOCKED\
             ) \
             UPDATE task_run_event_outbox o SET status='publishing',claim_token=$2,\
                 claim_until=now()+interval '30 seconds',updated_at=now() \
             FROM candidates c WHERE o.event_id=c.event_id RETURNING o.event_id",
        )
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .bind(&claim_token)
        .fetch_all(&mut *tx)
        .await
        .map_err(db_error)?;
        let values = if event_ids.is_empty() {
            Vec::new()
        } else {
            sqlx::query_scalar::<_, Json<serde_json::Value>>(
                "SELECT data FROM task_run_events WHERE id=ANY($1) ORDER BY created_at,id",
            )
            .bind(&event_ids)
            .fetch_all(&mut *tx)
            .await
            .map_err(db_error)?
        };
        tx.commit().await.map_err(db_error)?;
        values
            .into_iter()
            .map(|value| decode_json(value).map(|event| (event, claim_token.clone())))
            .collect()
    }

    pub(in crate::store) async fn complete_run_event_publish(
        &self,
        event_id: &str,
        claim_token: &str,
    ) -> Result<bool, String> {
        sqlx::query(
            "UPDATE task_run_event_outbox SET status='published',claim_token=NULL,claim_until=NULL,\
             last_error=NULL,updated_at=now() WHERE event_id=$1 AND status='publishing' AND claim_token=$2",
        )
        .bind(event_id)
        .bind(claim_token)
        .execute(&self.pool)
        .await
        .map(|result| result.rows_affected() == 1)
        .map_err(db_error)
    }

    pub(in crate::store) async fn mark_run_event_published(
        &self,
        event_id: &str,
    ) -> Result<(), String> {
        mark_run_event_outbox_published(&self.pool, event_id).await
    }

    pub(in crate::store) async fn fail_run_event_publish(
        &self,
        event_id: &str,
        claim_token: &str,
        error: &str,
    ) -> Result<bool, String> {
        sqlx::query(
            "UPDATE task_run_event_outbox SET \
                 status=CASE WHEN publish_attempts+1 >= 20 THEN 'dead_letter' ELSE 'pending' END,\
                 publish_attempts=publish_attempts+1,\
                 available_at=now()+(LEAST(300,POWER(2,LEAST(8,publish_attempts)))::text || ' seconds')::interval,\
                 claim_token=NULL,claim_until=NULL,last_error=$3,updated_at=now() \
             WHERE event_id=$1 AND status='publishing' AND claim_token=$2",
        )
        .bind(event_id)
        .bind(claim_token)
        .bind(error)
        .execute(&self.pool)
        .await
        .map(|result| result.rows_affected() == 1)
        .map_err(db_error)
    }

    pub(in crate::store) async fn has_run_event_type(
        &self,
        run_id: &str,
        event_type: &str,
    ) -> Result<bool, String> {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM task_run_events WHERE run_id=$1 AND event_type=$2)",
        )
        .bind(run_id)
        .bind(event_type)
        .fetch_one(&self.pool)
        .await
        .map_err(db_error)
    }

    pub(in crate::store) async fn get_run_event_by_type(
        &self,
        run_id: &str,
        event_type: &str,
    ) -> Result<Option<TaskRunEventRecord>, String> {
        load_optional_event(
            &self.pool,
            "SELECT data FROM task_run_events WHERE run_id=$1 AND event_type=$2 ORDER BY created_at,id LIMIT 1",
            run_id,
            Some(event_type),
        )
        .await
    }

    pub(in crate::store) async fn get_run_event(
        &self,
        run_id: &str,
        event_id: &str,
    ) -> Result<Option<TaskRunEventRecord>, String> {
        load_optional_event(
            &self.pool,
            "SELECT data FROM task_run_events WHERE run_id=$1 AND id=$2",
            run_id,
            Some(event_id),
        )
        .await
    }

    pub(in crate::store) async fn list_run_events(
        &self,
        run_id: &str,
    ) -> Result<Vec<TaskRunEventRecord>, String> {
        let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT data FROM task_run_events WHERE run_id=$1 ORDER BY created_at,id",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        rows.into_iter().map(decode_json).collect()
    }

    pub(in crate::store) async fn list_run_events_page(
        &self,
        run_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<TaskRunEventRecord>, usize), String> {
        let total: i64 = sqlx::query_scalar("SELECT count(*) FROM task_run_events WHERE run_id=$1")
            .bind(run_id)
            .fetch_one(&self.pool)
            .await
            .map_err(db_error)?;
        let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT data FROM task_run_events WHERE run_id=$1 ORDER BY created_at,id LIMIT $2 OFFSET $3",
        )
        .bind(run_id)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .bind(i64::try_from(offset).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        Ok((
            rows.into_iter()
                .map(decode_json)
                .collect::<Result<_, _>>()?,
            usize::try_from(total).map_err(|_| "event count exceeds usize".to_string())?,
        ))
    }

    pub(in crate::store) async fn list_run_events_after(
        &self,
        run_id: &str,
        after_created_at: Option<&str>,
        after_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<TaskRunEventRecord>, String> {
        let cursor_time = after_created_at.map(timestamp).transpose()?;
        let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT data FROM task_run_events WHERE run_id=$1 AND \
             ($2::timestamptz IS NULL OR $3::text IS NULL OR (created_at,id) > ($2,$3)) \
             ORDER BY created_at,id LIMIT $4",
        )
        .bind(run_id)
        .bind(cursor_time)
        .bind(after_id)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        rows.into_iter().map(decode_json).collect()
    }

    pub(in crate::store) async fn latest_run_event_cursor(
        &self,
        run_id: &str,
    ) -> Result<Option<(String, String)>, String> {
        let row = sqlx::query_as::<_, (DateTime<Utc>, String)>(
            "SELECT created_at,id FROM task_run_events WHERE run_id=$1 ORDER BY created_at DESC,id DESC LIMIT 1",
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_error)?;
        Ok(row.map(|(created_at, id)| (created_at.to_rfc3339(), id)))
    }

    pub(in crate::store) async fn prune_terminal_run_events_before(
        &self,
        cutoff: &str,
        candidate_limit: usize,
    ) -> Result<RunEventPruneResult, String> {
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        let run_ids = sqlx::query_scalar::<_, String>(
            "SELECT r.id FROM task_runs r \
             WHERE r.status IN ('succeeded','failed','cancelled','blocked') \
             AND EXISTS (SELECT 1 FROM task_run_events e WHERE e.run_id=r.id AND e.created_at < $1) \
             ORDER BY r.id LIMIT $2 FOR UPDATE OF r SKIP LOCKED",
        )
        .bind(timestamp(cutoff)?)
        .bind(i64::try_from(candidate_limit).unwrap_or(i64::MAX))
        .fetch_all(&mut *tx)
        .await
        .map_err(db_error)?;
        let deleted = if run_ids.is_empty() {
            0
        } else {
            sqlx::query("DELETE FROM task_run_events WHERE run_id=ANY($1) AND created_at < $2")
                .bind(&run_ids)
                .bind(timestamp(cutoff)?)
                .execute(&mut *tx)
                .await
                .map_err(db_error)?
                .rows_affected()
        };
        tx.commit().await.map_err(db_error)?;
        Ok(RunEventPruneResult {
            eligible_runs: run_ids.len(),
            deleted_events: deleted,
        })
    }

    pub(in crate::store) async fn append_run_event(
        &self,
        event: TaskRunEventRecord,
    ) -> Result<(), String> {
        persist_event(&self.pool, &event).await
    }
}

async fn load_optional_event(
    pool: &chatos_postgres::PgPool,
    sql: &str,
    first: &str,
    second: Option<&str>,
) -> Result<Option<TaskRunEventRecord>, String> {
    let value = sqlx::query_scalar::<_, Json<serde_json::Value>>(sql)
        .bind(first)
        .bind(second.unwrap_or_default())
        .fetch_optional(pool)
        .await
        .map_err(db_error)?;
    value.map(decode_json).transpose()
}

pub(super) async fn persist_event(
    pool: &chatos_postgres::PgPool,
    event: &TaskRunEventRecord,
) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(db_error)?;
    sqlx::query(
        "INSERT INTO task_run_events(id,run_id,event_type,created_at,data) VALUES($1,$2,$3,$4,$5) \
         ON CONFLICT(id) DO UPDATE SET run_id=EXCLUDED.run_id,event_type=EXCLUDED.event_type,created_at=EXCLUDED.created_at,data=EXCLUDED.data",
    )
    .bind(&event.id)
    .bind(&event.run_id)
    .bind(&event.event_type)
    .bind(timestamp(&event.created_at)?)
    .bind(json(event)?)
    .execute(&mut *tx)
    .await
    .map_err(db_error)?;
    sqlx::query(
        "INSERT INTO task_run_event_outbox(event_id,run_id,status,available_at,publish_attempts,created_at,updated_at) \
         VALUES($1,$2,'pending',now(),0,now(),now()) ON CONFLICT(event_id) DO NOTHING",
    )
    .bind(&event.id)
    .bind(&event.run_id)
    .execute(&mut *tx)
    .await
    .map_err(db_error)?;
    tx.commit().await.map_err(db_error)
}

pub(super) async fn mark_run_event_outbox_published(
    pool: &chatos_postgres::PgPool,
    event_id: &str,
) -> Result<(), String> {
    sqlx::query(
        "UPDATE task_run_event_outbox SET status='published',claim_token=NULL,claim_until=NULL,\
         last_error=NULL,updated_at=now() WHERE event_id=$1 AND status <> 'published'",
    )
    .bind(event_id)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(db_error)
}
