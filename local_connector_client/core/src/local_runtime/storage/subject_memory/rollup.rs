// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use uuid::Uuid;

use crate::local_now_rfc3339;

use super::super::{
    LocalDatabase, LocalSubjectMemoryRecord, LocalSubjectMemoryRollupPlan,
    SaveLocalSubjectMemoryRollupInput,
};

impl LocalDatabase {
    pub(crate) async fn prepare_subject_memory_rollup(
        &self,
        owner_user_id: &str,
        subject_type: &str,
        subject_id: &str,
        project_id: &str,
        recall_limit: i64,
    ) -> Result<Option<LocalSubjectMemoryRollupPlan>> {
        let existing_rollup = self
            .get_subject_memory(
                owner_user_id,
                subject_type,
                subject_id,
                project_id,
                "rollup",
            )
            .await?;
        let raw = sqlx::query_as::<_, LocalSubjectMemoryRecord>(
            r#"
            SELECT id, subject_type, subject_id, project_id, recall_key,
                   recall_text, source_session_id, source_summary_id, level,
                   confidence, last_seen_at, created_at, updated_at
            FROM subject_memories
            WHERE owner_user_id = ? AND subject_type = ? AND subject_id = ?
              AND project_id = ? AND recall_key <> 'rollup'
            ORDER BY updated_at ASC, id ASC
            LIMIT 1000
            "#,
        )
        .bind(owner_user_id)
        .bind(subject_type)
        .bind(subject_id)
        .bind(project_id)
        .fetch_all(self.pool())
        .await
        .context("load local subject memory rollup candidates")?;
        let recall_limit = recall_limit.clamp(2, 50) as usize;
        let total = raw.len() + usize::from(existing_rollup.is_some());
        if total <= recall_limit {
            return Ok(None);
        }
        let candidate_count = if existing_rollup.is_some() {
            total - recall_limit
        } else {
            total - recall_limit + 1
        };
        Ok(Some(LocalSubjectMemoryRollupPlan {
            existing_rollup,
            candidates: raw.into_iter().take(candidate_count.max(1)).collect(),
        }))
    }

    pub(crate) async fn save_subject_memory_rollup(
        &self,
        input: SaveLocalSubjectMemoryRollupInput,
    ) -> Result<LocalSubjectMemoryRecord> {
        let id = format!("lc_recall_{}", Uuid::new_v4());
        let now = local_now_rfc3339();
        let mut transaction = self
            .begin_write()
            .await
            .context("begin local recall rollup")?;
        for candidate_id in &input.candidate_ids {
            sqlx::query(
                "DELETE FROM subject_memories WHERE id = ? AND owner_user_id = ? AND recall_key <> 'rollup'",
            )
            .bind(candidate_id)
            .bind(input.owner_user_id.as_str())
            .execute(&mut *transaction)
            .await
            .context("delete rolled local subject memory")?;
        }
        sqlx::query(
            r#"
            INSERT INTO subject_memories (
                id, owner_user_id, subject_type, subject_id, project_id,
                recall_key, recall_text, source_session_id, source_summary_id,
                level, confidence, last_seen_at, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, 'rollup', ?, ?, ?, ?, NULL, ?, ?, ?)
            ON CONFLICT(owner_user_id, subject_type, subject_id, project_id, recall_key)
            DO UPDATE SET
                recall_text = excluded.recall_text,
                source_session_id = excluded.source_session_id,
                source_summary_id = excluded.source_summary_id,
                level = excluded.level,
                last_seen_at = excluded.last_seen_at,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(id)
        .bind(input.owner_user_id.as_str())
        .bind(input.subject_type.as_str())
        .bind(input.subject_id.as_str())
        .bind(input.project_id.as_str())
        .bind(input.recall_text.as_str())
        .bind(input.source_session_id.as_str())
        .bind(input.source_summary_id.as_str())
        .bind(input.level.max(1))
        .bind(now.as_str())
        .bind(now.as_str())
        .bind(now.as_str())
        .execute(&mut *transaction)
        .await
        .context("upsert local subject memory rollup")?;
        transaction
            .commit()
            .await
            .context("commit local recall rollup")?;
        self.get_subject_memory(
            input.owner_user_id.as_str(),
            input.subject_type.as_str(),
            input.subject_id.as_str(),
            input.project_id.as_str(),
            "rollup",
        )
        .await?
        .context("local subject memory rollup was not persisted")
    }
}
