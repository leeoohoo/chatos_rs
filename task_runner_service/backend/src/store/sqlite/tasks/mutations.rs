use super::*;

impl SqliteStore {
    pub(in crate::store) async fn save_task(&self, task: TaskRecord) -> Result<TaskRecord, String> {
        sqlx::query(
            "INSERT INTO tasks (
                id, title, description, objective, input_payload_json, status, priority,
                tags_json, default_model_config_id, memory_thread_id, tenant_id, subject_id,
                creator_user_id, creator_username, creator_display_name,
                owner_user_id, owner_username, owner_display_name, result_summary,
                process_log, last_run_id, schedule_json, parent_task_id, source_run_id,
                source_session_id, source_turn_id, source_user_message_id, task_tool_state_json,
                mcp_config_json, created_at, updated_at, deleted_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                description = excluded.description,
                objective = excluded.objective,
                input_payload_json = excluded.input_payload_json,
                status = excluded.status,
                priority = excluded.priority,
                tags_json = excluded.tags_json,
                default_model_config_id = excluded.default_model_config_id,
                memory_thread_id = excluded.memory_thread_id,
                tenant_id = excluded.tenant_id,
                subject_id = excluded.subject_id,
                creator_user_id = excluded.creator_user_id,
                creator_username = excluded.creator_username,
                creator_display_name = excluded.creator_display_name,
                owner_user_id = excluded.owner_user_id,
                owner_username = excluded.owner_username,
                owner_display_name = excluded.owner_display_name,
                result_summary = excluded.result_summary,
                process_log = excluded.process_log,
                last_run_id = excluded.last_run_id,
                schedule_json = excluded.schedule_json,
                parent_task_id = excluded.parent_task_id,
                source_run_id = excluded.source_run_id,
                source_session_id = excluded.source_session_id,
                source_turn_id = excluded.source_turn_id,
                source_user_message_id = excluded.source_user_message_id,
                task_tool_state_json = excluded.task_tool_state_json,
                mcp_config_json = excluded.mcp_config_json,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at,
                deleted_at = excluded.deleted_at",
        )
        .bind(&task.id)
        .bind(&task.title)
        .bind(task.description.clone())
        .bind(&task.objective)
        .bind(encode_json_option(&task.input_payload)?)
        .bind(task_status_to_str(task.status))
        .bind(task.priority)
        .bind(encode_json(&task.tags)?)
        .bind(task.default_model_config_id.clone())
        .bind(&task.memory_thread_id)
        .bind(&task.tenant_id)
        .bind(&task.subject_id)
        .bind(task.creator_user_id.clone())
        .bind(task.creator_username.clone())
        .bind(task.creator_display_name.clone())
        .bind(task.owner_user_id.clone())
        .bind(task.owner_username.clone())
        .bind(task.owner_display_name.clone())
        .bind(task.result_summary.clone())
        .bind(task.process_log.clone())
        .bind(task.last_run_id.clone())
        .bind(encode_json(&task.schedule)?)
        .bind(task.parent_task_id.clone())
        .bind(task.source_run_id.clone())
        .bind(task.source_session_id.clone())
        .bind(task.source_turn_id.clone())
        .bind(task.source_user_message_id.clone())
        .bind(encode_json(&task.task_tool_state)?)
        .bind(encode_json(&task.mcp_config)?)
        .bind(&task.created_at)
        .bind(&task.updated_at)
        .bind(task.deleted_at.clone())
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(task)
    }

    pub(in crate::store) async fn delete_task(&self, id: &str) -> Result<bool, String> {
        let mut tx = self.pool.begin().await.map_err(|err| err.to_string())?;
        sqlx::query("DELETE FROM task_prerequisites WHERE task_id = ? OR prerequisite_task_id = ?")
            .bind(id)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|err| err.to_string())?;
        sqlx::query(
            "DELETE FROM ui_prompts WHERE task_id = ? OR run_id IN (SELECT id FROM task_runs WHERE task_id = ?)",
        )
        .bind(id)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|err| err.to_string())?;
        sqlx::query("DELETE FROM task_run_events WHERE run_id IN (SELECT id FROM task_runs WHERE task_id = ?)")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|err| err.to_string())?;
        sqlx::query("DELETE FROM task_runs WHERE task_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|err| err.to_string())?;
        let result = sqlx::query("DELETE FROM tasks WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|err| err.to_string())?;
        tx.commit().await.map_err(|err| err.to_string())?;
        Ok(result.rows_affected() > 0)
    }
}
