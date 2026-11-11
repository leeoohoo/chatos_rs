// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::{now_rfc3339, EngineJobPolicy};

pub(crate) const STALE_LEGACY_REPAIR_JOB_TIMEOUT_SECS: i64 = 1800;

pub(crate) const JOB_TYPE_SUMMARY: &str = "summary";
pub(crate) const JOB_TYPE_ROLLUP: &str = "rollup";
pub(crate) const JOB_TYPE_SUBJECT_MEMORY: &str = "subject_memory";
pub(crate) const LEGACY_JOB_TYPE_THREAD_REPAIR: &str = "thread_repair";

pub fn default_job_types() -> &'static [&'static str] {
    &[JOB_TYPE_SUMMARY, JOB_TYPE_ROLLUP, JOB_TYPE_SUBJECT_MEMORY]
}

fn empty_policy(job_type: &str) -> EngineJobPolicy {
    EngineJobPolicy {
        job_type: job_type.to_string(),
        dispatch_threshold_tokens: None,
        max_source_batch_tokens: None,
        max_model_input_tokens: None,
        max_leaf_tokens: None,
        max_merge_rounds: None,
        count_limit: None,
        keep_level0_count: None,
        max_level: None,
        updated_at: now_rfc3339(),
    }
}

pub fn default_summary_job_policy() -> EngineJobPolicy {
    empty_policy(JOB_TYPE_SUMMARY)
}

pub fn default_rollup_job_policy() -> EngineJobPolicy {
    empty_policy(JOB_TYPE_ROLLUP)
}

pub fn default_subject_memory_job_policy() -> EngineJobPolicy {
    empty_policy(JOB_TYPE_SUBJECT_MEMORY)
}

pub(crate) fn default_job_policy(job_type: &str) -> EngineJobPolicy {
    match job_type.trim() {
        JOB_TYPE_SUMMARY => default_summary_job_policy(),
        JOB_TYPE_ROLLUP => default_rollup_job_policy(),
        JOB_TYPE_SUBJECT_MEMORY => default_subject_memory_job_policy(),
        other => empty_policy(other),
    }
}

pub(crate) fn normalize_job_policy(policy: &mut EngineJobPolicy) {
    policy.dispatch_threshold_tokens = policy.dispatch_threshold_tokens.map(|value| value.max(1));
    policy.max_source_batch_tokens = policy.max_source_batch_tokens.map(|value| value.max(128));
    policy.max_model_input_tokens = policy.max_model_input_tokens.map(|value| value.max(128));
    policy.max_leaf_tokens = policy.max_leaf_tokens.map(|value| value.max(128));
    policy.max_merge_rounds = policy.max_merge_rounds.map(|value| value.max(1));
    policy.count_limit = policy.count_limit.map(|value| value.max(0));
    policy.keep_level0_count = policy.keep_level0_count.map(|value| value.max(0));
    policy.max_level = policy.max_level.map(|value| value.max(1));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_canonical_generation_policies_are_public() {
        assert_eq!(
            default_job_types(),
            &[JOB_TYPE_SUMMARY, JOB_TYPE_ROLLUP, JOB_TYPE_SUBJECT_MEMORY]
        );
    }

    #[test]
    fn policy_limits_are_normalized_without_legacy_prompt_or_enable_state() {
        let mut policy = default_summary_job_policy();
        policy.dispatch_threshold_tokens = Some(0);
        policy.max_model_input_tokens = Some(64);
        policy.max_merge_rounds = Some(0);
        normalize_job_policy(&mut policy);
        assert_eq!(policy.dispatch_threshold_tokens, Some(1));
        assert_eq!(policy.max_model_input_tokens, Some(128));
        assert_eq!(policy.max_merge_rounds, Some(1));
    }
}
