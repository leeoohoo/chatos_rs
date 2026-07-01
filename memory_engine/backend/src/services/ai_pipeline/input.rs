// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub(crate) fn build_ai_input(
    summary_prompt: Option<&str>,
    directive: &str,
    items: &[String],
) -> String {
    let custom_prefix = summary_prompt
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("{value}\n\n"))
        .unwrap_or_default();
    let body = items.join("\n\n---\n\n");
    format!("{custom_prefix}{directive}\n\nSource items:\n{body}")
}
