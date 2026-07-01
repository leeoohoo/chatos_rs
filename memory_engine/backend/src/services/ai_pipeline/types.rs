// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::future::Future;
use std::pin::Pin;

pub const MIN_TOKEN_LIMIT: i64 = 128;
pub(crate) const MAX_OVERFLOW_RETRIES: usize = 4;
pub(crate) const MAX_MERGE_ROUNDS: usize = 16;
pub(crate) const MIN_MERGE_TARGET_TOKENS: i64 = 256;

pub type ContinueCheck<'a> =
    dyn Fn() -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> + Send + Sync + 'a;

#[derive(Debug, Clone)]
pub struct SummaryBuildResult {
    pub text: String,
    pub chunk_count: usize,
    pub overflow_retry_count: usize,
}

pub struct SummarizeTextsOptions<'a> {
    pub prompt_title: &'a str,
    pub summary_prompt: Option<&'a str>,
    pub leaf_directive: &'a str,
    pub merge_directive: &'a str,
    pub token_limit: i64,
    pub target_tokens: Option<i64>,
    pub initial_token_limit_floor: i64,
    pub split_oversized_items: bool,
    pub log_label: &'a str,
    pub continue_check: Option<&'a ContinueCheck<'a>>,
}
