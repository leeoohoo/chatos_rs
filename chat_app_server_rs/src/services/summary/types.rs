use std::sync::Arc;

use serde_json::Value;

#[derive(Debug, Clone)]
pub struct SummaryOptions {
    pub message_limit: i64,
    pub max_context_tokens: i64,
    pub keep_last_n: usize,
    pub target_summary_tokens: i64,
    pub merge_target_tokens: i64,
    pub model: String,
    pub temperature: f64,
    pub bisect_enabled: bool,
    pub bisect_max_depth: usize,
    pub bisect_min_messages: usize,
    pub retry_on_context_overflow: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummaryTrigger {
    Proactive,
    OverflowRetry,
}

impl SummaryTrigger {
    pub fn as_str(self) -> &'static str {
        match self {
            SummaryTrigger::Proactive => "proactive",
            SummaryTrigger::OverflowRetry => "overflow_retry",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummaryTriggerReason {
    MessageLimit,
    TokenLimit,
    OverflowRetry,
}

impl SummaryTriggerReason {
    pub fn as_str(self) -> &'static str {
        match self {
            SummaryTriggerReason::MessageLimit => "message_limit",
            SummaryTriggerReason::TokenLimit => "token_limit",
            SummaryTriggerReason::OverflowRetry => "overflow_retry",
        }
    }
}

#[derive(Clone, Default)]
pub struct SummaryCallbacks {
    pub on_start: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_stream: Option<Arc<dyn Fn(String) + Send + Sync>>,
    pub on_end: Option<Arc<dyn Fn(Value) + Send + Sync>>,
}

#[derive(Debug, Clone, Default)]
pub struct SummaryStats {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub chunk_count: usize,
    pub max_depth: usize,
    pub compression_ratio: f64,
}

#[derive(Debug, Clone, Default)]
pub struct SummaryResult {
    pub summarized: bool,
    pub summary_text: Option<String>,
    pub system_prompt: Option<String>,
    pub kept_messages: Vec<Value>,
    pub summarized_messages: Vec<Value>,
    pub truncated: bool,
    pub stats: SummaryStats,
}

#[derive(Clone)]
pub struct SummaryLlmRequest {
    pub context_messages: Vec<Value>,
    pub target_tokens: i64,
    pub model: String,
    pub temperature: f64,
    pub session_id: Option<String>,
    pub stream: bool,
    pub callbacks: Option<SummaryCallbacks>,
}

#[derive(Debug, Clone, Default)]
pub struct SummarySourceInfo {
    pub message_ids: Vec<String>,
    pub first_message_id: Option<String>,
    pub last_message_id: Option<String>,
    pub first_message_created_at: Option<String>,
    pub last_message_created_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PersistSummaryPayload {
    pub session_id: String,
    pub summary_text: String,
    pub summary_prompt: String,
    pub model: String,
    pub temperature: f64,
    pub target_summary_tokens: i64,
    pub keep_last_n: i64,
    pub approx_tokens: i64,
    pub trigger: SummaryTrigger,
    pub truncated: bool,
    pub stats: SummaryStats,
    pub source: SummarySourceInfo,
}

#[derive(Debug, Clone, Default)]
pub struct PersistSummaryOutcome {
    pub summary_id: Option<String>,
}

pub fn build_summarizer_system_prompt(target_tokens: i64) -> String {
    format!(
        "你是一名对话压缩专家。请将之前的对话（包含多次工具调用的结果）压缩为清晰、可追踪的上下文摘要。\n- 用中文输出\n- 严格保留重要事实、参数、路径、表名/字段名、ID 等关键细节\n- 去重冗余内容；保留结论与未解决的问题\n- 最终长度控制在约 {} tokens\n- 输出为自然文本，分点列出要点即可",
        target_tokens
    )
}

pub fn build_summary_user_prompt() -> &'static str {
    "请基于以上对话与工具调用结果，生成用于继续对话的上下文摘要。"
}

pub fn wrap_summary_as_system_prompt(summary: &str) -> String {
    format!(
        "以下是之前对话与工具调用的摘要（可视为“压缩记忆”）：\n\n{}",
        summary
    )
}
