// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};

use crate::local_now_rfc3339;

use super::{LocalDatabase, LocalRuntimeSettingsRecord, SaveLocalRuntimeSettingsInput};

impl LocalDatabase {
    pub(crate) async fn get_runtime_settings(
        &self,
        owner_user_id: &str,
        session_id: &str,
    ) -> Result<Option<LocalRuntimeSettingsRecord>> {
        sqlx::query_as::<_, LocalRuntimeSettingsRecord>(
            r#"
            SELECT settings.session_id, settings.selected_model_id,
                   settings.selected_model_name, settings.selected_thinking_level,
                   settings.workspace_root, settings.reasoning_enabled,
                   settings.plan_mode_enabled, settings.mcp_enabled,
                   settings.enabled_mcp_ids_json, settings.selected_skill_ids_json,
                   settings.auto_create_task, settings.memory_auto_summary_enabled,
                   settings.memory_summary_message_threshold,
                   settings.memory_summary_character_threshold,
                   settings.memory_recall_limit,
                   settings.created_at, settings.updated_at
            FROM session_runtime_settings AS settings
            INNER JOIN sessions ON sessions.id = settings.session_id
            WHERE settings.session_id = ? AND sessions.owner_user_id = ?
            "#,
        )
        .bind(session_id)
        .bind(owner_user_id)
        .fetch_optional(self.pool())
        .await
        .context("get local runtime settings")
    }

    pub(crate) async fn save_runtime_settings(
        &self,
        owner_user_id: &str,
        input: SaveLocalRuntimeSettingsInput,
    ) -> Result<LocalRuntimeSettingsRecord> {
        let session_exists = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sessions WHERE id = ? AND owner_user_id = ?",
        )
        .bind(input.session_id.as_str())
        .bind(owner_user_id)
        .fetch_one(self.pool())
        .await
        .context("validate local runtime settings session")?;
        if session_exists == 0 {
            return Err(anyhow::anyhow!("local runtime session is not available"));
        }

        let now = local_now_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO session_runtime_settings (
                session_id, selected_model_id, selected_model_name,
                selected_thinking_level, workspace_root, reasoning_enabled,
                plan_mode_enabled, mcp_enabled, enabled_mcp_ids_json,
                selected_skill_ids_json, auto_create_task,
                memory_auto_summary_enabled, memory_summary_message_threshold,
                memory_summary_character_threshold, memory_recall_limit,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(session_id) DO UPDATE SET
                selected_model_id = excluded.selected_model_id,
                selected_model_name = excluded.selected_model_name,
                selected_thinking_level = excluded.selected_thinking_level,
                workspace_root = excluded.workspace_root,
                reasoning_enabled = excluded.reasoning_enabled,
                plan_mode_enabled = excluded.plan_mode_enabled,
                mcp_enabled = excluded.mcp_enabled,
                enabled_mcp_ids_json = excluded.enabled_mcp_ids_json,
                selected_skill_ids_json = excluded.selected_skill_ids_json,
                auto_create_task = excluded.auto_create_task,
                memory_auto_summary_enabled = excluded.memory_auto_summary_enabled,
                memory_summary_message_threshold = excluded.memory_summary_message_threshold,
                memory_summary_character_threshold = excluded.memory_summary_character_threshold,
                memory_recall_limit = excluded.memory_recall_limit,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(input.session_id.as_str())
        .bind(input.selected_model_id.as_deref())
        .bind(input.selected_model_name.as_deref())
        .bind(input.selected_thinking_level.as_deref())
        .bind(input.workspace_root.as_deref())
        .bind(input.reasoning_enabled)
        .bind(input.plan_mode_enabled)
        .bind(input.mcp_enabled)
        .bind(input.enabled_mcp_ids_json.as_str())
        .bind(input.selected_skill_ids_json.as_str())
        .bind(input.auto_create_task)
        .bind(input.memory_auto_summary_enabled)
        .bind(input.memory_summary_message_threshold)
        .bind(input.memory_summary_character_threshold)
        .bind(input.memory_recall_limit)
        .bind(now.as_str())
        .bind(now.as_str())
        .execute(self.pool())
        .await
        .context("save local runtime settings")?;

        self.get_runtime_settings(owner_user_id, input.session_id.as_str())
            .await?
            .ok_or_else(|| anyhow::anyhow!("local runtime settings were not persisted"))
    }
}

#[cfg(test)]
#[path = "runtime_settings_policy_tests.rs"]
mod policy_tests;
