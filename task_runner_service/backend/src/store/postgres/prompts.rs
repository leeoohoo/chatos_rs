// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl PostgresStore {
    pub(in crate::store) async fn list_ask_user_prompts(
        &self,
        task_id: Option<&str>,
        run_id: Option<&str>,
        status: Option<AskUserPromptStatus>,
    ) -> Result<Vec<AskUserPromptRecord>, String> {
        let status = status.map(|value| enum_text(&value)).transpose()?;
        let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT data FROM ask_user_prompts \
             WHERE ($1::text IS NULL OR task_id=$1) AND ($2::text IS NULL OR run_id=$2) \
             AND ($3::text IS NULL OR status=$3) ORDER BY updated_at DESC,id",
        )
        .bind(task_id)
        .bind(run_id)
        .bind(status)
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        rows.into_iter().map(decode_json).collect()
    }

    pub(in crate::store) async fn list_ask_user_prompts_page(
        &self,
        filters: &PromptListFilters,
    ) -> Result<PaginatedResponse<AskUserPromptRecord>, String> {
        let status = filters.status.map(|value| enum_text(&value)).transpose()?;
        let limit = filters.limit.unwrap_or(DEFAULT_PAGE_LIMIT);
        let offset = filters.offset.unwrap_or(0);
        let total: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM ask_user_prompts \
             WHERE ($1::text IS NULL OR task_id=$1) AND ($2::text IS NULL OR run_id=$2) \
             AND ($3::text IS NULL OR status=$3)",
        )
        .bind(filters.task_id.as_deref())
        .bind(filters.run_id.as_deref())
        .bind(status.as_deref())
        .fetch_one(&self.pool)
        .await
        .map_err(db_error)?;
        let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT data FROM ask_user_prompts \
             WHERE ($1::text IS NULL OR task_id=$1) AND ($2::text IS NULL OR run_id=$2) \
             AND ($3::text IS NULL OR status=$3) ORDER BY updated_at DESC,id LIMIT $4 OFFSET $5",
        )
        .bind(filters.task_id.as_deref())
        .bind(filters.run_id.as_deref())
        .bind(status)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .bind(i64::try_from(offset).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        Ok(build_page_response(
            rows.into_iter()
                .map(decode_json)
                .collect::<Result<_, _>>()?,
            usize::try_from(total).map_err(|_| "prompt count exceeds usize".to_string())?,
            limit,
            offset,
        ))
    }

    pub(in crate::store) async fn get_ask_user_prompt(
        &self,
        id: &str,
    ) -> Result<Option<AskUserPromptRecord>, String> {
        sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT data FROM ask_user_prompts WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_error)?
        .map(decode_json)
        .transpose()
    }

    pub(in crate::store) async fn save_ask_user_prompt(
        &self,
        prompt: AskUserPromptRecord,
    ) -> Result<AskUserPromptRecord, String> {
        persist_prompt(&self.pool, &prompt).await?;
        Ok(prompt)
    }

    pub(in crate::store) async fn prune_terminal_ask_user_prompts_before(
        &self,
        cutoff: &str,
        candidate_limit: usize,
    ) -> Result<AskUserPromptPruneResult, String> {
        let cutoff = timestamp(cutoff)?;
        let rows = sqlx::query_scalar::<_, String>(
            "WITH eligible AS (SELECT p.id FROM ask_user_prompts p JOIN task_runs r ON r.id=p.run_id \
             WHERE p.status IN ('submitted','cancelled','timed_out','failed') AND NOT p.resolution_event_pending \
             AND p.updated_at < $1 AND r.status IN ('succeeded','failed','cancelled','blocked') \
             ORDER BY p.updated_at,p.id LIMIT $2 FOR UPDATE OF p SKIP LOCKED), \
             deleted AS (DELETE FROM ask_user_prompts p USING eligible e WHERE p.id=e.id RETURNING p.id) \
             SELECT id FROM deleted",
        )
        .bind(cutoff)
        .bind(i64::try_from(candidate_limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        Ok(AskUserPromptPruneResult {
            eligible_prompts: rows.len(),
            deleted_prompts: rows.len() as u64,
        })
    }

    pub(in crate::store) async fn list_pending_ask_user_resolution_events(
        &self,
        limit: usize,
    ) -> Result<Vec<AskUserPromptRecord>, String> {
        let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT data FROM ask_user_prompts WHERE resolution_event_pending AND status <> 'pending' \
             ORDER BY updated_at,id LIMIT $1",
        )
        .bind(i64::try_from(limit.max(1)).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        rows.into_iter().map(decode_json).collect()
    }

    pub(in crate::store) async fn acknowledge_ask_user_resolution_event(
        &self,
        prompt_id: &str,
    ) -> Result<bool, String> {
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        let value = sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT data FROM ask_user_prompts WHERE id=$1 AND resolution_event_pending FOR UPDATE",
        )
        .bind(prompt_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_error)?;
        let Some(value) = value else {
            return Ok(false);
        };
        let mut prompt: AskUserPromptRecord = decode_json(value)?;
        prompt.resolution_event_pending = false;
        persist_prompt(&mut *tx, &prompt).await?;
        tx.commit().await.map_err(db_error)?;
        Ok(true)
    }

    pub(in crate::store) async fn list_ask_user_prompt_task_counts(
        &self,
        status: Option<AskUserPromptStatus>,
        task_ids: Option<&[String]>,
    ) -> Result<Vec<AskUserPromptTaskCountRecord>, String> {
        if task_ids.is_some_and(<[String]>::is_empty) {
            return Ok(Vec::new());
        }
        let status = status.map(|value| enum_text(&value)).transpose()?;
        let task_ids = task_ids.map(|values| values.to_vec());
        let rows = sqlx::query_as::<_, (String, i64)>(
            "SELECT task_id,count(*) FROM ask_user_prompts WHERE task_id IS NOT NULL \
             AND ($1::text IS NULL OR status=$1) AND ($2::text[] IS NULL OR task_id=ANY($2)) \
             GROUP BY task_id ORDER BY count(*) DESC,task_id",
        )
        .bind(status)
        .bind(task_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        rows.into_iter()
            .map(|(task_id, count)| {
                Ok(AskUserPromptTaskCountRecord {
                    task_id,
                    count: usize::try_from(count)
                        .map_err(|_| "prompt count exceeds usize".to_string())?,
                })
            })
            .collect()
    }
}

async fn persist_prompt<'e, E>(executor: E, prompt: &AskUserPromptRecord) -> Result<(), String>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query(
        "INSERT INTO ask_user_prompts(id,task_id,run_id,status,resolution_event_pending,conversation_id,conversation_turn_id,created_at,updated_at,expires_at,data) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT(id) DO UPDATE SET task_id=EXCLUDED.task_id,run_id=EXCLUDED.run_id,status=EXCLUDED.status,resolution_event_pending=EXCLUDED.resolution_event_pending,conversation_id=EXCLUDED.conversation_id,conversation_turn_id=EXCLUDED.conversation_turn_id,updated_at=EXCLUDED.updated_at,expires_at=EXCLUDED.expires_at,data=EXCLUDED.data",
    )
    .bind(&prompt.id)
    .bind(&prompt.task_id)
    .bind(&prompt.run_id)
    .bind(enum_text(&prompt.status)?)
    .bind(prompt.resolution_event_pending)
    .bind(&prompt.conversation_id)
    .bind(&prompt.conversation_turn_id)
    .bind(timestamp(&prompt.created_at)?)
    .bind(timestamp(&prompt.updated_at)?)
    .bind(optional_timestamp(prompt.expires_at.as_deref())?)
    .bind(json(prompt)?)
    .execute(executor)
    .await
    .map(|_| ())
    .map_err(db_error)
}
