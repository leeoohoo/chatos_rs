// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use serde_json::{Map, Value};

use super::LocalDatabase;

impl LocalDatabase {
    pub(crate) async fn mark_guidance_applied(
        &self,
        owner_user_id: &str,
        message_id: &str,
        applied_at: &str,
    ) -> Result<()> {
        let raw_metadata = sqlx::query_scalar::<_, Option<String>>(
            r#"
            SELECT messages.metadata_json
            FROM messages
            INNER JOIN sessions ON sessions.id = messages.session_id
            WHERE messages.id = ? AND sessions.owner_user_id = ?
            "#,
        )
        .bind(message_id)
        .bind(owner_user_id)
        .fetch_optional(self.pool())
        .await
        .context("load local guidance metadata")?
        .flatten();
        let Some(raw_metadata) = raw_metadata else {
            return Ok(());
        };
        let mut metadata = match serde_json::from_str::<Value>(raw_metadata.as_str()) {
            Ok(Value::Object(metadata)) => metadata,
            _ => Map::new(),
        };
        let guidance = metadata
            .entry("runtime_guidance".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if let Some(guidance) = guidance.as_object_mut() {
            guidance.insert("status".to_string(), Value::String("applied".to_string()));
            guidance.insert(
                "applied_at".to_string(),
                Value::String(applied_at.to_string()),
            );
        }
        sqlx::query(
            r#"
            UPDATE messages
            SET metadata_json = ?
            WHERE id = ? AND EXISTS (
                SELECT 1 FROM sessions
                WHERE sessions.id = messages.session_id AND sessions.owner_user_id = ?
            )
            "#,
        )
        .bind(Value::Object(metadata).to_string())
        .bind(message_id)
        .bind(owner_user_id)
        .execute(self.pool())
        .await
        .context("mark local guidance applied")?;
        Ok(())
    }
}
