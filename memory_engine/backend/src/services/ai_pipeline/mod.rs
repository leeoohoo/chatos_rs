// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod chunking;
pub(crate) mod cloud_agent;
mod input;
mod overflow;
pub(crate) mod profiles;
#[cfg(test)]
mod tests;
mod types;

pub use chunking::estimate_tokens_text;
#[allow(unused_imports)]
pub use overflow::is_context_overflow_error;
pub use types::SummaryBuildResult;
#[cfg(test)]
pub use types::CHUNK_TEST_CAPACITY;

pub(crate) use types::MAX_OVERFLOW_RETRIES;
