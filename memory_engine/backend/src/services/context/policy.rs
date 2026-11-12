// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::ComposeContextPolicy;

pub(crate) const CONTEXT_POLICY_REVISION: &str = "memory-compose-v3-model-capacity";
pub(crate) struct ResolvedComposeContextPolicy {
    pub(crate) max_memory_tokens: usize,
    pub(crate) max_recent_record_tokens: usize,
    pub(crate) max_thread_summary_tokens: usize,
    pub(crate) max_project_memory_tokens: usize,
    pub(crate) max_agent_memory_tokens: usize,
    pub(crate) max_relevant_history_tokens: usize,
}

impl ResolvedComposeContextPolicy {
    pub(crate) fn from_request(policy: &ComposeContextPolicy) -> Result<Self, String> {
        let requested = [
            policy.max_memory_tokens,
            policy.max_recent_record_tokens,
            policy.max_thread_summary_tokens,
            policy.max_project_memory_tokens,
            policy.max_agent_memory_tokens,
        ];
        if requested.into_iter().any(|value| value == 0) {
            return Err("all compose token budgets must be greater than zero".to_string());
        }
        let total = policy.max_memory_tokens;
        let resolved = Self {
            max_memory_tokens: total,
            max_recent_record_tokens: policy.max_recent_record_tokens.min(total),
            max_thread_summary_tokens: policy.max_thread_summary_tokens.min(total),
            max_project_memory_tokens: policy.max_project_memory_tokens.min(total),
            max_agent_memory_tokens: policy.max_agent_memory_tokens.min(total),
            max_relevant_history_tokens: total,
        };
        if resolved
            .max_recent_record_tokens
            .saturating_add(resolved.max_thread_summary_tokens)
            .saturating_add(resolved.max_project_memory_tokens)
            .saturating_add(resolved.max_agent_memory_tokens)
            < resolved.max_memory_tokens
        {
            return Err("compose pool budgets cannot cover max_memory_tokens".to_string());
        }
        Ok(resolved)
    }
}
