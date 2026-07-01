// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::support::{normalized_optional, prompt_event_payload, status_label};
use super::*;

impl AskUserPromptService {
    pub async fn list_prompts(
        &self,
        task_id: Option<&str>,
        run_id: Option<&str>,
        status: Option<AskUserPromptStatus>,
    ) -> Result<Vec<AskUserPromptRecord>, String> {
        self.store
            .list_ask_user_prompts(task_id, run_id, status)
            .await
    }

    pub async fn get_prompt(&self, id: &str) -> Result<Option<AskUserPromptRecord>, String> {
        self.store.get_ask_user_prompt(id).await
    }

    pub async fn list_prompt_task_counts(
        &self,
        status: Option<AskUserPromptStatus>,
    ) -> Result<Vec<AskUserPromptTaskCountRecord>, String> {
        self.store.list_ask_user_prompt_task_counts(status).await
    }

    pub async fn list_prompts_page(
        &self,
        filters: PromptListFilters,
    ) -> Result<PaginatedResponse<AskUserPromptRecord>, String> {
        let filters = sanitize_prompt_list_filters(filters);
        self.store.list_ask_user_prompts_page(&filters).await
    }

    pub async fn submit_prompt(
        &self,
        id: &str,
        input: SubmitAskUserPromptRequest,
    ) -> Result<Option<AskUserPromptRecord>, String> {
        let Some(mut prompt) = self.store.get_ask_user_prompt(id).await? else {
            return Ok(None);
        };
        if prompt.status != AskUserPromptStatus::Pending {
            return Err(format!(
                "提示当前状态不允许提交: {}",
                status_label(prompt.status)
            ));
        }

        let response = AskUserResponseSubmission {
            status: "submitted".to_string(),
            values: input.values,
            selection: input.selection,
            reason: normalized_optional(input.reason),
        };
        prompt.status = AskUserPromptStatus::Submitted;
        prompt.response = Some(response);
        prompt.updated_at = now_rfc3339();
        let saved = self.store.save_ask_user_prompt(prompt).await?;
        self.append_prompt_event(
            &saved,
            "ask_user_prompt_submitted",
            Some("已收到人工确认输入".to_string()),
            Some(prompt_event_payload(&saved)),
        )
        .await;
        self.try_send_chatos_ask_user_prompt_resolved(&saved).await;
        self.waiters.wake(id);
        Ok(Some(saved))
    }

    pub async fn cancel_prompt(
        &self,
        id: &str,
        input: CancelAskUserPromptRequest,
    ) -> Result<Option<AskUserPromptRecord>, String> {
        let Some(mut prompt) = self.store.get_ask_user_prompt(id).await? else {
            return Ok(None);
        };
        if prompt.status != AskUserPromptStatus::Pending {
            return Err(format!(
                "提示当前状态不允许取消: {}",
                status_label(prompt.status)
            ));
        }
        if !prompt.allow_cancel {
            return Err("当前提示不允许取消".to_string());
        }

        let reason = normalized_optional(input.reason);
        prompt.status = AskUserPromptStatus::Cancelled;
        prompt.response = Some(AskUserResponseSubmission {
            status: "cancelled".to_string(),
            values: None,
            selection: None,
            reason: reason.clone(),
        });
        prompt.updated_at = now_rfc3339();
        let saved = self.store.save_ask_user_prompt(prompt).await?;
        self.append_prompt_event(
            &saved,
            "ask_user_prompt_cancelled",
            Some("人工取消了提示".to_string()),
            Some(prompt_event_payload(&saved)),
        )
        .await;
        self.try_send_chatos_ask_user_prompt_resolved(&saved).await;
        self.waiters.wake(id);
        Ok(Some(saved))
    }

    pub(in crate::ask_user_prompt_service) async fn append_prompt_event(
        &self,
        prompt: &AskUserPromptRecord,
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
                "failed to append ask user prompt event for run {}: {}",
                run_id,
                err
            );
        }
    }

    pub(in crate::ask_user_prompt_service) async fn timeout_prompt(
        &self,
        id: &str,
    ) -> Result<AskUserPromptRecord, String> {
        let Some(mut prompt) = self.store.get_ask_user_prompt(id).await? else {
            return Err(format!("提示不存在: {id}"));
        };
        if prompt.status != AskUserPromptStatus::Pending {
            return Ok(prompt);
        }

        prompt.status = AskUserPromptStatus::TimedOut;
        prompt.response = Some(AskUserResponseSubmission {
            status: "timed_out".to_string(),
            values: None,
            selection: None,
            reason: Some("prompt timed out".to_string()),
        });
        prompt.updated_at = now_rfc3339();
        let saved = self.store.save_ask_user_prompt(prompt).await?;
        self.append_prompt_event(
            &saved,
            "ask_user_prompt_timed_out",
            Some("人工确认提示已超时".to_string()),
            Some(prompt_event_payload(&saved)),
        )
        .await;
        self.try_send_chatos_ask_user_prompt_resolved(&saved).await;
        Ok(saved)
    }

    pub(in crate::ask_user_prompt_service) async fn resolve_context_ids(
        &self,
        payload: &AskUserPromptPayload,
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
