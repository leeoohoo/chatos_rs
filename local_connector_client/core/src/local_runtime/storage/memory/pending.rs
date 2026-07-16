// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};

use super::super::{LocalDatabase, LocalMessageRecord};

impl LocalDatabase {
    pub(crate) async fn pending_memory_messages(
        &self,
        owner_user_id: &str,
        session_id: &str,
        limit: i64,
    ) -> Result<Vec<LocalMessageRecord>> {
        let summary = self
            .latest_memory_summary(owner_user_id, session_id)
            .await?;
        let after_sequence = self
            .summary_end_sequence(session_id, summary.as_ref())
            .await?;
        self.list_summarizable_messages_after(owner_user_id, session_id, after_sequence, limit)
            .await
    }

    pub(crate) async fn count_pending_memory_messages(
        &self,
        owner_user_id: &str,
        session_id: &str,
    ) -> Result<i64> {
        Ok(self
            .pending_memory_stats(owner_user_id, session_id)
            .await?
            .0)
    }

    pub(crate) async fn pending_memory_stats(
        &self,
        owner_user_id: &str,
        session_id: &str,
    ) -> Result<(i64, i64)> {
        let summary = self
            .latest_memory_summary(owner_user_id, session_id)
            .await?;
        let after_sequence = self
            .summary_end_sequence(session_id, summary.as_ref())
            .await?;
        sqlx::query_as::<_, (i64, i64)>(
            r#"
            SELECT COUNT(*), COALESCE(SUM(LENGTH(messages.content)), 0)
            FROM messages
            INNER JOIN sessions ON sessions.id = messages.session_id
            LEFT JOIN turns ON turns.id = messages.turn_id
            WHERE messages.session_id = ? AND sessions.owner_user_id = ?
              AND messages.sequence_no > ?
              AND (messages.turn_id IS NULL OR turns.status = 'completed')
            "#,
        )
        .bind(session_id)
        .bind(owner_user_id)
        .bind(after_sequence)
        .fetch_one(self.pool())
        .await
        .context("load pending local memory statistics")
    }

    async fn list_summarizable_messages_after(
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
            LEFT JOIN turns ON turns.id = messages.turn_id
            WHERE messages.session_id = ? AND sessions.owner_user_id = ?
              AND messages.sequence_no > ?
              AND (messages.turn_id IS NULL OR turns.status = 'completed')
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
        .context("list summarizable local memory messages")
    }
}
