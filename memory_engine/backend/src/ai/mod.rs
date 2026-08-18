// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod client;
mod parsing;
mod protocol;

pub(crate) use client::{AiClient, AiGenerateTextError, SUMMARY_SYSTEM_PROMPT};

#[cfg(test)]
mod tests;
