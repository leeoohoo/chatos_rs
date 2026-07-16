// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use std::collections::HashSet;

use super::super::{LocalDatabase, LocalSubjectMemoryRecord};

impl LocalDatabase {
    pub(crate) async fn list_subject_memories_for_session(
        &self,
        owner_user_id: &str,
        session_id: &str,
        limit: i64,
    ) -> Result<Vec<LocalSubjectMemoryRecord>> {
        let limit = limit.clamp(1, 50);
        let session = self
            .get_session(session_id, owner_user_id)
            .await?
            .context("local subject memory session was not found")?;
        let agent_id = session.selected_agent_id.as_deref().unwrap_or("");
        let records = sqlx::query_as::<_, LocalSubjectMemoryRecord>(
            r#"
            SELECT id, subject_type, subject_id, project_id, recall_key,
                   recall_text, source_session_id, source_summary_id, level,
                   confidence, last_seen_at, created_at, updated_at
            FROM subject_memories
            WHERE owner_user_id = ? AND project_id = ? AND source_session_id <> ?
              AND (
                  (subject_type = 'project' AND subject_id = ?)
                  OR (subject_type = 'agent' AND subject_id = ?)
              )
            ORDER BY CASE subject_type WHEN 'agent' THEN 0 ELSE 1 END,
                     updated_at DESC, id DESC
            LIMIT ?
            "#,
        )
        .bind(owner_user_id)
        .bind(session.project_id.as_str())
        .bind(session_id)
        .bind(session.project_id.as_str())
        .bind(agent_id)
        .bind(limit * 2)
        .fetch_all(self.pool())
        .await
        .context("list local subject memories")?;
        let mut seen_summaries = HashSet::new();
        Ok(records
            .into_iter()
            .filter(|record| {
                let dedupe_key = if record.recall_key == "rollup" {
                    format!("rollup:{}:{}", record.subject_type, record.id)
                } else {
                    record.source_summary_id.clone()
                };
                seen_summaries.insert(dedupe_key)
            })
            .take(limit as usize)
            .collect())
    }

    pub(super) async fn get_subject_memory(
        &self,
        owner_user_id: &str,
        subject_type: &str,
        subject_id: &str,
        project_id: &str,
        recall_key: &str,
    ) -> Result<Option<LocalSubjectMemoryRecord>> {
        sqlx::query_as::<_, LocalSubjectMemoryRecord>(
            r#"
            SELECT id, subject_type, subject_id, project_id, recall_key,
                   recall_text, source_session_id, source_summary_id, level,
                   confidence, last_seen_at, created_at, updated_at
            FROM subject_memories
            WHERE owner_user_id = ? AND subject_type = ? AND subject_id = ?
              AND project_id = ? AND recall_key = ?
            "#,
        )
        .bind(owner_user_id)
        .bind(subject_type)
        .bind(subject_id)
        .bind(project_id)
        .bind(recall_key)
        .fetch_optional(self.pool())
        .await
        .context("get local subject memory")
    }
}
