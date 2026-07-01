// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub fn is_context_overflow_error(err: &str) -> bool {
    let message = err.to_lowercase();
    message.contains("context_length_exceeded")
        || message.contains("input exceeds the context window")
        || message.contains("maximum context length")
        || message.contains("context window") && message.contains("exceed")
        || message.contains("context length")
        || message.contains("token limit")
        || message.contains("prompt is too long")
        || message.contains("too many tokens")
        || message.contains("max context")
}
