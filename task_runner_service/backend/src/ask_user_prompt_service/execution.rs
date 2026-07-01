// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::support::{prompt_event_payload, prompt_to_decision};
use super::*;

#[async_trait]
impl AskUserStore for AskUserPromptService {
    async fn execute_prompt(
        &self,
        payload: AskUserPromptPayload,
        on_stream_chunk: Option<AskUserStreamChunkCallback>,
    ) -> Result<AskUserDecision, String> {
        let (task_id, run_id) = self.resolve_context_ids(&payload).await?;
        let created_at = now_rfc3339();
        let expires_at = if payload.timeout_ms > 0 {
            Some(
                (Utc::now()
                    + ChronoDuration::milliseconds(payload.timeout_ms.min(i64::MAX as u64) as i64))
                .to_rfc3339(),
            )
        } else {
            None
        };
        let prompt =
            AskUserPromptRecord::from_payload(payload, task_id, run_id, created_at, expires_at);
        let notify = self.waiters.register(&prompt.id);
        let timeout_ms = prompt.timeout_ms;
        let prompt_id = prompt.id.clone();
        self.store.save_ask_user_prompt(prompt.clone()).await?;
        self.append_prompt_event(
            &prompt,
            "ask_user_prompt_pending",
            Some("任务等待人工确认".to_string()),
            Some(prompt_event_payload(&prompt)),
        )
        .await;
        self.try_send_chatos_ask_user_prompt_required(&prompt).await;

        if let Some(callback) = on_stream_chunk {
            let title = if prompt.title.trim().is_empty() {
                prompt.kind.clone()
            } else {
                prompt.title.clone()
            };
            callback(format!("Task Runner 等待人工确认: {title} ({})", prompt.id));
        }

        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            let Some(current) = self.store.get_ask_user_prompt(&prompt_id).await? else {
                self.waiters.remove(&prompt_id);
                return Err(format!("提示不存在: {prompt_id}"));
            };
            if current.status != AskUserPromptStatus::Pending {
                self.waiters.remove(&prompt_id);
                return Ok(prompt_to_decision(current));
            }

            tokio::select! {
                _ = notify.notified() => {}
                _ = tokio::time::sleep(PROMPT_STATUS_POLL_INTERVAL) => {}
                _ = tokio::time::sleep_until(deadline) => {
                    let timed_out = self.timeout_prompt(&prompt_id).await?;
                    self.waiters.remove(&prompt_id);
                    return Ok(prompt_to_decision(timed_out));
                }
            }
        }
    }
}
