use std::collections::{HashSet, VecDeque};

use serde_json::{json, Value};
use tracing::{info, warn};

use crate::models::message::{Message, MessageService};
use crate::models::session::SessionService;
use crate::models::session_summary_v2::{SessionSummaryV2, SessionSummaryV2Service};
use crate::services::llm_prompt_runner::{run_text_prompt_with_runtime, PromptRunnerRuntime};
use crate::services::summary::engine::maybe_summarize;
use crate::services::summary::token_budget::estimate_messages_tokens;
use crate::services::summary::traits::{SummaryBoxFuture, SummaryLlmClient};
use crate::services::summary::types::{
    build_summarizer_system_prompt, build_summary_user_prompt, SummaryLlmRequest, SummaryOptions,
    SummaryTrigger,
};

use super::config;
use super::types::{EffectiveSummaryJobConfig, SummaryJobDefaults};

#[derive(Debug, Clone)]
pub struct SessionProcessOutcome {
    pub status: String,
    pub trigger_type: Option<String>,
    pub summary_id: Option<String>,
    pub marked_messages: usize,
}

impl SessionProcessOutcome {
    fn skipped(reason: &str) -> Self {
        Self {
            status: format!("skipped:{}", reason),
            trigger_type: None,
            summary_id: None,
            marked_messages: 0,
        }
    }

    fn failed(trigger_type: String) -> Self {
        Self {
            status: "failed".to_string(),
            trigger_type: Some(trigger_type),
            summary_id: None,
            marked_messages: 0,
        }
    }
}

pub async fn process_session(
    session_id: &str,
    defaults: &SummaryJobDefaults,
) -> Result<SessionProcessOutcome, String> {
    let session = match SessionService::get_by_id(session_id).await? {
        Some(value) => value,
        None => return Ok(SessionProcessOutcome::skipped("session_missing")),
    };

    let effective = config::resolve_effective_config(&session, defaults).await;
    if !effective.enabled {
        return Ok(SessionProcessOutcome::skipped("disabled"));
    }

    let message_limit = effective.round_limit.max(1) as usize;
    let pending =
        MessageService::get_pending_for_summary(session_id, Some(message_limit as i64)).await?;
    if pending.is_empty() {
        return Ok(SessionProcessOutcome::skipped("no_pending"));
    }
    if pending.len() < message_limit {
        return Ok(SessionProcessOutcome::skipped("threshold_not_met"));
    }

    let selected_messages: Vec<Message> = pending.into_iter().take(message_limit).collect();
    if selected_messages.is_empty() {
        return Ok(SessionProcessOutcome::skipped("no_pending"));
    }
    let selected_tokens =
        estimate_messages_tokens(pending_to_summary_messages(&selected_messages).as_slice());

    let runtime = config::resolve_runtime(&effective).await;
    let model_name = runtime.model.clone();
    let client = JobSummaryLlmClient::new(runtime);
    let mut options = build_summary_options(&effective, &model_name);
    // We already take fixed-count head messages; summarize each selected chunk fully.
    options.keep_last_n = 0;

    let chunks = split_chunks_by_token_limit(selected_messages.as_slice(), effective.token_limit);
    let split_chunk_count = chunks.len();
    let trigger_type = if chunks.len() > 1 {
        "message_count_limit+token_limit_split".to_string()
    } else {
        "message_count_limit".to_string()
    };

    let mut total_marked = 0usize;
    let mut last_summary_id: Option<String> = None;
    let mut failed_chunks = 0usize;

    for chunk in chunks {
        match summarize_and_persist_chunk(
            session_id,
            chunk.as_slice(),
            &client,
            &options,
            &model_name,
            &trigger_type,
        )
        .await
        {
            Ok(Some(outcome)) => {
                total_marked += outcome.marked_messages;
                last_summary_id = Some(outcome.summary_id);
            }
            Ok(None) => {}
            Err(err) => {
                failed_chunks += 1;
                warn!(
                    "[SESSION-SUMMARY-JOB] summarize chunk failed: session_id={} error={}",
                    session_id, err
                );
            }
        }
    }

    if total_marked == 0 {
        if failed_chunks > 0 {
            return Ok(SessionProcessOutcome::failed(trigger_type));
        }
        return Ok(SessionProcessOutcome::skipped("no_source_messages"));
    }

    info!(
        "[SESSION-SUMMARY-JOB] summarized session_id={} trigger={} selected_messages={} selected_tokens={} split_chunks={} marked={} failed_chunks={} summary_id={}",
        session_id,
        trigger_type,
        selected_messages.len(),
        selected_tokens,
        split_chunk_count,
        total_marked,
        failed_chunks,
        last_summary_id.clone().unwrap_or_default()
    );

    Ok(SessionProcessOutcome {
        status: if failed_chunks > 0 {
            "summarized_partial_failed".to_string()
        } else {
            "summarized".to_string()
        },
        trigger_type: Some(trigger_type),
        summary_id: last_summary_id,
        marked_messages: total_marked,
    })
}

fn build_summary_options(config: &EffectiveSummaryJobConfig, model_name: &str) -> SummaryOptions {
    SummaryOptions {
        message_limit: 1,
        max_context_tokens: 0,
        keep_last_n: config.keep_last_n_messages,
        target_summary_tokens: config.target_summary_tokens,
        merge_target_tokens: (config.target_summary_tokens * 85 / 100).max(200),
        model: model_name.to_string(),
        temperature: 0.2,
        bisect_enabled: true,
        bisect_max_depth: 6,
        bisect_min_messages: 4,
        retry_on_context_overflow: true,
    }
}

fn split_chunks_by_token_limit(messages: &[Message], token_limit: i64) -> Vec<Vec<Message>> {
    if messages.is_empty() {
        return Vec::new();
    }
    if token_limit <= 0 {
        return vec![messages.to_vec()];
    }

    let mut queue: VecDeque<Vec<Message>> = VecDeque::new();
    let mut leaves: Vec<Vec<Message>> = Vec::new();
    queue.push_back(messages.to_vec());

    while let Some(chunk) = queue.pop_front() {
        if chunk.is_empty() {
            continue;
        }

        let chunk_tokens = estimate_messages_tokens(pending_to_summary_messages(&chunk).as_slice());
        if chunk_tokens > token_limit && chunk.len() > 1 {
            let mid = chunk.len() / 2;
            let left = chunk[..mid].to_vec();
            let right = chunk[mid..].to_vec();
            queue.push_back(left);
            queue.push_back(right);
            continue;
        }

        leaves.push(chunk);
    }

    leaves
}

fn pending_to_summary_messages(messages: &[Message]) -> Vec<Value> {
    messages
        .iter()
        .map(|message| {
            let mut content = message.content.clone();
            if content.trim().is_empty() {
                if let Some(meta) = message.metadata.as_ref() {
                    content = meta.to_string();
                }
            }

            let mut value = json!({
                "id": message.id,
                "created_at": message.created_at,
                "role": message.role,
                "content": content,
            });

            if let Some(tool_call_id) = message.tool_call_id.as_ref() {
                value["tool_call_id"] = Value::String(tool_call_id.clone());
            }
            if let Some(tool_calls) = message.tool_calls.as_ref() {
                value["tool_calls"] = tool_calls.clone();
            }
            if let Some(reasoning) = message.reasoning.as_ref() {
                value["reasoning"] = Value::String(reasoning.clone());
            }
            value
        })
        .collect()
}

fn collect_message_ids(messages: &[Value]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    for message in messages {
        let id = message
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim();
        if id.is_empty() {
            continue;
        }
        if seen.insert(id.to_string()) {
            out.push(id.to_string());
        }
    }

    out
}

#[derive(Debug, Clone)]
struct ChunkPersistOutcome {
    summary_id: String,
    marked_messages: usize,
}

async fn summarize_and_persist_chunk(
    session_id: &str,
    messages: &[Message],
    client: &JobSummaryLlmClient,
    options: &SummaryOptions,
    model_name: &str,
    trigger_type: &str,
) -> Result<Option<ChunkPersistOutcome>, String> {
    if messages.is_empty() {
        return Ok(None);
    }

    let context_messages = pending_to_summary_messages(messages);
    let chunk_tokens = estimate_messages_tokens(context_messages.as_slice());
    let summary_result = maybe_summarize(
        client,
        context_messages.as_slice(),
        options,
        Some(session_id.to_string()),
        None,
        SummaryTrigger::OverflowRetry,
    )
    .await;

    let result = match summary_result {
        Ok(value) => value,
        Err(err) => {
            persist_failed_summary(
                session_id,
                messages,
                chunk_tokens,
                trigger_type,
                model_name,
                err.as_str(),
            )
            .await;
            return Err(err);
        }
    };

    if !result.summarized {
        return Ok(None);
    }

    let summary_text = result.summary_text.unwrap_or_default().trim().to_string();
    if summary_text.is_empty() {
        return Ok(None);
    }

    let source_ids = collect_message_ids(result.summarized_messages.as_slice());
    if source_ids.is_empty() {
        return Ok(None);
    }

    let source_tokens = estimate_messages_tokens(result.summarized_messages.as_slice());
    let source_start_message_id = source_ids.first().cloned();
    let source_end_message_id = source_ids.last().cloned();

    let summary_record = SessionSummaryV2::new(
        session_id.to_string(),
        summary_text,
        model_name.to_string(),
        trigger_type.to_string(),
        source_start_message_id,
        source_end_message_id,
        source_ids.len() as i64,
        source_tokens,
        "done".to_string(),
        None,
    );

    let summary_id = summary_record.id.clone();
    SessionSummaryV2Service::create(summary_record).await?;

    let summarized_at = crate::core::time::now_rfc3339();
    let marked = MessageService::mark_summarized(
        session_id,
        source_ids.as_slice(),
        summary_id.as_str(),
        summarized_at.as_str(),
    )
    .await?;

    Ok(Some(ChunkPersistOutcome {
        summary_id,
        marked_messages: marked,
    }))
}

async fn persist_failed_summary(
    session_id: &str,
    pending: &[Message],
    pending_tokens: i64,
    trigger_type: &str,
    model_name: &str,
    error: &str,
) {
    let source_start_message_id = pending.first().map(|message| message.id.clone());
    let source_end_message_id = pending.last().map(|message| message.id.clone());
    let fail_record = SessionSummaryV2::new(
        session_id.to_string(),
        String::new(),
        model_name.to_string(),
        trigger_type.to_string(),
        source_start_message_id,
        source_end_message_id,
        pending.len() as i64,
        pending_tokens,
        "failed".to_string(),
        Some(error.to_string()),
    );

    if let Err(err) = SessionSummaryV2Service::create(fail_record).await {
        warn!(
            "[SESSION-SUMMARY-JOB] persist failed summary record error: session_id={} detail={}",
            session_id, err
        );
    }
}

#[derive(Clone)]
struct JobSummaryLlmClient {
    runtime: PromptRunnerRuntime,
}

impl JobSummaryLlmClient {
    fn new(runtime: PromptRunnerRuntime) -> Self {
        Self { runtime }
    }
}

impl SummaryLlmClient for JobSummaryLlmClient {
    fn summarize<'a>(
        &'a self,
        request: SummaryLlmRequest,
    ) -> SummaryBoxFuture<'a, Result<String, String>> {
        Box::pin(async move {
            let system_prompt = build_summarizer_system_prompt(request.target_tokens.max(200));
            let conversation_text = serialize_context_messages(request.context_messages.as_slice());
            let user_prompt = format!("{}\n\n{}", conversation_text, build_summary_user_prompt());

            run_text_prompt_with_runtime(
                &self.runtime,
                system_prompt.as_str(),
                user_prompt.as_str(),
                Some(request.target_tokens.max(256)),
                "session_summary_job",
            )
            .await
        })
    }
}

fn serialize_context_messages(messages: &[Value]) -> String {
    let mut lines = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        let role = message
            .get("role")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        let call_id = message
            .get("tool_call_id")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let content = message
            .get("content")
            .map(content_to_text)
            .unwrap_or_else(String::new);

        let prefix = if call_id.is_empty() {
            format!("[{}][{}]", index + 1, role)
        } else {
            format!("[{}][{}][tool_call_id={}]", index + 1, role, call_id)
        };
        lines.push(format!("{} {}", prefix, content));

        if let Some(tool_calls) = message.get("tool_calls") {
            lines.push(format!(
                "[{}][assistant_tool_calls] {}",
                index + 1,
                tool_calls
            ));
        }
    }

    lines.join("\n")
}

fn content_to_text(content: &Value) -> String {
    if let Some(text) = content.as_str() {
        return text.to_string();
    }

    if let Some(array) = content.as_array() {
        let mut output = Vec::new();
        for part in array {
            if let Some(text) = part.as_str() {
                output.push(text.to_string());
                continue;
            }
            if let Some(text) = part.get("text").and_then(|value| value.as_str()) {
                output.push(text.to_string());
                continue;
            }
            output.push(part.to_string());
        }
        return output.join("\n");
    }

    content.to_string()
}
