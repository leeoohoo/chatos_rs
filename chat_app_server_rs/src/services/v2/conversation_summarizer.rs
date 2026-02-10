use serde_json::{json, Value};
use tracing::{info, warn};

use crate::config::Config;
use crate::models::message::Message;
use crate::models::session_summary::{SessionSummary, SessionSummaryService};
use crate::models::session_summary_message::SessionSummaryMessageService;
use crate::services::v2::ai_request_handler::{AiRequestHandler, StreamCallbacks};
use crate::services::v2::message_manager::MessageManager;
use crate::utils::abort_registry;

fn estimate_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    (text.len() + 3) / 4
}

fn build_summarizer_system_prompt(target_tokens: i64) -> String {
    format!(
        "你是一名对话压缩专家。请将之前的对话（包含多次工具调用的结果）压缩为清晰、可追踪的上下文摘要。\n- 用中文输出\n- 严格保留重要事实、参数、路径、表名/字段名、ID 等关键细节\n- 去重冗余内容；保留结论与未解决的问题\n- 最终长度控制在约 {} tokens\n- 输出为自然文本，分点列出要点即可",
        target_tokens
    )
}

pub struct ConversationSummarizer {
    ai_request_handler: AiRequestHandler,
    message_manager: MessageManager,
    defaults: SummaryDefaults,
}

#[derive(Clone, Default)]
pub struct SummaryCallbacks {
    pub on_start: Option<std::sync::Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_stream: Option<std::sync::Arc<dyn Fn(String) + Send + Sync>>,
    pub on_end: Option<std::sync::Arc<dyn Fn(Value) + Send + Sync>>,
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

struct SummaryDefaults {
    message_limit: i64,
    max_context_tokens: i64,
    keep_last_n: i64,
    target_summary_tokens: i64,
    model: String,
    temperature: f64,
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
                keep_last_n: cfg.summary_keep_last_n,
                target_summary_tokens: cfg.summary_target_tokens,
                model: "gpt-4".to_string(),
                temperature: cfg.summary_temperature,
            },
        }
    }

    pub async fn maybe_summarize(
        &self,
        session_id: &str,
        opts: Option<SummaryOverrides>,
    ) -> Result<Option<(String, Vec<Value>)>, String> {
        let overrides = opts.unwrap_or_default();
        let message_limit = overrides
            .message_limit
            .unwrap_or(self.defaults.message_limit);
        let max_context_tokens = overrides
            .max_context_tokens
            .unwrap_or(self.defaults.max_context_tokens);
        let keep_last_n = overrides.keep_last_n.unwrap_or(self.defaults.keep_last_n);
        let target_summary_tokens = overrides
            .target_summary_tokens
            .unwrap_or(self.defaults.target_summary_tokens);
        let model = overrides
            .model
            .unwrap_or_else(|| self.defaults.model.clone());
        let temperature = overrides.temperature.unwrap_or(self.defaults.temperature);

        let all_messages = self
            .message_manager
            .get_session_messages(session_id, None)
            .await;
        let effective_records: Vec<Message> = all_messages
            .into_iter()
            .filter(|m| {
                m.metadata
                    .as_ref()
                    .and_then(|v| v.get("type"))
                    .and_then(|v| v.as_str())
                    != Some("session_summary")
            })
            .collect();
        let effective_messages: Vec<Value> = effective_records.iter().map(|m| {
            if m.role == "tool" {
                json!({"role": "tool", "tool_call_id": m.tool_call_id.clone().unwrap_or_default(), "content": m.content})
            } else {
                let mut msg = json!({"role": m.role, "content": m.content});
                if let Some(tc) = &m.tool_calls { msg["tool_calls"] = tc.clone(); }
                msg
            }
        }).collect();

        let total_tokens = estimate_messages_tokens(&effective_messages);

        let need = (effective_messages.len() as i64 >= message_limit)
            || (total_tokens >= max_context_tokens);
        info!(
            "[SUM] check: session={}, messages={}, tokens={}, need={}",
            session_id,
            effective_messages.len(),
            total_tokens,
            need
        );
        if !need {
            return Ok(None);
        }

        let keep_last_n = keep_last_n.max(0) as usize;
        let mut kept_start = effective_records.len().saturating_sub(keep_last_n);
        while kept_start > 0 && kept_start < effective_records.len() {
            if effective_records[kept_start].role != "tool" {
                break;
            }
            kept_start -= 1;
        }
        let kept = if keep_last_n > 0 {
            effective_messages[kept_start..].to_vec()
        } else {
            Vec::new()
        };
        let mut to_summarize = if keep_last_n > 0 {
            effective_messages[..kept_start].to_vec()
        } else {
            effective_messages.clone()
        };
        let mut to_summarize_records = if keep_last_n > 0 {
            effective_records[..kept_start].to_vec()
        } else {
            effective_records.clone()
        };
        if max_context_tokens > 0 {
            let total = estimate_messages_tokens(&to_summarize);
            if total > max_context_tokens {
                let truncated = truncate_messages_by_tokens(&to_summarize, max_context_tokens);
                let dropped = to_summarize.len().saturating_sub(truncated.len());
                if dropped > 0 && dropped <= to_summarize_records.len() {
                    to_summarize_records = to_summarize_records[dropped..].to_vec();
                }
                to_summarize = truncated;
            }
        }

        let system_prompt = build_summarizer_system_prompt(target_summary_tokens);
        let mut summarize_messages = Vec::new();
        summarize_messages.push(json!({"role": "system", "content": system_prompt}));
        summarize_messages.extend(to_summarize.clone());
        summarize_messages.push(json!({"role": "user", "content": "请基于以上对话与工具调用结果，生成用于继续对话的上下文摘要。"}));

        if abort_registry::is_aborted(session_id) {
            return Err("aborted".to_string());
        }

        let resp = self
            .ai_request_handler
            .handle_request(
                summarize_messages,
                None,
                model.clone(),
                Some(temperature),
                None,
                StreamCallbacks {
                    on_chunk: None,
                    on_thinking: None,
                },
                false,
                None,
                None,
                Some(session_id.to_string()),
                false,
                "summary",
            )
            .await?;

        let summary_text = resp.content.clone();
        let next_system_prompt = format!(
            "以下是之前对话与工具调用的摘要（可视为“压缩记忆”）：\n\n{}",
            summary_text
        );

        let mut summary_id: Option<String> = None;
        if !to_summarize_records.is_empty() {
            let mut record = SessionSummary::new(session_id.to_string(), summary_text.clone());
            record.summary_prompt = Some(system_prompt.clone());
            record.model = Some(model.clone());
            record.temperature = Some(temperature);
            record.target_summary_tokens = Some(target_summary_tokens);
            record.keep_last_n = Some(keep_last_n as i64);
            record.message_count = Some(to_summarize_records.len() as i64);
            record.approx_tokens = Some(estimate_messages_tokens(&to_summarize));
            record.first_message_id = to_summarize_records.first().map(|m| m.id.clone());
            record.last_message_id = to_summarize_records.last().map(|m| m.id.clone());
            record.first_message_created_at =
                to_summarize_records.first().map(|m| m.created_at.clone());
            record.last_message_created_at =
                to_summarize_records.last().map(|m| m.created_at.clone());

            let record_id = record.id.clone();
            match SessionSummaryService::create(record).await {
                Ok(_) => {
                    summary_id = Some(record_id.clone());
                    let message_ids: Vec<String> =
                        to_summarize_records.iter().map(|m| m.id.clone()).collect();
                    if let Err(err) = SessionSummaryMessageService::create_links(
                        &record_id,
                        session_id,
                        &message_ids,
                    )
                    .await
                    {
                        warn!("[SUM] create summary message links failed: {}", err);
                    }
                }
                Err(err) => {
                    warn!("[SUM] create summary record failed: {}", err);
                }
            }
        }

        // persist summary message
        let mut summary_meta = json!({"type": "session_summary", "keepLastN": keep_last_n as i64, "summary_timestamp": chrono::Utc::now().timestamp_millis()});
        if let Some(id) = summary_id.clone() {
            if let Some(map) = summary_meta.as_object_mut() {
                map.insert("summary_id".to_string(), Value::String(id));
            }
        }
        let _ = self
            .message_manager
            .save_assistant_message(
                session_id,
                "【上下文已压缩为摘要】",
                Some(summary_text.clone()),
                None,
                Some(summary_meta),
                None,
            )
            .await;

        Ok(Some((next_system_prompt, kept)))
    }

    pub async fn maybe_summarize_in_memory(
        &self,
        messages: &[Value],
        opts: Option<SummaryOverrides>,
        session_id: Option<String>,
        persist: bool,
        callbacks: Option<SummaryCallbacks>,
    ) -> Result<InMemorySummaryResult, String> {
        let overrides = opts.unwrap_or_default();
        let message_limit = overrides
            .message_limit
            .unwrap_or(self.defaults.message_limit);
        let max_context_tokens = overrides
            .max_context_tokens
            .unwrap_or(self.defaults.max_context_tokens);
        let keep_last_n = overrides
            .keep_last_n
            .unwrap_or(self.defaults.keep_last_n)
            .max(0) as usize;
        let target_summary_tokens = overrides
            .target_summary_tokens
            .unwrap_or(self.defaults.target_summary_tokens);
        let model = overrides
            .model
            .unwrap_or_else(|| self.defaults.model.clone());
        let temperature = overrides.temperature.unwrap_or(self.defaults.temperature);

        let mut total_tokens = 0i64;
        for msg in messages {
            if let Some(content) = msg.get("content") {
                total_tokens += estimate_tokens_value(content) as i64;
            }
        }

        let need = (messages.len() as i64 >= message_limit) || (total_tokens >= max_context_tokens);
        if !need {
            return Ok(InMemorySummaryResult::default());
        }

        let mut kept_start = messages.len().saturating_sub(keep_last_n);
        while kept_start > 0 && kept_start < messages.len() {
            if messages[kept_start].get("role").and_then(|v| v.as_str()) != Some("tool") {
                break;
            }
            kept_start -= 1;
        }
        let _kept = if keep_last_n > 0 {
            messages[kept_start..].to_vec()
        } else {
            Vec::new()
        };
        let mut to_summarize = if keep_last_n > 0 {
            messages[..kept_start].to_vec()
        } else {
            messages.to_vec()
        };
        if max_context_tokens > 0 {
            let total = estimate_messages_tokens(&to_summarize);
            if total > max_context_tokens {
                to_summarize = truncate_messages_by_tokens(&to_summarize, max_context_tokens);
            }
        }

        if let Some(cb) = callbacks.as_ref().and_then(|c| c.on_start.clone()) {
            cb(json!({ "keepLastN": keep_last_n, "summarize_count": to_summarize.len() }));
        }

        let system_prompt = build_summarizer_system_prompt(target_summary_tokens);
        let mut summarize_messages = Vec::new();
        summarize_messages.push(json!({"role": "system", "content": system_prompt}));
        summarize_messages.extend(to_summarize.clone());
        summarize_messages.push(json!({"role": "user", "content": "请基于以上对话与工具调用结果，生成用于继续对话的上下文摘要。"}));

        if let Some(sid) = session_id.as_ref() {
            if abort_registry::is_aborted(sid) {
                return Err("aborted".to_string());
            }
        }

        let stream_cb = callbacks.as_ref().and_then(|c| c.on_stream.clone());
        let resp = self
            .ai_request_handler
            .handle_request(
                summarize_messages,
                None,
                model.clone(),
                Some(temperature),
                None,
                StreamCallbacks {
                    on_chunk: stream_cb,
                    on_thinking: None,
                },
                false,
                None,
                None,
                session_id.clone(),
                callbacks
                    .as_ref()
                    .and_then(|c| c.on_stream.as_ref())
                    .is_some(),
                "summary",
            )
            .await?;

        let summary_text = resp.content.clone();
        let next_system_prompt = format!(
            "以下是之前对话与工具调用的摘要（可视为“压缩记忆”）：\n\n{}",
            summary_text
        );

        if persist {
            if let Some(sid) = session_id.as_ref() {
                let mut summary_id: Option<String> = None;
                let all_messages = self.message_manager.get_session_messages(sid, None).await;
                let effective_records: Vec<Message> = all_messages
                    .into_iter()
                    .filter(|m| {
                        m.metadata
                            .as_ref()
                            .and_then(|v| v.get("type"))
                            .and_then(|v| v.as_str())
                            != Some("session_summary")
                    })
                    .collect();

                if !effective_records.is_empty() {
                    let mut kept_start = effective_records.len().saturating_sub(keep_last_n);
                    while kept_start > 0 && kept_start < effective_records.len() {
                        if effective_records[kept_start].role != "tool" {
                            break;
                        }
                        kept_start -= 1;
                    }
                    let to_summarize_records = if keep_last_n > 0 {
                        effective_records[..kept_start].to_vec()
                    } else {
                        effective_records.clone()
                    };
                    if !to_summarize_records.is_empty() {
                        let mut record = SessionSummary::new(sid.to_string(), summary_text.clone());
                        record.summary_prompt = Some(system_prompt.clone());
                        record.model = Some(model.clone());
                        record.temperature = Some(temperature);
                        record.target_summary_tokens = Some(target_summary_tokens);
                        record.keep_last_n = Some(keep_last_n as i64);
                        record.message_count = Some(to_summarize_records.len() as i64);
                        record.approx_tokens = Some(estimate_messages_tokens(&to_summarize) as i64);
                        record.first_message_id =
                            to_summarize_records.first().map(|m| m.id.clone());
                        record.last_message_id = to_summarize_records.last().map(|m| m.id.clone());
                        record.first_message_created_at =
                            to_summarize_records.first().map(|m| m.created_at.clone());
                        record.last_message_created_at =
                            to_summarize_records.last().map(|m| m.created_at.clone());

                        let record_id = record.id.clone();
                        match SessionSummaryService::create(record).await {
                            Ok(_) => {
                                summary_id = Some(record_id.clone());
                                let message_ids: Vec<String> =
                                    to_summarize_records.iter().map(|m| m.id.clone()).collect();
                                if let Err(err) = SessionSummaryMessageService::create_links(
                                    &record_id,
                                    sid,
                                    &message_ids,
                                )
                                .await
                                {
                                    warn!("[SUM-MEM] create summary message links failed: {}", err);
                                }
                            }
                            Err(err) => {
                                warn!("[SUM-MEM] create summary record failed: {}", err);
                            }
                        }
                    }
                }

                let mut summary_meta = json!({"type": "session_summary", "keepLastN": keep_last_n as i64, "summary_timestamp": chrono::Utc::now().timestamp_millis()});
                if let Some(id) = summary_id.clone() {
                    if let Some(map) = summary_meta.as_object_mut() {
                        map.insert("summary_id".to_string(), Value::String(id));
                    }
                }
                let _ = self
                    .message_manager
                    .save_assistant_message(
                        sid,
                        "【上下文已压缩为摘要】",
                        Some(summary_text.clone()),
                        None,
                        Some(summary_meta),
                        None,
                    )
                    .await;
            }
        }

        let preview: String = summary_text.chars().take(800).collect();
        let truncated = summary_text.len() > preview.len();
        if let Some(cb) = callbacks.as_ref().and_then(|c| c.on_end.clone()) {
            cb(
                json!({ "summary_preview": preview, "full_summary": summary_text, "truncated": truncated, "keepLastN": keep_last_n }),
            );
        }

        Ok(InMemorySummaryResult {
            summarized: true,
            system_prompt: Some(next_system_prompt),
            summary: Some(summary_text),
            keep_last_n: keep_last_n,
            summarize_count: to_summarize.len(),
            truncated,
        })
    }
}

#[derive(Default)]
pub struct SummaryOverrides {
    pub message_limit: Option<i64>,
    pub max_context_tokens: Option<i64>,
    pub keep_last_n: Option<i64>,
    pub target_summary_tokens: Option<i64>,
    pub model: Option<String>,
    pub temperature: Option<f64>,
}

fn estimate_tokens_value(content: &Value) -> usize {
    if let Some(s) = content.as_str() {
        return estimate_tokens(s);
    }
    if let Some(arr) = content.as_array() {
        let mut sum = 0usize;
        for part in arr {
            if let Some(s) = part.as_str() {
                sum += estimate_tokens(s);
                continue;
            }
            if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                if let Some(ptype) = part.get("type").and_then(|v| v.as_str()) {
                    if ptype == "text" || ptype == "input_text" || ptype == "output_text" {
                        sum += estimate_tokens(text);
                        continue;
                    }
                }
                sum += estimate_tokens(text);
            }
        }
        return sum;
    }
    if let Some(obj) = content.as_object() {
        if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
            return estimate_tokens(text);
        }
        return estimate_tokens(&content.to_string());
    }
    0
}

fn estimate_messages_tokens(messages: &[Value]) -> i64 {
    let mut total = 0i64;
    for msg in messages {
        let mut t = estimate_tokens_value(msg.get("content").unwrap_or(&Value::Null)) as i64;
        if let Some(tc) = msg.get("tool_calls") {
            t += estimate_tokens(&tc.to_string()) as i64;
        }
        total += t;
    }
    total
}

fn truncate_messages_by_tokens(messages: &[Value], max_tokens: i64) -> Vec<Value> {
    if max_tokens <= 0 || messages.is_empty() {
        return Vec::new();
    }
    let mut remaining = max_tokens;
    let mut out_rev: Vec<Value> = Vec::new();
    for msg in messages.iter().rev() {
        let t = estimate_tokens_value(msg.get("content").unwrap_or(&Value::Null)) as i64;
        if remaining - t < 0 {
            if out_rev.is_empty() && remaining > 0 {
                out_rev.push(truncate_message_content(msg, remaining));
            }
            break;
        }
        remaining -= t;
        out_rev.push(msg.clone());
    }
    out_rev.reverse();
    out_rev
}

fn truncate_message_content(msg: &Value, max_tokens: i64) -> Value {
    if max_tokens <= 0 {
        return msg.clone();
    }
    let mut out = msg.clone();
    if let Some(obj) = out.as_object_mut() {
        let content = obj.get("content").cloned().unwrap_or(Value::Null);
        obj.insert(
            "content".to_string(),
            truncate_content_value(&content, max_tokens),
        );
    }
    out
}

fn truncate_content_value(content: &Value, max_tokens: i64) -> Value {
    if max_tokens <= 0 {
        return Value::String(String::new());
    }
    if let Some(s) = content.as_str() {
        return Value::String(truncate_text_by_tokens(s, max_tokens));
    }
    if let Some(arr) = content.as_array() {
        let mut out = Vec::new();
        let mut remaining = max_tokens;
        for part in arr {
            if remaining <= 0 {
                break;
            }
            if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                let truncated = truncate_text_by_tokens(text, remaining);
                let used = estimate_tokens(&truncated) as i64;
                let mut new_part = part.clone();
                if let Some(map) = new_part.as_object_mut() {
                    map.insert("text".to_string(), Value::String(truncated));
                }
                out.push(new_part);
                remaining -= used;
                continue;
            }
            if let Some(s) = part.as_str() {
                let truncated = truncate_text_by_tokens(s, remaining);
                let used = estimate_tokens(&truncated) as i64;
                out.push(Value::String(truncated));
                remaining -= used;
                continue;
            }
            out.push(part.clone());
        }
        return Value::Array(out);
    }
    Value::String(truncate_text_by_tokens(&content.to_string(), max_tokens))
}

fn truncate_text_by_tokens(text: &str, max_tokens: i64) -> String {
    if max_tokens <= 0 {
        return String::new();
    }
    let max_chars = (max_tokens * 4) as usize;
    if text.len() <= max_chars {
        return text.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    let marker = "\n...[truncated]";
    if max_chars <= marker.len() {
        return marker[..max_chars].to_string();
    }
    let cut = max_chars - marker.len();
    format!("{}{}", &text[..cut], marker)
}
