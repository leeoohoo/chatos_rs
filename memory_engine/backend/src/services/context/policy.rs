// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::ComposeContextPolicy;

const DEFAULT_PENDING_RECORD_LIMIT: i64 = 10_000;
const DEFAULT_SUMMARY_LIMIT: i64 = 2;

pub(crate) struct ResolvedComposeContextPolicy {
    pub(crate) include_recent_records: bool,
    pub(crate) include_thread_summary: bool,
    pub(crate) include_subject_memory: bool,
    pub(crate) recent_limit: i64,
    pub(crate) summary_limit: i64,
}

impl ResolvedComposeContextPolicy {
    pub(crate) fn from_request(policy: Option<&ComposeContextPolicy>) -> Self {
        Self {
            include_recent_records: policy
                .and_then(|item| item.include_recent_records)
                .unwrap_or(true),
            include_thread_summary: policy
                .and_then(|item| item.include_thread_summary)
                .unwrap_or(true),
            include_subject_memory: policy
                .and_then(|item| item.include_subject_memory)
                .unwrap_or(true),
            recent_limit: policy
                .and_then(|item| item.recent_record_limit)
                .unwrap_or(DEFAULT_PENDING_RECORD_LIMIT as usize)
                .max(1) as i64,
            summary_limit: policy
                .and_then(|item| item.summary_limit)
                .unwrap_or(DEFAULT_SUMMARY_LIMIT as usize)
                .max(1) as i64,
        }
    }
}
