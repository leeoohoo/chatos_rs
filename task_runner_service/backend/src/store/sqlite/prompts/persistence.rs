use super::*;

impl SqliteStore {
    pub(in crate::store) async fn get_ui_prompt(
        &self,
        id: &str,
    ) -> Result<Option<UiPromptRecord>, String> {
        let row = sqlx::query("SELECT * FROM ui_prompts WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        row.as_ref().map(ui_prompt_from_row).transpose()
    }

    pub(in crate::store) async fn save_ui_prompt(
        &self,
        prompt: UiPromptRecord,
    ) -> Result<UiPromptRecord, String> {
        sqlx::query(
            "INSERT INTO ui_prompts (
                id, task_id, run_id, conversation_id, conversation_turn_id, tool_call_id, kind,
                title, message, allow_cancel, timeout_ms, payload_json, response_json, status,
                created_at, updated_at, expires_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                task_id = excluded.task_id,
                run_id = excluded.run_id,
                conversation_id = excluded.conversation_id,
                conversation_turn_id = excluded.conversation_turn_id,
                tool_call_id = excluded.tool_call_id,
                kind = excluded.kind,
                title = excluded.title,
                message = excluded.message,
                allow_cancel = excluded.allow_cancel,
                timeout_ms = excluded.timeout_ms,
                payload_json = excluded.payload_json,
                response_json = excluded.response_json,
                status = excluded.status,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at,
                expires_at = excluded.expires_at",
        )
        .bind(&prompt.id)
        .bind(prompt.task_id.clone())
        .bind(prompt.run_id.clone())
        .bind(&prompt.conversation_id)
        .bind(&prompt.conversation_turn_id)
        .bind(prompt.tool_call_id.clone())
        .bind(&prompt.kind)
        .bind(&prompt.title)
        .bind(&prompt.message)
        .bind(bool_to_int(prompt.allow_cancel))
        .bind(prompt.timeout_ms as i64)
        .bind(encode_json(&prompt.payload)?)
        .bind(encode_json_optional(prompt.response.as_ref())?)
        .bind(ui_prompt_status_to_str(prompt.status))
        .bind(&prompt.created_at)
        .bind(&prompt.updated_at)
        .bind(prompt.expires_at.clone())
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(prompt)
    }
}
