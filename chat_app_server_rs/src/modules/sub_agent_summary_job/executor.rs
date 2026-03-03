use std::collections::VecDeque;

use serde_json::{json, Value};
use tracing::{info, warn};

use crate::models::sub_agent_run_message::{SubAgentRunMessage, SubAgentRunMessageService};
use crate::models::sub_agent_run_summary::{SubAgentRunSummary, SubAgentRunSummaryService};
use crate::repositories::ai_model_configs;
use crate::services::llm_prompt_runner::{run_text_prompt_with_runtime, PromptRunnerRuntime};
use crate::services::summary::engine::maybe_summarize;
use crate::services::summary::token_budget::estimate_messages_tokens;
use crate::services::summary::traits::{SummaryBoxFuture, SummaryLlmClient};
use crate::services::summary::types::{
    build_summarizer_system_prompt, build_summary_user_prompt, SummaryLlmRequest, SummaryOptions,
    SummaryTrigger,
};

use super::types::{SummaryJobDefaults, MIN_TARGET_SUMMARY_TOKENS};

const PREVIOUS_SUMMARY_CONTEXT_LIMIT: i64 = 2;

#[derive(Debug, Clone)]
pub struct RunProcessOutcome {
    pub status: String,
    pub trigger_type: Option<String>,
    pub summary_id: Option<String>,
    pub marked_messages: usize,
}

impl RunProcessOutcome {
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

pub async fn process_run(
    run_id: &str,
    defaults: &SummaryJobDefaults,
) -> Result<RunProcessOutcome, String> {
    if run_id.trim().is_empty() {
        return Ok(RunProcessOutcome::skipped("empty_run_id"));
    }

    let message_limit = defaults.round_limit.max(1) as usize;
    let pending =
        SubAgentRunMessageService::get_pending_for_summary(run_id, Some(message_limit as i64))
            .await?;
    if pending.is_empty() {
        return Ok(RunProcessOutcome::skipped("no_pending"));
    }
    if pending.len() < message_limit {
        return Ok(RunProcessOutcome::skipped("threshold_not_met"));
    }

    let selected_messages: Vec<SubAgentRunMessage> =
        pending.into_iter().take(message_limit).collect();
    if selected_messages.is_empty() {
        return Ok(RunProcessOutcome::skipped("no_pending"));
    }
    let selected_tokens =
        estimate_messages_tokens(pending_to_summary_messages(&selected_messages).as_slice());

    let runtime = resolve_runtime(defaults).await;
    let model_name = runtime.model.clone();
    let client = JobSummaryLlmClient::new(runtime);
    let mut options = build_summary_options(defaults, &model_name);
    options.keep_last_n = 0;

    let chunks = split_chunks_by_token_limit(selected_messages.as_slice(), defaults.token_limit);
    let split_chunk_count = chunks.len();
    let trigger_type = if chunks.len() > 1 {
        "message_count_limit+token_limit_split".to_string()
    } else {
        "message_count_limit".to_string()
    };

    let previous_summary_context =
        load_recent_summary_context(run_id, PREVIOUS_SUMMARY_CONTEXT_LIMIT).await;

    let mut chunk_summaries: Vec<String> = Vec::new();
    for (index, chunk) in chunks.iter().enumerate() {
        match summarize_chunk_text(
            run_id,
            chunk.as_slice(),
            previous_summary_context.as_slice(),
            &client,
            &options,
        )
        .await
        {
            Ok(summary_text) => chunk_summaries.push(summary_text),
            Err(err) => {
                let error = format!("chunk {} summarize failed: {}", index + 1, err);
                persist_failed_summary(
                    run_id,
                    selected_messages.as_slice(),
                    selected_tokens,
                    trigger_type.as_str(),
                    model_name.as_str(),
                    error.as_str(),
                )
                .await;
                warn!(
                    "[SUB-AGENT-SUMMARY-JOB] summarize failed: run_id={} trigger={} selected_messages={} selected_tokens={} split_chunks={} error={}",
                    run_id,
                    trigger_type,
                    selected_messages.len(),
                    selected_tokens,
                    split_chunk_count,
                    error
                );
                return Ok(RunProcessOutcome::failed(trigger_type));
            }
        }
    }

    if chunk_summaries.is_empty() {
        return Ok(RunProcessOutcome::skipped("no_source_messages"));
    }

    let final_summary_text = match merge_chunk_summaries(
        run_id,
        chunk_summaries.as_slice(),
        previous_summary_context.as_slice(),
        &client,
        &options,
    )
    .await
    {
        Ok(text) => text,
        Err(err) => {
            persist_failed_summary(
                run_id,
                selected_messages.as_slice(),
                selected_tokens,
                trigger_type.as_str(),
                model_name.as_str(),
                err.as_str(),
            )
            .await;
            warn!(
                "[SUB-AGENT-SUMMARY-JOB] merge chunk summaries failed: run_id={} trigger={} selected_messages={} selected_tokens={} split_chunks={} error={}",
                run_id,
                trigger_type,
                selected_messages.len(),
                selected_tokens,
                split_chunk_count,
                err
            );
            return Ok(RunProcessOutcome::failed(trigger_type));
        }
    };

    let source_ids: Vec<String> = selected_messages
        .iter()
        .map(|item| item.id.clone())
        .collect();
    if source_ids.is_empty() {
        return Ok(RunProcessOutcome::skipped("no_source_messages"));
    }
    let source_start_message_id = source_ids.first().cloned();
    let source_end_message_id = source_ids.last().cloned();

    let summary_record = SubAgentRunSummary::new(
        run_id.to_string(),
        final_summary_text,
        model_name.clone(),
        trigger_type.clone(),
        source_start_message_id,
        source_end_message_id,
        source_ids.len() as i64,
        selected_tokens,
        "done".to_string(),
        None,
    );
    let summary_id = summary_record.id.clone();
    SubAgentRunSummaryService::create(summary_record).await?;

    let summarized_at = crate::core::time::now_rfc3339();
    let marked = SubAgentRunMessageService::mark_summarized(
        run_id,
        source_ids.as_slice(),
        summary_id.as_str(),
        summarized_at.as_str(),
    )
    .await?;

    info!(
        "[SUB-AGENT-SUMMARY-JOB] summarized run_id={} trigger={} selected_messages={} selected_tokens={} split_chunks={} marked={} summary_id={}",
        run_id,
        trigger_type,
        selected_messages.len(),
        selected_tokens,
        split_chunk_count,
        marked,
        summary_id
    );

    Ok(RunProcessOutcome {
        status: "summarized".to_string(),
        trigger_type: Some(trigger_type),
        summary_id: Some(summary_id),
        marked_messages: marked,
    })
}

fn build_summary_options(config: &SummaryJobDefaults, model_name: &str) -> SummaryOptions {
    let target_summary_tokens = config.target_summary_tokens.max(MIN_TARGET_SUMMARY_TOKENS);

    SummaryOptions {
        message_limit: 1,
        max_context_tokens: 0,
        keep_last_n: config.keep_last_n_messages,
        target_summary_tokens,
        merge_target_tokens: (target_summary_tokens * 85 / 100).max(200),
        model: model_name.to_string(),
        temperature: 0.2,
        bisect_enabled: true,
        bisect_max_depth: 6,
        bisect_min_messages: 4,
        retry_on_context_overflow: true,
    }
}

fn split_chunks_by_token_limit(
    messages: &[SubAgentRunMessage],
    token_limit: i64,
) -> Vec<Vec<SubAgentRunMessage>> {
    if messages.is_empty() {
        return Vec::new();
    }
    if token_limit <= 0 {
        return vec![messages.to_vec()];
    }

    let mut queue: VecDeque<Vec<SubAgentRunMessage>> = VecDeque::new();
    let mut leaves: Vec<Vec<SubAgentRunMessage>> = Vec::new();
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

fn pending_to_summary_messages(messages: &[SubAgentRunMessage]) -> Vec<Value> {
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
            if let Some(reasoning) = message.reasoning.as_ref() {
                value["reasoning"] = Value::String(reasoning.clone());
            }
            value
        })
        .collect()
}

async fn summarize_chunk_text(
    run_id: &str,
    messages: &[SubAgentRunMessage],
    previous_summary_context: &[String],
    client: &JobSummaryLlmClient,
    options: &SummaryOptions,
) -> Result<String, String> {
    if messages.is_empty() {
        return Err("empty chunk".to_string());
    }

    let mut context_messages = build_previous_summary_context_messages(previous_summary_context);
    context_messages.extend(pending_to_summary_messages(messages));
    let summary_result = maybe_summarize(
        client,
        context_messages.as_slice(),
        options,
        Some(run_id.to_string()),
        None,
        SummaryTrigger::OverflowRetry,
    )
    .await;

    let result = match summary_result {
        Ok(value) => value,
        Err(err) => return Err(err),
    };

    if !result.summarized {
        return Err("summary not generated".to_string());
    }

    let summary_text = result.summary_text.unwrap_or_default().trim().to_string();
    if summary_text.is_empty() {
        return Err("AI 未返回总结文本".to_string());
    }

    Ok(summary_text)
}

async fn merge_chunk_summaries(
    run_id: &str,
    chunk_summaries: &[String],
    previous_summary_context: &[String],
    client: &JobSummaryLlmClient,
    options: &SummaryOptions,
) -> Result<String, String> {
    if chunk_summaries.is_empty() {
        return Err("no chunk summaries".to_string());
    }
    if chunk_summaries.len() == 1 {
        return Ok(chunk_summaries[0].clone());
    }

    let mut merge_messages = build_previous_summary_context_messages(previous_summary_context);
    for (index, chunk_summary) in chunk_summaries.iter().enumerate() {
        merge_messages.push(json!({
            "role": "assistant",
            "content": format!("分片总结 {}:\n{}", index + 1, chunk_summary),
        }));
    }

    let mut merge_options = options.clone();
    merge_options.keep_last_n = 0;
    merge_options.target_summary_tokens = options.target_summary_tokens.max(256);
    let result = maybe_summarize(
        client,
        merge_messages.as_slice(),
        &merge_options,
        Some(run_id.to_string()),
        None,
        SummaryTrigger::OverflowRetry,
    )
    .await?;
    if !result.summarized {
        return Err("merge summaries not generated".to_string());
    }

    let merged_summary = result.summary_text.unwrap_or_default().trim().to_string();
    if merged_summary.is_empty() {
        return Err("AI 未返回合并后的总结文本".to_string());
    }
    Ok(merged_summary)
}

async fn load_recent_summary_context(run_id: &str, limit: i64) -> Vec<String> {
    let target_limit = limit.max(1) as usize;
    let fetch_limit = (target_limit as i64 * 10).max(target_limit as i64);
    let mut out = Vec::new();
    match SubAgentRunSummaryService::list_by_run(run_id, Some(fetch_limit), 0).await {
        Ok(records) => {
            for record in records {
                if record.status != "done" {
                    continue;
                }
                let text = record.summary_text.trim();
                if text.is_empty() {
                    continue;
                }
                out.push(text.to_string());
                if out.len() >= target_limit {
                    break;
                }
            }
        }
        Err(err) => {
            warn!(
                "[SUB-AGENT-SUMMARY-JOB] load previous summaries failed: run_id={} error={}",
                run_id, err
            );
        }
    }

    out.reverse();
    out
}

fn build_previous_summary_context_messages(previous_summary_context: &[String]) -> Vec<Value> {
    previous_summary_context
        .iter()
        .enumerate()
        .map(|(index, text)| {
            json!({
                "role": "system",
                "content": format!("历史 Sub-Agent 总结 {}:\n{}", index + 1, text),
            })
        })
        .collect()
}

async fn persist_failed_summary(
    run_id: &str,
    pending: &[SubAgentRunMessage],
    pending_tokens: i64,
    trigger_type: &str,
    model_name: &str,
    error: &str,
) {
    let source_start_message_id = pending.first().map(|message| message.id.clone());
    let source_end_message_id = pending.last().map(|message| message.id.clone());
    let fail_record = SubAgentRunSummary::new(
        run_id.to_string(),
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

    if let Err(err) = SubAgentRunSummaryService::create(fail_record).await {
        warn!(
            "[SUB-AGENT-SUMMARY-JOB] persist failed summary record error: run_id={} detail={}",
            run_id, err
        );
    }
}

async fn resolve_runtime(defaults: &SummaryJobDefaults) -> PromptRunnerRuntime {
    if let Some(model_config_id) = defaults.model_config_id.as_deref() {
        match ai_model_configs::get_ai_model_config_by_id(model_config_id).await {
            Ok(Some(model_cfg)) if model_cfg.enabled => {
                let source = json!({
                    "model_name": model_cfg.model,
                    "provider": model_cfg.provider,
                    "thinking_level": model_cfg.thinking_level,
                    "api_key": model_cfg.api_key,
                    "base_url": model_cfg.base_url,
                    "supports_responses": model_cfg.supports_responses,
                    "temperature": 0.2,
                });
                return PromptRunnerRuntime::from_ai_model_config(
                    &source,
                    &defaults.fallback_model,
                );
            }
            Ok(Some(_)) => {
                warn!(
                    "[SUB-AGENT-SUMMARY-JOB] model config disabled, fallback to default model: {}",
                    model_config_id
                );
            }
            Ok(None) => {
                warn!(
                    "[SUB-AGENT-SUMMARY-JOB] model config not found, fallback to default model: {}",
                    model_config_id
                );
            }
            Err(err) => {
                warn!(
                    "[SUB-AGENT-SUMMARY-JOB] load model config failed ({}), fallback to default model: {}",
                    model_config_id, err
                );
            }
        }
    }

    let fallback = json!({
        "model_name": defaults.fallback_model,
        "provider": "gpt",
        "temperature": 0.2,
        "supports_responses": false,
    });
    PromptRunnerRuntime::from_ai_model_config(&fallback, &defaults.fallback_model)
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
            let target_tokens = request.target_tokens.max(MIN_TARGET_SUMMARY_TOKENS);
            let system_prompt = build_summarizer_system_prompt(target_tokens);
            let conversation_text = serialize_context_messages(request.context_messages.as_slice());
            let user_prompt = format!("{}\n\n{}", conversation_text, build_summary_user_prompt());

            run_text_prompt_with_runtime(
                &self.runtime,
                system_prompt.as_str(),
                user_prompt.as_str(),
                Some(target_tokens.max(256)),
                "sub_agent_summary_job",
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
