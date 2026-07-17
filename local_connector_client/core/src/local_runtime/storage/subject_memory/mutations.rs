// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use uuid::Uuid;

use crate::local_now_rfc3339;

use super::super::{
    LocalDatabase, LocalMemorySummaryRecord, LocalSessionRecord, LocalSubjectMemoryRecord,
};

impl LocalDatabase {
    pub(crate) async fn upsert_subject_memories_for_summary(
        &self,
        owner_user_id: &str,
        session: &LocalSessionRecord,
        summary: &LocalMemorySummaryRecord,
    ) -> Result<Vec<LocalSubjectMemoryRecord>> {
        let mut records = Vec::new();
        if let Some(record) = self
            .upsert_subject_memory(
                owner_user_id,
                "project",
                session.project_id.as_str(),
                session,
                summary,
            )
            .await?
        {
            records.push(record);
        }
        if let Some(agent_id) = session
            .selected_agent_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if let Some(record) = self
                .upsert_subject_memory(owner_user_id, "agent", agent_id, session, summary)
                .await?
            {
                records.push(record);
            }
        }
        Ok(records)
    }

    async fn upsert_subject_memory(
        &self,
        owner_user_id: &str,
        subject_type: &str,
        subject_id: &str,
        session: &LocalSessionRecord,
        summary: &LocalMemorySummaryRecord,
    ) -> Result<Option<LocalSubjectMemoryRecord>> {
        let forgotten = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*) FROM subject_memory_forget_markers
            WHERE owner_user_id = ? AND subject_type = ? AND subject_id = ?
              AND project_id = ? AND recall_key = ?
            "#,
        )
        .bind(owner_user_id)
        .bind(subject_type)
        .bind(subject_id)
        .bind(session.project_id.as_str())
        .bind(format!("session:{}", session.id))
        .fetch_one(self.pool())
        .await
        .context("check local recall forget marker")?;
        if forgotten > 0 {
            return Ok(None);
        }
        let id = format!("lc_recall_{}", Uuid::new_v4());
        let recall_key = format!("session:{}", session.id);
        let now = local_now_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO subject_memories (
                id, owner_user_id, subject_type, subject_id, project_id,
                recall_key, recall_text, source_session_id, source_summary_id,
                level, confidence, last_seen_at, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0, NULL, ?, ?, ?)
            ON CONFLICT(owner_user_id, subject_type, subject_id, project_id, recall_key)
            DO UPDATE SET
                recall_text = excluded.recall_text,
                source_summary_id = excluded.source_summary_id,
                last_seen_at = excluded.last_seen_at,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(id.as_str())
        .bind(owner_user_id)
        .bind(subject_type)
        .bind(subject_id)
        .bind(session.project_id.as_str())
        .bind(recall_key.as_str())
        .bind(summary.summary_text.as_str())
        .bind(session.id.as_str())
        .bind(summary.id.as_str())
        .bind(now.as_str())
        .bind(now.as_str())
        .bind(now.as_str())
        .execute(self.pool())
        .await
        .context("upsert local subject memory")?;
        Ok(Some(
            self.get_subject_memory(
                owner_user_id,
                subject_type,
                subject_id,
                session.project_id.as_str(),
                recall_key.as_str(),
            )
            .await?
            .context("local subject memory was not persisted")?,
        ))
    }
}
