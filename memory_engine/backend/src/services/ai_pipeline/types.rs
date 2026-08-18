// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub const MIN_TOKEN_LIMIT: i64 = 128;
pub(crate) const MAX_OVERFLOW_RETRIES: usize = 4;
pub(crate) const MAX_MERGE_ROUNDS: usize = 16;
pub(crate) const MIN_MERGE_TARGET_TOKENS: i64 = 256;

#[derive(Debug, Clone)]
pub struct SummaryBuildResult {
    pub text: String,
    pub chunk_count: usize,
    pub overflow_retry_count: usize,
}
