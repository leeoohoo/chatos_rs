use serde_json::{json, Value};
use tracing::warn;

use crate::config::Config;
use crate::models::message::Message;
use crate::services::summary::engine::{maybe_summarize, retry_after_context_overflow};
use crate::services::summary::persist::persist_summary;
use crate::services::summary::types::{
    build_summarizer_system_prompt, PersistSummaryPayload, SummaryOptions, SummaryResult,
    SummarySourceInfo, SummaryTrigger,
};
use crate::services::v2::ai_request_handler::AiRequestHandler;
use crate::services::v2::message_manager::MessageManager;
use crate::services::v2::summary_adapter::V2SummaryAdapter;

pub use crate::services::summary::types::SummaryCallbacks;

pub struct ConversationSummarizer {
    ai_request_handler: AiRequestHandler,
    message_manager: MessageManager,
    defaults: SummaryDefaults,
}

#[derive(Debug, Default, Clone)]
pub struct InMemorySummaryResult {
    pub summarized: bool,
    pub system_prompt: Option<String>,
    pub summary: Option<String>,
    pub keep_last_n: usize,
    pub summarize_count: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone)]
struct SummaryDefaults {
    message_limit: i64,
    max_context_tokens: i64,
    keep_last_n: usize,
    target_summary_tokens: i64,
    merge_target_tokens: i64,
    model: String,
    temperature: f64,
    bisect_enabled: bool,
    bisect_max_depth: usize,
    bisect_min_messages: usize,
    retry_on_context_overflow: bool,
}

impl ConversationSummarizer {
    pub fn new(ai_request_handler: AiRequestHandler, message_manager: MessageManager) -> Self {
        let cfg = Config::get();
        Self {
            ai_request_handler,
            message_manager,
            defaults: SummaryDefaults {
                message_limit: cfg.summary_message_limit,
                max_context_tokens: cfg.summary_max_context_tokens,
                keep_last_n: cfg.summary_keep_last_n.max(0) as usize,
                target_summary_tokens: cfg.summary_target_tokens,
                merge_target_tokens: cfg.summary_merge_target_tokens,
                model: "gpt-4".to_string(),
                temperature: cfg.summary_temperature,
                bisect_enabled: cfg.summary_bisect_enabled,
                bisect_max_depth: cfg.summary_bisect_max_depth.max(1) as usize,
                bisect_min_messages: cfg.summary_bisect_min_messages.max(1) as usize,
                retry_on_context_overflow: cfg.summary_retry_on_context_overflow,
            },
        }
    }

    pub async fn maybe_summarize(
        &self,
        session_id: &str,
        opts: Option<SummaryOverrides>,
    ) -> Result<Option<(String, Vec<Value>)>, String> {
        let effective_records = self.load_effective_records(session_id).await;
        let effective_messages = map_records_to_messages(effective_records.as_slice());
        let options = self.resolve_options(opts.unwrap_or_default());

        let adapter = V2SummaryAdapter::new(
            self.ai_request_handler.clone(),
            self.message_manager.clone(),
        );
        let result = maybe_summarize(
            &adapter,
            effective_messages.as_slice(),
            &options,
            Some(session_id.to_string()),
            None,
            SummaryTrigger::Proactive,
        )
        .await?;

        if !result.summarized {
            return Ok(None);
        }

        if let Some(summary_text) = result.summary_text.as_ref() {
            let source = build_source_info(
                effective_records.as_slice(),
                result.summarized_messages.len(),
                result.kept_messages.len(),
            );
            self.persist_result(
                &adapter,
                session_id,
                summary_text,
                &result,
                &options,
                source,
                SummaryTrigger::Proactive,
            )
            .await;
        }

        Ok(result
            .system_prompt
            .clone()
            .map(|prompt| (prompt, result.kept_messages.clone())))
    }

    pub async fn maybe_summarize_in_memory(
        &self,
        messages: &[Value],
        opts: Option<SummaryOverrides>,
        session_id: Option<String>,
        persist: bool,
        callbacks: Option<SummaryCallbacks>,
    ) -> Result<InMemorySummaryResult, String> {
        let options = self.resolve_options(opts.unwrap_or_default());
        let adapter = V2SummaryAdapter::new(
            self.ai_request_handler.clone(),
            self.message_manager.clone(),
        );

        let result = maybe_summarize(
            &adapter,
            messages,
            &options,
            session_id.clone(),
            callbacks,
            SummaryTrigger::Proactive,
        )
        .await?;

        if !result.summarized {
            return Ok(InMemorySummaryResult::default());
        }

        if persist {
            if let (Some(sid), Some(summary_text)) =
                (session_id.as_ref(), result.summary_text.as_ref())
            {
                let effective_records = self.load_effective_records(sid).await;
                let source = build_source_info(
                    effective_records.as_slice(),
                    result.summarized_messages.len(),
                    result.kept_messages.len(),
                );
                self.persist_result(
                    &adapter,
                    sid,
                    summary_text,
                    &result,
                    &options,
                    source,
                    SummaryTrigger::Proactive,
                )
                .await;
            }
        }

        Ok(to_in_memory_result(&result, options.keep_last_n))
    }

    pub async fn retry_after_context_overflow_in_memory(
        &self,
        messages: &[Value],
        err: &str,
        opts: Option<SummaryOverrides>,
        session_id: Option<String>,
        persist: bool,
        callbacks: Option<SummaryCallbacks>,
    ) -> Result<Option<InMemorySummaryResult>, String> {
        let options = self.resolve_options(opts.unwrap_or_default());
        let adapter = V2SummaryAdapter::new(
            self.ai_request_handler.clone(),
            self.message_manager.clone(),
        );

        let result = retry_after_context_overflow(
            &adapter,
            messages,
            err,
            &options,
            session_id.clone(),
            callbacks,
        )
        .await?;

        let Some(result) = result else {
            return Ok(None);
        };

        if persist {
            if let (Some(sid), Some(summary_text)) =
                (session_id.as_ref(), result.summary_text.as_ref())
            {
                let effective_records = self.load_effective_records(sid).await;
                let source = build_source_info(
                    effective_records.as_slice(),
                    result.summarized_messages.len(),
                    result.kept_messages.len(),
                );
                self.persist_result(
                    &adapter,
                    sid,
                    summary_text,
                    &result,
                    &options,
                    source,
                    SummaryTrigger::OverflowRetry,
                )
                .await;
            }
        }

        Ok(Some(to_in_memory_result(&result, options.keep_last_n)))
    }

    async fn load_effective_records(&self, session_id: &str) -> Vec<Message> {
        self.message_manager
            .get_session_messages(session_id, None)
            .await
            .into_iter()
            .filter(|message| {
                message
                    .metadata
                    .as_ref()
                    .and_then(|value| value.get("type"))
                    .and_then(|value| value.as_str())
                    != Some("session_summary")
            })
            .collect()
    }

    async fn persist_result(
        &self,
        adapter: &V2SummaryAdapter,
        session_id: &str,
        summary_text: &str,
        result: &SummaryResult,
        options: &SummaryOptions,
        source: SummarySourceInfo,
        trigger: SummaryTrigger,
    ) {
        let payload = PersistSummaryPayload {
            session_id: session_id.to_string(),
            summary_text: summary_text.to_string(),
            summary_prompt: build_summarizer_system_prompt(options.target_summary_tokens),
            model: options.model.clone(),
            temperature: options.temperature,
            target_summary_tokens: options.target_summary_tokens,
            keep_last_n: options.keep_last_n as i64,
            approx_tokens: result.stats.input_tokens,
            trigger,
            truncated: result.truncated,
            stats: result.stats.clone(),
            source,
        };

        match persist_summary(adapter, payload).await {
            Ok(outcome) => {
                if let Some(summary_id) = outcome.summary_id {
                    tracing::info!("[SUM-V2] persisted summary_id={}", summary_id);
                }
            }
            Err(err) => {
                warn!("[SUM-V2] persist summary failed: {}", err);
            }
        }
    }

    fn resolve_options(&self, overrides: SummaryOverrides) -> SummaryOptions {
        SummaryOptions {
            message_limit: overrides
                .message_limit
                .unwrap_or(self.defaults.message_limit),
            max_context_tokens: overrides
                .max_context_tokens
                .unwrap_or(self.defaults.max_context_tokens),
            keep_last_n: overrides
                .keep_last_n
                .unwrap_or(self.defaults.keep_last_n as i64)
                .max(0) as usize,
            target_summary_tokens: overrides
                .target_summary_tokens
                .unwrap_or(self.defaults.target_summary_tokens),
            merge_target_tokens: overrides
                .merge_target_tokens
                .unwrap_or(self.defaults.merge_target_tokens),
            model: overrides
                .model
                .unwrap_or_else(|| self.defaults.model.clone()),
            temperature: overrides.temperature.unwrap_or(self.defaults.temperature),
            bisect_enabled: overrides
                .bisect_enabled
                .unwrap_or(self.defaults.bisect_enabled),
            bisect_max_depth: overrides
                .bisect_max_depth
                .unwrap_or(self.defaults.bisect_max_depth as i64)
                .max(1) as usize,
            bisect_min_messages: overrides
                .bisect_min_messages
                .unwrap_or(self.defaults.bisect_min_messages as i64)
                .max(1) as usize,
            retry_on_context_overflow: overrides
                .retry_on_context_overflow
                .unwrap_or(self.defaults.retry_on_context_overflow),
        }
    }
}

#[derive(Default)]
pub struct SummaryOverrides {
    pub message_limit: Option<i64>,
    pub max_context_tokens: Option<i64>,
    pub keep_last_n: Option<i64>,
    pub target_summary_tokens: Option<i64>,
    pub merge_target_tokens: Option<i64>,
    pub model: Option<String>,
    pub temperature: Option<f64>,
    pub bisect_enabled: Option<bool>,
    pub bisect_max_depth: Option<i64>,
    pub bisect_min_messages: Option<i64>,
    pub retry_on_context_overflow: Option<bool>,
}

fn map_records_to_messages(records: &[Message]) -> Vec<Value> {
    records
        .iter()
        .map(|message| {
            if message.role == "tool" {
                json!({
                    "role": "tool",
                    "tool_call_id": message.tool_call_id.clone().unwrap_or_default(),
                    "content": message.content
                })
            } else {
                let mut item = json!({
                    "role": message.role,
                    "content": message.content
                });
                if let Some(tool_calls) = message.tool_calls.clone() {
                    item["tool_calls"] = tool_calls;
                }
                item
            }
        })
        .collect()
}

fn build_source_info(
    records: &[Message],
    summarized_messages_len: usize,
    kept_messages_len: usize,
) -> SummarySourceInfo {
    if records.is_empty() || summarized_messages_len == 0 {
        return SummarySourceInfo::default();
    }

    let total = records.len();
    let kept_start = total.saturating_sub(kept_messages_len);
    let summarize_end = kept_start.min(total);
    let summarize_start = summarize_end.saturating_sub(summarized_messages_len.min(summarize_end));

    if summarize_start >= summarize_end {
        return SummarySourceInfo::default();
    }

    let slice = &records[summarize_start..summarize_end];
    SummarySourceInfo {
        message_ids: slice.iter().map(|item| item.id.clone()).collect(),
        first_message_id: slice.first().map(|item| item.id.clone()),
        last_message_id: slice.last().map(|item| item.id.clone()),
        first_message_created_at: slice.first().map(|item| item.created_at.clone()),
        last_message_created_at: slice.last().map(|item| item.created_at.clone()),
    }
}

fn to_in_memory_result(result: &SummaryResult, keep_last_n: usize) -> InMemorySummaryResult {
    InMemorySummaryResult {
        summarized: result.summarized,
        system_prompt: result.system_prompt.clone(),
        summary: result.summary_text.clone(),
        keep_last_n,
        summarize_count: result.summarized_messages.len(),
        truncated: result.truncated,
    }
}
