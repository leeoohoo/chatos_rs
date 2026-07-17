// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use serde::Serialize;
use sqlx::FromRow;

use crate::local_now_rfc3339;

use super::LocalDatabase;

#[derive(Debug, Clone, Serialize, FromRow)]
pub(crate) struct LocalAskUserPromptRecord {
    pub(crate) id: String,
    pub(crate) session_id: String,
    pub(crate) turn_id: String,
    pub(crate) owner_user_id: String,
    pub(crate) tool_call_id: Option<String>,
    pub(crate) kind: String,
    pub(crate) status: String,
    pub(crate) prompt_json: String,
    pub(crate) response_json: Option<String>,
    pub(crate) expires_at: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

impl LocalDatabase {
    pub(crate) async fn create_ask_user_prompt(
        &self,
        record: &LocalAskUserPromptRecord,
    ) -> Result<()> {
        let valid_turn = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*) FROM turns
            INNER JOIN sessions ON sessions.id = turns.session_id
            WHERE turns.id = ? AND turns.session_id = ? AND sessions.owner_user_id = ?
            "#,
        )
        .bind(record.turn_id.as_str())
        .bind(record.session_id.as_str())
        .bind(record.owner_user_id.as_str())
        .fetch_one(self.pool())
        .await
        .context("validate local Ask User turn")?;
        if valid_turn == 0 {
            return Err(anyhow::anyhow!("local Ask User turn is not available"));
        }
        sqlx::query(
            r#"
            INSERT INTO ask_user_prompts (
                id, session_id, turn_id, owner_user_id, tool_call_id, kind,
                status, prompt_json, response_json, expires_at, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(record.id.as_str())
        .bind(record.session_id.as_str())
        .bind(record.turn_id.as_str())
        .bind(record.owner_user_id.as_str())
        .bind(record.tool_call_id.as_deref())
        .bind(record.kind.as_str())
        .bind(record.status.as_str())
        .bind(record.prompt_json.as_str())
        .bind(record.response_json.as_deref())
        .bind(record.expires_at.as_deref())
        .bind(record.created_at.as_str())
        .bind(record.updated_at.as_str())
        .execute(self.pool())
        .await
        .context("create local Ask User prompt")?;
        Ok(())
    }

    pub(crate) async fn get_ask_user_prompt(
        &self,
        owner_user_id: &str,
        prompt_id: &str,
    ) -> Result<Option<LocalAskUserPromptRecord>> {
        sqlx::query_as::<_, LocalAskUserPromptRecord>(
            "SELECT * FROM ask_user_prompts WHERE id = ? AND owner_user_id = ?",
        )
        .bind(prompt_id)
        .bind(owner_user_id)
        .fetch_optional(self.pool())
        .await
        .context("get local Ask User prompt")
    }

    pub(crate) async fn list_ask_user_prompts(
        &self,
        owner_user_id: &str,
        session_id: &str,
        include_pending: bool,
        limit: usize,
    ) -> Result<Vec<LocalAskUserPromptRecord>> {
        sqlx::query_as::<_, LocalAskUserPromptRecord>(
            r#"
            SELECT * FROM ask_user_prompts
            WHERE owner_user_id = ? AND session_id = ?
              AND (? OR status != 'pending')
            ORDER BY created_at DESC LIMIT ?
            "#,
        )
        .bind(owner_user_id)
        .bind(session_id)
        .bind(include_pending)
        .bind(limit.clamp(1, 500) as i64)
        .fetch_all(self.pool())
        .await
        .context("list local Ask User prompts")
    }

    pub(crate) async fn resolve_ask_user_prompt(
        &self,
        owner_user_id: &str,
        prompt_id: &str,
        status: &str,
        response_json: &str,
    ) -> Result<Option<LocalAskUserPromptRecord>> {
        let now = local_now_rfc3339();
        let result = sqlx::query(
            r#"
            UPDATE ask_user_prompts SET status = ?, response_json = ?, updated_at = ?
            WHERE id = ? AND owner_user_id = ? AND status = 'pending'
            "#,
        )
        .bind(status)
        .bind(response_json)
        .bind(now)
        .bind(prompt_id)
        .bind(owner_user_id)
        .execute(self.pool())
        .await
        .context("resolve local Ask User prompt")?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }
        self.get_ask_user_prompt(owner_user_id, prompt_id).await
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
