// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use uuid::Uuid;

use crate::local_now_rfc3339;

use super::{CreateLocalSessionInput, LocalDatabase, LocalSessionRecord};

impl LocalDatabase {
    pub(crate) async fn create_session(
        &self,
        input: CreateLocalSessionInput,
    ) -> Result<LocalSessionRecord> {
        self.create_session_with_contact(input, None).await
    }

    pub(crate) async fn create_session_with_contact(
        &self,
        input: CreateLocalSessionInput,
        contact_id: Option<String>,
    ) -> Result<LocalSessionRecord> {
        let project_exists = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM local_projects WHERE project_id = ? AND owner_user_id = ?",
        )
        .bind(input.project_id.as_str())
        .bind(input.owner_user_id.as_str())
        .fetch_one(self.pool())
        .await
        .context("validate local session project")?;
        if project_exists == 0 {
            return Err(anyhow::anyhow!("local runtime project is not registered"));
        }

        let session_id = format!("lc_session_{}", Uuid::new_v4());
        let now = local_now_rfc3339();
        let mut transaction = self.begin_write().await.context("begin local session")?;
        sqlx::query(
            r#"
            INSERT INTO sessions (
                id, project_id, owner_user_id, title, contact_id, selected_model_id,
                selected_agent_id, status, message_count, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, 'active', 0, ?, ?)
            "#,
        )
        .bind(session_id.as_str())
        .bind(input.project_id.as_str())
        .bind(input.owner_user_id.as_str())
        .bind(input.title.as_str())
        .bind(contact_id.as_deref())
        .bind(input.selected_model_id.as_deref())
        .bind(input.selected_agent_id.as_deref())
        .bind(now.as_str())
        .bind(now.as_str())
        .execute(&mut *transaction)
        .await
        .context("create local runtime session")?;
        sqlx::query(
            r#"
            INSERT INTO session_runtime_settings (
                session_id, selected_model_id, reasoning_enabled, plan_mode_enabled,
                mcp_enabled, enabled_mcp_ids_json, selected_skill_ids_json,
                auto_create_task, created_at, updated_at
            ) VALUES (?, ?, 0, 0, 1, '[]', '[]', 0, ?, ?)
            "#,
        )
        .bind(session_id.as_str())
        .bind(input.selected_model_id.as_deref())
        .bind(now.as_str())
        .bind(now.as_str())
        .execute(&mut *transaction)
        .await
        .context("create local session runtime settings")?;
        transaction.commit().await.context("commit local session")?;

        self.get_session(session_id.as_str(), input.owner_user_id.as_str())
            .await?
            .ok_or_else(|| anyhow::anyhow!("local runtime session was not persisted"))
    }

    pub(crate) async fn get_session(
        &self,
        session_id: &str,
        owner_user_id: &str,
    ) -> Result<Option<LocalSessionRecord>> {
        sqlx::query_as::<_, LocalSessionRecord>(
            r#"
            SELECT id, project_id, owner_user_id, title, contact_id,
                   selected_model_id, selected_agent_id, status, message_count, created_at, updated_at
            FROM sessions
            WHERE id = ? AND owner_user_id = ?
            "#,
        )
        .bind(session_id)
        .bind(owner_user_id)
        .fetch_optional(self.pool())
        .await
        .context("get local runtime session")
    }

    pub(crate) async fn list_sessions(
        &self,
        owner_user_id: &str,
        project_id: &str,
    ) -> Result<Vec<LocalSessionRecord>> {
        sqlx::query_as::<_, LocalSessionRecord>(
            r#"
            SELECT id, project_id, owner_user_id, title, contact_id,
                   selected_model_id, selected_agent_id, status, message_count, created_at, updated_at
            FROM sessions
            WHERE owner_user_id = ? AND project_id = ? AND status = 'active'
            ORDER BY updated_at DESC, id ASC
            "#,
        )
        .bind(owner_user_id)
        .bind(project_id)
        .fetch_all(self.pool())
        .await
        .context("list local runtime sessions")
    }
}
