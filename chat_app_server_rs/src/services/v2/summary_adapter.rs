use serde_json::{json, Value};
use tracing::warn;

use crate::models::session_summary::{SessionSummary, SessionSummaryService};
use crate::models::session_summary_message::SessionSummaryMessageService;
use crate::services::summary::engine::build_summary_messages_for_llm;
use crate::services::summary::persist::build_summary_metadata;
use crate::services::summary::traits::{SummaryBoxFuture, SummaryLlmClient, SummaryStore};
use crate::services::summary::types::{
    PersistSummaryOutcome, PersistSummaryPayload, SummaryLlmRequest,
};
use crate::services::v2::ai_request_handler::{AiRequestHandler, StreamCallbacks};
use crate::services::v2::message_manager::MessageManager;

#[derive(Clone)]
pub struct V2SummaryAdapter {
    ai_request_handler: AiRequestHandler,
    message_manager: MessageManager,
}

impl V2SummaryAdapter {
    pub fn new(ai_request_handler: AiRequestHandler, message_manager: MessageManager) -> Self {
        Self {
            ai_request_handler,
            message_manager,
        }
    }
}

impl SummaryLlmClient for V2SummaryAdapter {
    fn summarize<'a>(
        &'a self,
        request: SummaryLlmRequest,
    ) -> SummaryBoxFuture<'a, Result<String, String>> {
        Box::pin(async move {
            let summary_messages =
                build_summary_messages_for_llm(request.context_messages, request.target_tokens);
            let stream_cb = request
                .callbacks
                .as_ref()
                .and_then(|callbacks| callbacks.on_stream.clone());

            let response = self
                .ai_request_handler
                .handle_request(
                    summary_messages,
                    None,
                    request.model,
                    Some(request.temperature),
                    None,
                    StreamCallbacks {
                        on_chunk: stream_cb,
                        on_thinking: None,
                    },
                    false,
                    None,
                    None,
                    request.session_id,
                    request.stream,
                    None,
                    None,
                    "summary",
                )
                .await?;

            Ok(response.content)
        })
    }
}

impl SummaryStore for V2SummaryAdapter {
    fn persist_summary<'a>(
        &'a self,
        payload: PersistSummaryPayload,
    ) -> SummaryBoxFuture<'a, Result<PersistSummaryOutcome, String>> {
        Box::pin(async move {
            let summary_metadata = build_summary_metadata(&payload);
            let mut record =
                SessionSummary::new(payload.session_id.clone(), payload.summary_text.clone());
            record.summary_prompt = Some(payload.summary_prompt.clone());
            record.model = Some(payload.model.clone());
            record.temperature = Some(payload.temperature);
            record.target_summary_tokens = Some(payload.target_summary_tokens);
            record.keep_last_n = Some(payload.keep_last_n);
            record.message_count = Some(payload.source.message_ids.len() as i64);
            record.approx_tokens = Some(payload.approx_tokens);
            record.first_message_id = payload.source.first_message_id.clone();
            record.last_message_id = payload.source.last_message_id.clone();
            record.first_message_created_at = payload.source.first_message_created_at.clone();
            record.last_message_created_at = payload.source.last_message_created_at.clone();
            record.metadata = Some(summary_metadata.clone());

            let record_id = record.id.clone();
            let mut summary_id: Option<String> = None;
            match SessionSummaryService::create(record).await {
                Ok(_) => {
                    summary_id = Some(record_id.clone());
                    if !payload.source.message_ids.is_empty() {
                        if let Err(err) = SessionSummaryMessageService::create_links(
                            &record_id,
                            &payload.session_id,
                            payload.source.message_ids.as_slice(),
                        )
                        .await
                        {
                            warn!("[SUM-V2] create summary message links failed: {}", err);
                        }
                    }
                }
                Err(err) => {
                    warn!("[SUM-V2] create summary record failed: {}", err);
                }
            }

            let mut message_meta = json!({
                "type": "session_summary",
                "keepLastN": payload.keep_last_n,
                "summary_timestamp": chrono::Utc::now().timestamp_millis(),
            });
            if let Some(map) = message_meta.as_object_mut() {
                if let Some(summary_map) = summary_metadata.as_object() {
                    for (key, value) in summary_map {
                        map.insert(key.to_string(), value.clone());
                    }
                }
                if let Some(id) = summary_id.clone() {
                    map.insert("summary_id".to_string(), Value::String(id));
                }
            }

            let _ = self
                .message_manager
                .save_assistant_message(
                    &payload.session_id,
                    "【上下文已压缩为摘要】",
                    Some(payload.summary_text.clone()),
                    None,
                    None,
                    None,
                    Some(message_meta),
                    None,
                )
                .await;

            Ok(PersistSummaryOutcome { summary_id })
        })
    }
}
