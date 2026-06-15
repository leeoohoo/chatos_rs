use super::support::{normalized_optional, prompt_event_payload, status_label};
use super::*;

impl UiPromptService {
    pub async fn list_prompts(
        &self,
        task_id: Option<&str>,
        run_id: Option<&str>,
        status: Option<UiPromptStatus>,
    ) -> Result<Vec<UiPromptRecord>, String> {
        self.store.list_ui_prompts(task_id, run_id, status).await
    }

    pub async fn get_prompt(&self, id: &str) -> Result<Option<UiPromptRecord>, String> {
        self.store.get_ui_prompt(id).await
    }

    pub async fn list_prompt_task_counts(
        &self,
        status: Option<UiPromptStatus>,
    ) -> Result<Vec<UiPromptTaskCountRecord>, String> {
        self.store.list_ui_prompt_task_counts(status).await
    }

    pub async fn list_prompts_page(
        &self,
        filters: PromptListFilters,
    ) -> Result<PaginatedResponse<UiPromptRecord>, String> {
        let filters = sanitize_prompt_list_filters(filters);
        self.store.list_ui_prompts_page(&filters).await
    }

    pub async fn submit_prompt(
        &self,
        id: &str,
        input: SubmitUiPromptRequest,
    ) -> Result<Option<UiPromptRecord>, String> {
        let Some(mut prompt) = self.store.get_ui_prompt(id).await? else {
            return Ok(None);
        };
        if prompt.status != UiPromptStatus::Pending {
            return Err(format!(
                "提示当前状态不允许提交: {}",
                status_label(prompt.status)
            ));
        }

        let response = UiPromptResponseSubmission {
            status: "submitted".to_string(),
            values: input.values,
            selection: input.selection,
            reason: normalized_optional(input.reason),
        };
        prompt.status = UiPromptStatus::Submitted;
        prompt.response = Some(response);
        prompt.updated_at = now_rfc3339();
        let saved = self.store.save_ui_prompt(prompt).await?;
        self.append_prompt_event(
            &saved,
            "ui_prompt_submitted",
            Some("已收到人工确认输入".to_string()),
            Some(prompt_event_payload(&saved)),
        )
        .await;
        self.waiters.wake(id);
        Ok(Some(saved))
    }

    pub async fn cancel_prompt(
        &self,
        id: &str,
        input: CancelUiPromptRequest,
    ) -> Result<Option<UiPromptRecord>, String> {
        let Some(mut prompt) = self.store.get_ui_prompt(id).await? else {
            return Ok(None);
        };
        if prompt.status != UiPromptStatus::Pending {
            return Err(format!(
                "提示当前状态不允许取消: {}",
                status_label(prompt.status)
            ));
        }
        if !prompt.allow_cancel {
            return Err("当前提示不允许取消".to_string());
        }

        let reason = normalized_optional(input.reason);
        prompt.status = UiPromptStatus::Cancelled;
        prompt.response = Some(UiPromptResponseSubmission {
            status: "cancelled".to_string(),
            values: None,
            selection: None,
            reason: reason.clone(),
        });
        prompt.updated_at = now_rfc3339();
        let saved = self.store.save_ui_prompt(prompt).await?;
        self.append_prompt_event(
            &saved,
            "ui_prompt_cancelled",
            Some("人工取消了提示".to_string()),
            Some(prompt_event_payload(&saved)),
        )
        .await;
        self.waiters.wake(id);
        Ok(Some(saved))
    }

    pub(in crate::ui_prompt_service) async fn append_prompt_event(
        &self,
        prompt: &UiPromptRecord,
        event_type: &str,
        message: Option<String>,
        payload: Option<Value>,
    ) {
        let Some(run_id) = prompt.run_id.as_deref() else {
            return;
        };
        if let Err(err) = self
            .store
            .append_run_event(TaskRunEventRecord::new(
                run_id.to_string(),
                event_type.to_string(),
                message,
                payload,
            ))
            .await
        {
            tracing::warn!(
                "failed to append ui prompt event for run {}: {}",
                run_id,
                err
            );
        }
    }

    pub(in crate::ui_prompt_service) async fn timeout_prompt(
        &self,
        id: &str,
    ) -> Result<UiPromptRecord, String> {
        let Some(mut prompt) = self.store.get_ui_prompt(id).await? else {
            return Err(format!("提示不存在: {id}"));
        };
        if prompt.status != UiPromptStatus::Pending {
            return Ok(prompt);
        }

        prompt.status = UiPromptStatus::TimedOut;
        prompt.response = Some(UiPromptResponseSubmission {
            status: "timed_out".to_string(),
            values: None,
            selection: None,
            reason: Some("prompt timed out".to_string()),
        });
        prompt.updated_at = now_rfc3339();
        let saved = self.store.save_ui_prompt(prompt).await?;
        self.append_prompt_event(
            &saved,
            "ui_prompt_timed_out",
            Some("人工确认提示已超时".to_string()),
            Some(prompt_event_payload(&saved)),
        )
        .await;
        Ok(saved)
    }

    pub(in crate::ui_prompt_service) async fn resolve_context_ids(
        &self,
        payload: &UiPromptPayload,
    ) -> Result<(Option<String>, Option<String>), String> {
        if let Some(run) = self.store.get_run(&payload.conversation_turn_id).await? {
            return Ok((Some(run.task_id), Some(run.id)));
        }
        let task_id = self
            .store
            .get_task(&payload.conversation_id)
            .await?
            .map(|task| task.id);
        Ok((task_id, None))
    }
}
