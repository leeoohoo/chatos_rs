// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod client;
mod parsing;
mod protocol;
mod retry;

pub(crate) use client::{AiClient, AiGenerateTextError, SUMMARY_SYSTEM_PROMPT};
pub(crate) use retry::transient_retry_backoff_ms;

#[cfg(test)]
mod tests;
