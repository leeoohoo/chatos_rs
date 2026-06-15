use super::*;

impl TaskService {
    pub async fn update_task(
        &self,
        id: &str,
        patch: UpdateTaskRequest,
    ) -> Result<Option<TaskRecord>, String> {
        let Some(mut task) = self.store.get_task(id).await? else {
            return Ok(None);
        };

        if let Some(title) = patch.title {
            validate_required("title", &title)?;
            task.title = title.trim().to_string();
        }
        if let Some(description) = patch.description {
            task.description = normalized_optional(Some(description));
        }
        if let Some(objective) = patch.objective {
            validate_required("objective", &objective)?;
            task.objective = objective.trim().to_string();
        }
        if let Some(input_payload) = patch.input_payload {
            task.input_payload = Some(input_payload);
        }
        if let Some(status) = patch.status {
            if matches!(status, TaskStatus::Queued | TaskStatus::Running) {
                return Err(
                    "任务排队/运行状态由系统维护，请通过执行任务进入 queued 或 running".to_string(),
                );
            }
            if self.store.has_active_run_for_task(id).await? {
                return Err("任务仍有运行中的执行记录，请先取消或等待完成".to_string());
            }
            task.status = status;
        }
        if let Some(priority) = patch.priority {
            task.priority = priority;
        }
        if let Some(tags) = patch.tags {
            task.tags = normalize_tags(Some(tags));
        }
        if let Some(model_config_id) = patch.default_model_config_id {
            let model_config_id = model_config_id.trim().to_string();
            if !model_config_id.is_empty() {
                self.ensure_model_config_exists(&model_config_id).await?;
                task.default_model_config_id = Some(model_config_id);
            } else {
                task.default_model_config_id = None;
            }
        }
        if let Some(schedule) = patch.schedule {
            task.schedule = sanitize_task_schedule_config(schedule, Some(&task.schedule))?;
        }
        if let Some(mcp_config) = patch.mcp_config {
            task.mcp_config = sanitize_task_mcp_config(mcp_config);
            self.validate_task_mcp_config(&task.mcp_config).await?;
        }
        let prerequisite_task_ids = patch
            .prerequisite_task_ids
            .map(normalize_prerequisite_task_ids);
        if let Some(prerequisite_task_ids) = prerequisite_task_ids.as_ref() {
            self.validate_task_prerequisites(id, prerequisite_task_ids, None)
                .await?;
            task.prerequisite_task_ids = prerequisite_task_ids.clone();
        }
        task.updated_at = now_rfc3339();
        self.ensure_task_thread(&task).await?;
        let saved = self.store.save_task(task).await?;
        if let Some(prerequisite_task_ids) = prerequisite_task_ids {
            self.store
                .set_task_prerequisites(id, prerequisite_task_ids)
                .await?;
        }
        self.hydrate_task_prerequisites(saved).await.map(Some)
    }

    pub async fn record_task_process(
        &self,
        id: &str,
        input: RecordTaskProcessRequest,
    ) -> Result<Option<TaskRecord>, String> {
        let Some(mut task) = self.store.get_task(id).await? else {
            return Ok(None);
        };
        let now = now_rfc3339();
        task.process_log = apply_task_process_log_update(task.process_log, input, now.as_str())?;
        task.updated_at = now;
        let saved = self.store.save_task(task).await?;
        self.hydrate_task_prerequisites(saved).await.map(Some)
    }

    pub async fn update_task_mcp(
        &self,
        id: &str,
        patch: UpdateTaskMcpRequest,
    ) -> Result<Option<TaskRecord>, String> {
        let Some(mut task) = self.store.get_task(id).await? else {
            return Ok(None);
        };
        if let Some(enabled) = patch.enabled {
            task.mcp_config.enabled = enabled;
        }
        if let Some(init_mode) = patch.init_mode {
            task.mcp_config.init_mode = init_mode;
        }
        if let Some(prompt_mode) = patch.builtin_prompt_mode {
            task.mcp_config.builtin_prompt_mode = prompt_mode;
        }
        if let Some(prompt_locale) = patch.builtin_prompt_locale {
            let normalized = prompt_locale.trim();
            if !normalized.is_empty() {
                task.mcp_config.builtin_prompt_locale = normalized.to_string();
            }
        }
        if let Some(kinds) = patch.enabled_builtin_kinds {
            task.mcp_config.enabled_builtin_kinds = normalize_builtin_kind_names(kinds);
        }
        if let Some(workspace_dir) = patch.workspace_dir {
            task.mcp_config.workspace_dir = normalized_optional(Some(workspace_dir));
        }
        if let Some(default_remote_server_id) = patch.default_remote_server_id {
            task.mcp_config.default_remote_server_id =
                normalized_optional(Some(default_remote_server_id));
        }
        if let Some(external_mcp_config_ids) = patch.external_mcp_config_ids {
            task.mcp_config.external_mcp_config_ids = external_mcp_config_ids;
        }
        task.mcp_config = sanitize_task_mcp_config(task.mcp_config);
        self.validate_task_mcp_config(&task.mcp_config).await?;
        task.updated_at = now_rfc3339();
        Ok(Some(self.store.save_task(task).await?))
    }
}
