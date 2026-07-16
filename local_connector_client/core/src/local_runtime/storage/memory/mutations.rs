// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use uuid::Uuid;

use crate::local_now_rfc3339;

use super::super::{CreateLocalMemorySummaryInput, LocalDatabase, LocalMemorySummaryRecord};

impl LocalDatabase {
    pub(crate) async fn create_memory_summary(
        &self,
        input: CreateLocalMemorySummaryInput,
    ) -> Result<LocalMemorySummaryRecord> {
        let summary_id = format!("lc_summary_{}", Uuid::new_v4());
        let now = local_now_rfc3339();
        let result = sqlx::query(
            r#"
            INSERT INTO memory_summaries (
                id, session_id, summary_text, summary_model, trigger_type,
                source_start_message_id, source_end_message_id,
                source_message_count, source_estimated_tokens, level,
                status, created_at, updated_at
            )
            SELECT ?, sessions.id, ?, ?, ?, ?, ?, ?, ?, ?, 'completed', ?, ?
            FROM sessions
            WHERE sessions.id = ? AND sessions.owner_user_id = ?
            "#,
        )
        .bind(summary_id.as_str())
        .bind(input.summary_text.as_str())
        .bind(input.summary_model.as_str())
        .bind(input.trigger_type.as_str())
        .bind(input.source_start_message_id.as_deref())
        .bind(input.source_end_message_id.as_deref())
        .bind(input.source_message_count.max(0))
        .bind(input.source_estimated_tokens.max(0))
        .bind(input.level.max(0))
        .bind(now.as_str())
        .bind(now.as_str())
        .bind(input.session_id.as_str())
        .bind(input.owner_user_id.as_str())
        .execute(self.pool())
        .await
        .context("create local memory summary")?;
        if result.rows_affected() != 1 {
            return Err(anyhow::anyhow!("local memory session is not available"));
        }
        sqlx::query_as::<_, LocalMemorySummaryRecord>(
            r#"
            SELECT id, session_id, summary_text, summary_model, trigger_type,
                   source_start_message_id, source_end_message_id,
                   source_message_count, source_estimated_tokens, level,
                   status, error_message, created_at, updated_at
            FROM memory_summaries
            WHERE id = ?
            "#,
        )
        .bind(summary_id)
        .fetch_one(self.pool())
        .await
        .context("load created local memory summary")
    }

    pub(crate) async fn delete_memory_summary(
        &self,
        owner_user_id: &str,
        session_id: &str,
        summary_id: &str,
    ) -> Result<bool> {
        let result = sqlx::query(
            r#"
            DELETE FROM memory_summaries
            WHERE id = ? AND session_id = ? AND EXISTS (
                SELECT 1 FROM sessions
                WHERE sessions.id = memory_summaries.session_id
                  AND sessions.owner_user_id = ?
            )
            "#,
        )
        .bind(summary_id)
        .bind(session_id)
        .bind(owner_user_id)
        .execute(self.pool())
        .await
        .context("delete local memory summary")?;
        Ok(result.rows_affected() == 1)
    }

    pub(crate) async fn clear_memory_summaries(
        &self,
        owner_user_id: &str,
        session_id: &str,
    ) -> Result<i64> {
        let result = sqlx::query(
            r#"
            DELETE FROM memory_summaries
            WHERE session_id = ? AND EXISTS (
                SELECT 1 FROM sessions
                WHERE sessions.id = memory_summaries.session_id
                  AND sessions.owner_user_id = ?
            )
            "#,
        )
        .bind(session_id)
        .bind(owner_user_id)
        .execute(self.pool())
        .await
        .context("clear local memory summaries")?;
        Ok(result.rows_affected() as i64)
    }
}
