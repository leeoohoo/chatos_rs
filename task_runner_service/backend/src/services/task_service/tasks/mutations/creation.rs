use super::*;

impl TaskService {
    pub async fn create_task(
        &self,
        input: CreateTaskRequest,
        creator: Option<&CurrentUser>,
        source_context: Option<TaskSourceContext>,
    ) -> Result<TaskRecord, String> {
        validate_required("title", &input.title)?;
        validate_required("objective", &input.objective)?;
        if let Some(model_config_id) = input.default_model_config_id.as_deref() {
            self.ensure_model_config_access(model_config_id, creator)
                .await?;
        }
        if matches!(input.status, Some(TaskStatus::Queued | TaskStatus::Running)) {
            return Err(
                "任务排队/运行状态由系统维护，请通过执行任务进入 queued 或 running".to_string(),
            );
        }
        let prerequisite_task_ids = normalize_prerequisite_task_ids(
            input.prerequisite_task_ids.clone().unwrap_or_default(),
        );

        let id = Uuid::new_v4().to_string();
        self.validate_task_prerequisites(&id, &prerequisite_task_ids, creator)
            .await?;
        let now = now_rfc3339();
        let source_context = source_context.unwrap_or_default();
        let schedule = sanitize_task_schedule_config(input.schedule.unwrap_or_default(), None)?;
        let mut mcp_config = sanitize_task_mcp_config(input.mcp_config.unwrap_or_default());
        if let Some(workspace_dir) = normalized_optional(source_context.workspace_dir.clone()) {
            mcp_config.workspace_dir = Some(workspace_dir);
        }
        if mcp_config.workspace_dir.is_some() {
            let _ = ensure_workspace_dir_available(
                self.config.default_workspace_dir.as_str(),
                mcp_config.workspace_dir.as_deref(),
            )?;
        }
        let passthrough_remote_server =
            if let Some(remote_server_config) = source_context.remote_server_config.clone() {
                Some(build_remote_server_record(
                    remote_server_config,
                    creator,
                    Some(id.clone()),
                    now.clone(),
                )?)
            } else {
                None
            };
        if let Some(remote_server) = passthrough_remote_server.as_ref() {
            mcp_config.default_remote_server_id = Some(remote_server.id.clone());
        }
        if passthrough_remote_server.is_none() {
            self.validate_task_mcp_config(&mcp_config, creator).await?;
        }
        let tenant_id = resolve_task_tenant_id(
            input.tenant_id,
            creator,
            self.config.default_tenant_id.as_str(),
        )?;
        let task = TaskRecord {
            id: id.clone(),
            title: input.title.trim().to_string(),
            description: normalized_optional(input.description),
            objective: input.objective.trim().to_string(),
            input_payload: input.input_payload,
            status: input.status.unwrap_or(TaskStatus::Draft),
            priority: input.priority.unwrap_or(0),
            tags: normalize_tags(input.tags),
            default_model_config_id: normalized_optional(input.default_model_config_id),
            memory_thread_id: format!("task-{id}"),
            tenant_id,
            subject_id: input
                .subject_id
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| self.config.default_subject_id.clone()),
            creator_user_id: creator.map(|user| user.id.clone()),
            creator_username: creator.map(|user| user.username.clone()),
            creator_display_name: creator.map(|user| user.display_name.clone()),
            owner_user_id: creator
                .and_then(|user| user.effective_owner_user_id().map(ToOwned::to_owned)),
            owner_username: creator
                .and_then(|user| user.effective_owner_username().map(ToOwned::to_owned)),
            owner_display_name: creator.and_then(|user| {
                user.effective_owner_display_name()
                    .map(ToOwned::to_owned)
                    .or_else(|| user.effective_owner_username().map(ToOwned::to_owned))
            }),
            result_summary: None,
            process_log: None,
            last_run_id: None,
            schedule,
            parent_task_id: None,
            source_run_id: None,
            source_session_id: normalized_optional(source_context.source_session_id),
            source_turn_id: normalized_optional(source_context.source_turn_id),
            source_user_message_id: normalized_optional(source_context.source_user_message_id),
            prerequisite_task_ids: prerequisite_task_ids.clone(),
            task_tool_state: TaskToolState::default(),
            mcp_config,
            created_at: now.clone(),
            updated_at: now,
            deleted_at: None,
        };
        self.ensure_task_thread(&task).await?;
        if let Some(remote_server) = passthrough_remote_server {
            self.store.save_remote_server(remote_server).await?;
        }
        let saved = self.store.save_task(task).await?;
        self.store
            .set_task_prerequisites(&id, prerequisite_task_ids)
            .await?;
        let hydrated = self.hydrate_task_prerequisites(saved).await?;
        Ok(hydrated)
    }
}
