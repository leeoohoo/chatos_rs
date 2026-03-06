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

use super::types::{EffectiveSummaryJobConfig, MIN_TARGET_SUMMARY_TOKENS};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TriggerKind {
    MessageCountLimit,
    TokenLimit,
}

impl TriggerKind {
    fn as_str(self) -> &'static str {
        match self {
            TriggerKind::MessageCountLimit => "message_count_limit",
            TriggerKind::TokenLimit => "token_limit",
        }
    }
}

pub async fn process_run(
    run_id: &str,
    effective: &EffectiveSummaryJobConfig,
) -> Result<RunProcessOutcome, String> {
    if run_id.trim().is_empty() {
        return Ok(RunProcessOutcome::skipped("empty_run_id"));
    }
    if !effective.enabled {
        return Ok(RunProcessOutcome::skipped("disabled"));
    }
    if effective.model_config_id.is_none() {
        return Ok(RunProcessOutcome::skipped("no_model_config"));
    }

    let message_limit = effective.round_limit.max(1) as usize;
    let pending =
        SubAgentRunMessageService::get_pending_for_summary(run_id, Some(message_limit as i64))
            .await?;
    if pending.is_empty() {
        return Ok(RunProcessOutcome::skipped("no_pending"));
    }
    let pending_tokens = estimate_messages_tokens(pending_to_summary_messages(&pending).as_slice());
    let trigger_kind = if let Some(kind) = determine_trigger_kind(
        pending.len(),
        pending_tokens,
        message_limit,
        effective.token_limit,
    ) {
        kind
    } else {
        return Ok(RunProcessOutcome::skipped("threshold_not_met"));
    };

    let selected_messages: Vec<SubAgentRunMessage> =
        pending.into_iter().take(message_limit).collect();
    if selected_messages.is_empty() {
        return Ok(RunProcessOutcome::skipped("no_pending"));
    }
    let partitioned =
        partition_messages_by_token_limit(selected_messages.as_slice(), effective.token_limit);
    let oversized_count = partitioned.oversized.len();
    let oversized_ids_preview = summarize_message_ids_preview(partitioned.oversized.as_slice(), 5);
    let selected_tokens =
        estimate_messages_tokens(pending_to_summary_messages(&selected_messages).as_slice());

    let runtime = resolve_runtime(effective).await;
    let model_name = runtime.model.clone();
    let client = JobSummaryLlmClient::new(runtime);
    let mut options = build_summary_options(effective, &model_name);
    options.keep_last_n = 0;

    let chunks =
        split_chunks_by_token_limit(partitioned.summarizable.as_slice(), effective.token_limit);
    let split_chunk_count = chunks.len();
    let trigger_type = build_trigger_type(trigger_kind, split_chunk_count > 1, oversized_count > 0);

    if partitioned.summarizable.is_empty() {
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
            build_oversized_only_summary_text(
                oversized_count,
                oversized_ids_preview.as_slice(),
                effective.token_limit,
            ),
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
            "[SUB-AGENT-SUMMARY-JOB] summarized oversized-only batch: run_id={} trigger={} selected_messages={} selected_tokens={} oversized_skipped_count={} oversized_skipped_ids_preview={} marked={} summary_id={}",
            run_id,
            trigger_type,
            selected_messages.len(),
            selected_tokens,
            oversized_count,
            oversized_ids_preview.join(","),
            marked,
            summary_id
        );
        return Ok(RunProcessOutcome {
            status: "summarized".to_string(),
            trigger_type: Some(trigger_type),
            summary_id: Some(summary_id),
            marked_messages: marked,
        });
    }

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

    let final_summary_text = append_oversized_skip_note(
        final_summary_text,
        oversized_count,
        oversized_ids_preview.as_slice(),
    );

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
        "[SUB-AGENT-SUMMARY-JOB] summarized run_id={} trigger={} selected_messages={} selected_tokens={} split_chunks={} oversized_skipped_count={} oversized_skipped_ids_preview={} marked={} summary_id={}",
        run_id,
        trigger_type,
        selected_messages.len(),
        selected_tokens,
        split_chunk_count,
        oversized_count,
        oversized_ids_preview.join(","),
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

fn build_summary_options(config: &EffectiveSummaryJobConfig, model_name: &str) -> SummaryOptions {
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

fn determine_trigger_kind(
    pending_count: usize,
    pending_tokens: i64,
    message_limit: usize,
    token_limit: i64,
) -> Option<TriggerKind> {
    if pending_count >= message_limit.max(1) {
        return Some(TriggerKind::MessageCountLimit);
    }
    if token_limit > 0 && pending_tokens >= token_limit {
        return Some(TriggerKind::TokenLimit);
    }
    None
}

#[derive(Debug, Clone, Default)]
struct PartitionedMessages {
    summarizable: Vec<SubAgentRunMessage>,
    oversized: Vec<SubAgentRunMessage>,
}

fn build_trigger_type(
    trigger_kind: TriggerKind,
    split_by_token_limit: bool,
    has_oversized_skipped: bool,
) -> String {
    let mut parts = vec![trigger_kind.as_str().to_string()];
    if split_by_token_limit {
        parts.push("token_limit_split".to_string());
    }
    if has_oversized_skipped {
        parts.push("oversized_single_skipped".to_string());
    }
    parts.join("+")
}

fn partition_messages_by_token_limit(
    messages: &[SubAgentRunMessage],
    token_limit: i64,
) -> PartitionedMessages {
    if messages.is_empty() {
        return PartitionedMessages::default();
    }
    if token_limit <= 0 {
        return PartitionedMessages {
            summarizable: messages.to_vec(),
            oversized: Vec::new(),
        };
    }

    let mut summarizable = Vec::new();
    let mut oversized = Vec::new();
    for message in messages {
        let message_tokens = estimate_message_tokens_for_summary(message);
        if message_tokens > token_limit {
            oversized.push(message.clone());
        } else {
            summarizable.push(message.clone());
        }
    }

    PartitionedMessages {
        summarizable,
        oversized,
    }
}

fn estimate_message_tokens_for_summary(message: &SubAgentRunMessage) -> i64 {
    estimate_messages_tokens([pending_to_summary_message(message)].as_slice())
}

fn summarize_message_ids_preview(messages: &[SubAgentRunMessage], max_count: usize) -> Vec<String> {
    messages
        .iter()
        .take(max_count.max(1))
        .map(|message| message.id.clone())
        .collect()
}

fn build_oversized_only_summary_text(
    oversized_count: usize,
    oversized_ids_preview: &[String],
    token_limit: i64,
) -> String {
    let ids = if oversized_ids_preview.is_empty() {
        "none".to_string()
    } else {
        oversized_ids_preview.join(", ")
    };

    format!(
        "本批次待总结消息中有 {} 条消息单条超过 token_limit={}，已跳过正文并标记为已总结。消息 ID 预览：{}。",
        oversized_count, token_limit, ids
    )
}

fn append_oversized_skip_note(
    summary_text: String,
    oversized_count: usize,
    oversized_ids_preview: &[String],
) -> String {
    if oversized_count == 0 {
        return summary_text;
    }

    let ids = if oversized_ids_preview.is_empty() {
        "none".to_string()
    } else {
        oversized_ids_preview.join(", ")
    };

    format!(
        "{}\n\n补充说明：有 {} 条超长消息未纳入本次总结正文（已标记为已总结），消息 ID 预览：{}。",
        summary_text, oversized_count, ids
    )
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
    messages.iter().map(pending_to_summary_message).collect()
}

fn pending_to_summary_message(message: &SubAgentRunMessage) -> Value {
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

async fn resolve_runtime(effective: &EffectiveSummaryJobConfig) -> PromptRunnerRuntime {
    if let Some(model_config_id) = effective.model_config_id.as_deref() {
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
                    &effective.fallback_model,
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
        "model_name": effective.fallback_model,
        "provider": "gpt",
        "temperature": 0.2,
        "supports_responses": false,
    });
    PromptRunnerRuntime::from_ai_model_config(&fallback, &effective.fallback_model)
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        append_oversized_skip_note, build_oversized_only_summary_text, build_trigger_type,
        determine_trigger_kind, partition_messages_by_token_limit, TriggerKind,
    };
    use crate::models::sub_agent_run_message::SubAgentRunMessage;

    fn test_message(id: &str, content: &str) -> SubAgentRunMessage {
        SubAgentRunMessage {
            id: id.to_string(),
            run_id: "run_1".to_string(),
            role: "tool".to_string(),
            content: content.to_string(),
            tool_call_id: Some(format!("call_{}", id)),
            reasoning: None,
            metadata: Some(json!({ "source": "test" })),
            summary_status: Some("pending".to_string()),
            summary_id: None,
            summarized_at: None,
            created_at: "2026-03-05T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn partitions_oversized_messages() {
        let messages = vec![
            test_message("m1", &"a".repeat(200)),
            test_message("m2", "ok"),
        ];
        let partition = partition_messages_by_token_limit(messages.as_slice(), 10);
        assert_eq!(partition.oversized.len(), 1);
        assert_eq!(partition.summarizable.len(), 1);
        assert_eq!(partition.oversized[0].id, "m1");
        assert_eq!(partition.summarizable[0].id, "m2");
    }

    #[test]
    fn builds_trigger_type_with_suffixes() {
        assert_eq!(
            build_trigger_type(TriggerKind::MessageCountLimit, true, true),
            "message_count_limit+token_limit_split+oversized_single_skipped"
        );
        assert_eq!(
            build_trigger_type(TriggerKind::TokenLimit, false, false),
            "token_limit"
        );
    }

    #[test]
    fn oversized_summary_text_contains_ids() {
        let text =
            build_oversized_only_summary_text(2, &["m1".to_string(), "m2".to_string()], 6000);
        assert!(text.contains("2 条消息"));
        assert!(text.contains("m1"));
        assert!(text.contains("token_limit=6000"));
    }

    #[test]
    fn appends_oversized_note_when_needed() {
        let summary =
            append_oversized_skip_note("基础总结".to_string(), 1, &["message_x".to_string()]);
        assert!(summary.contains("基础总结"));
        assert!(summary.contains("message_x"));
        assert!(summary.contains("超长消息"));
    }

    #[test]
    fn determine_trigger_kind_uses_count_then_token() {
        assert_eq!(
            determine_trigger_kind(8, 10, 8, 6000),
            Some(TriggerKind::MessageCountLimit)
        );
        assert_eq!(
            determine_trigger_kind(3, 6001, 8, 6000),
            Some(TriggerKind::TokenLimit)
        );
        assert_eq!(determine_trigger_kind(3, 100, 8, 6000), None);
    }
}
