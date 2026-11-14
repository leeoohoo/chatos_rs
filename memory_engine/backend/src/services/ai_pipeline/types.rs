// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(test)]
pub const CHUNK_TEST_CAPACITY: i64 = 128;
pub(crate) const MAX_OVERFLOW_RETRIES: usize = 4;

#[derive(Debug, Clone)]
pub struct SummaryBuildResult {
    pub text: String,
    pub structured_payload: serde_json::Value,
    pub chunk_count: usize,
    pub overflow_retry_count: usize,
}
