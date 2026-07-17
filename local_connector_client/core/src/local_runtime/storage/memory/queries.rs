// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};

use super::super::{
    LocalDatabase, LocalMemoryContext, LocalMemorySummaryRecord, LocalMessageRecord,
};

impl LocalDatabase {
    pub(crate) async fn list_memory_summaries(
        &self,
        owner_user_id: &str,
        session_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<LocalMemorySummaryRecord>> {
        sqlx::query_as::<_, LocalMemorySummaryRecord>(
            r#"
            SELECT memory_summaries.id, memory_summaries.session_id,
                   memory_summaries.summary_text, memory_summaries.summary_model,
                   memory_summaries.trigger_type,
                   memory_summaries.source_start_message_id,
                   memory_summaries.source_end_message_id,
                   memory_summaries.source_message_count,
                   memory_summaries.source_estimated_tokens,
                   memory_summaries.level, memory_summaries.status,
                   memory_summaries.error_message, memory_summaries.created_at,
                   memory_summaries.updated_at
            FROM memory_summaries
            INNER JOIN sessions ON sessions.id = memory_summaries.session_id
            WHERE memory_summaries.session_id = ? AND sessions.owner_user_id = ?
            ORDER BY memory_summaries.created_at DESC, memory_summaries.id DESC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(session_id)
        .bind(owner_user_id)
        .bind(limit.clamp(1, 500))
        .bind(offset.max(0))
        .fetch_all(self.pool())
        .await
        .context("list local memory summaries")
    }

    pub(crate) async fn latest_memory_summary(
        &self,
        owner_user_id: &str,
        session_id: &str,
    ) -> Result<Option<LocalMemorySummaryRecord>> {
        Ok(self
            .list_memory_summaries(owner_user_id, session_id, 1, 0)
            .await?
            .into_iter()
            .next())
    }

    pub(crate) async fn load_memory_context(
        &self,
        owner_user_id: &str,
        session_id: &str,
        recall_limit: i64,
    ) -> Result<LocalMemoryContext> {
        let summary = self
            .latest_memory_summary(owner_user_id, session_id)
            .await?;
        let after_sequence = self
            .summary_end_sequence(session_id, summary.as_ref())
            .await?;
        let messages = self
            .list_messages_after(owner_user_id, session_id, after_sequence, 500)
            .await?;
        let recalls = self
            .list_subject_memories_for_session(owner_user_id, session_id, recall_limit)
            .await?;
        Ok(LocalMemoryContext {
            summary,
            recalls,
            messages,
        })
    }

    pub(super) async fn summary_end_sequence(
        &self,
        session_id: &str,
        summary: Option<&LocalMemorySummaryRecord>,
    ) -> Result<i64> {
        let Some(message_id) = summary.and_then(|item| item.source_end_message_id.as_deref())
        else {
            return Ok(0);
        };
        Ok(sqlx::query_scalar::<_, i64>(
            "SELECT sequence_no FROM messages WHERE session_id = ? AND id = ?",
        )
        .bind(session_id)
        .bind(message_id)
        .fetch_optional(self.pool())
        .await
        .context("load local memory summary end sequence")?
        .unwrap_or_default())
    }

    async fn list_messages_after(
        &self,
        owner_user_id: &str,
        session_id: &str,
        after_sequence: i64,
        limit: i64,
    ) -> Result<Vec<LocalMessageRecord>> {
        sqlx::query_as::<_, LocalMessageRecord>(
            r#"
            SELECT messages.id, messages.session_id, messages.turn_id,
                   messages.sequence_no, messages.role, messages.content,
                   messages.reasoning, messages.tool_calls_json,
                   messages.tool_call_id, messages.metadata_json,
                   messages.created_at
            FROM messages
            INNER JOIN sessions ON sessions.id = messages.session_id
            WHERE messages.session_id = ? AND sessions.owner_user_id = ?
              AND messages.sequence_no > ?
            ORDER BY messages.sequence_no ASC
            LIMIT ?
            "#,
        )
        .bind(session_id)
        .bind(owner_user_id)
        .bind(after_sequence.max(0))
        .bind(limit.clamp(1, 2_000))
        .fetch_all(self.pool())
        .await
        .context("list local messages after memory summary")
    }
}
