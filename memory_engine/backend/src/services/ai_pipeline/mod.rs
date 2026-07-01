// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod chunking;
mod input;
mod overflow;
mod pipeline;
#[cfg(test)]
mod tests;
mod types;

pub use chunking::estimate_tokens_text;
#[allow(unused_imports)]
pub use overflow::is_context_overflow_error;
pub use pipeline::summarize_texts_with_split;
pub use types::{ContinueCheck, SummarizeTextsOptions, SummaryBuildResult, MIN_TOKEN_LIMIT};

pub(crate) use types::{MAX_MERGE_ROUNDS, MAX_OVERFLOW_RETRIES, MIN_MERGE_TARGET_TOKENS};
