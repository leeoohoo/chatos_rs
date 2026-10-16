// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_runtime::ToolResult;

#[derive(Debug, Clone)]
pub struct ToolResultModelBudget {
    per_result_max_chars: usize,
    remaining_total_chars: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolResultModelBudgetLimits {
    pub per_result_max_chars: usize,
    pub total_max_chars: usize,
}

pub const DEFAULT_TOOL_RESULT_MODEL_MAX_CHARS: usize = 160_000;
pub const DEFAULT_TOOL_RESULTS_MODEL_TOTAL_MAX_CHARS: usize = 800_000;
pub const TOOL_RESULT_MODEL_MAX_CHARS_ENV: &str = "AI_RUNTIME_TOOL_RESULT_MODEL_MAX_CHARS";
pub const TOOL_RESULTS_MODEL_TOTAL_MAX_CHARS_ENV: &str =
    "AI_RUNTIME_TOOL_RESULTS_MODEL_TOTAL_MAX_CHARS";

impl ToolResultModelBudget {
    pub fn from_env() -> Self {
        Self::from_limits(ToolResultModelBudgetLimits::from_env())
    }

    pub fn from_limits(limits: ToolResultModelBudgetLimits) -> Self {
        Self::new(limits.per_result_max_chars, limits.total_max_chars)
    }

    pub fn new(per_result_max_chars: usize, total_max_chars: usize) -> Self {
        Self {
            per_result_max_chars: per_result_max_chars.max(1),
            remaining_total_chars: total_max_chars.max(1),
        }
    }

    pub fn sanitize_content(&mut self, tool_name: &str, content: &str) -> String {
        let content_chars = content.chars().count();
        if content_chars <= self.per_result_max_chars && content_chars <= self.remaining_total_chars
        {
            self.remaining_total_chars = self.remaining_total_chars.saturating_sub(content_chars);
            return content.to_string();
        }

        let (reason, available_chars) = if content_chars > self.per_result_max_chars {
            (
                "per_result_limit",
                self.per_result_max_chars.min(self.remaining_total_chars),
            )
        } else {
            ("total_budget", self.remaining_total_chars)
        };
        let bounded = tool_result_budget_error(
            tool_name,
            content,
            content_chars,
            content.len(),
            reason,
            available_chars,
        );
        self.remaining_total_chars = self
            .remaining_total_chars
            .saturating_sub(bounded.chars().count());
        bounded
    }
}

impl ToolResultModelBudgetLimits {
    pub fn from_env() -> Self {
        Self::new(
            env_usize(
                TOOL_RESULT_MODEL_MAX_CHARS_ENV,
                DEFAULT_TOOL_RESULT_MODEL_MAX_CHARS,
            ),
            env_usize(
                TOOL_RESULTS_MODEL_TOTAL_MAX_CHARS_ENV,
                DEFAULT_TOOL_RESULTS_MODEL_TOTAL_MAX_CHARS,
            ),
        )
    }

    pub fn new(per_result_max_chars: usize, total_max_chars: usize) -> Self {
        Self {
            per_result_max_chars: per_result_max_chars.max(1),
            total_max_chars: total_max_chars.max(1),
        }
    }
}

pub fn sanitize_tool_results_for_model(results: Vec<ToolResult>) -> Vec<ToolResult> {
    sanitize_tool_results_for_model_with_budget(results, None)
}

pub fn sanitize_tool_results_for_model_with_budget(
    results: Vec<ToolResult>,
    limits: Option<ToolResultModelBudgetLimits>,
) -> Vec<ToolResult> {
    let mut budget = limits
        .map(ToolResultModelBudget::from_limits)
        .unwrap_or_else(ToolResultModelBudget::from_env);
    results
        .into_iter()
        .map(|mut result| {
            result.content = budget.sanitize_content(result.name.as_str(), result.content.as_str());
            result
        })
        .collect()
}

fn env_usize(key: &str, default_value: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default_value)
}

fn tool_result_budget_error(
    tool_name: &str,
    content: &str,
    content_chars: usize,
    content_bytes: usize,
    reason: &str,
    max_chars: usize,
) -> String {
    let tool_name = tool_name.trim();
    let tool_display = if tool_name.is_empty() {
        "unknown"
    } else {
        tool_name
    };
    let _ = content;
    serde_json::json!({
        "error": "tool_result_budget_exceeded",
        "tool": tool_display,
        "original_chars": content_chars,
        "original_bytes": content_bytes,
        "reason": reason,
        "available_chars": max_chars,
        "content_preserved": false,
        "guidance": "No partial head/tail excerpt was supplied. Use explicit pagination, a managed artifact, or a durable tool_result_read reference."
    })
    .to_string()
}
