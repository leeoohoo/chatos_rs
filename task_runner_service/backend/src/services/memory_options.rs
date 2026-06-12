use memory_engine_sdk::ComposeContextPolicy;

use crate::models::{TaskMemoryContextOptions, TaskMemoryRecordsOptions};

use super::normalized_optional;

pub(super) fn sanitize_task_memory_context_policy(
    options: TaskMemoryContextOptions,
) -> ComposeContextPolicy {
    ComposeContextPolicy {
        include_recent_records: Some(options.include_recent_records.unwrap_or(true)),
        include_thread_summary: Some(options.include_thread_summary.unwrap_or(true)),
        include_subject_memory: Some(options.include_subject_memory.unwrap_or(false)),
        recent_record_limit: Some(options.recent_record_limit.unwrap_or(12).clamp(1, 100)),
        summary_limit: Some(options.summary_limit.unwrap_or(6).clamp(1, 50)),
    }
}

pub(super) fn sanitize_task_memory_records_options(
    options: TaskMemoryRecordsOptions,
) -> TaskMemoryRecordsOptions {
    let limit = options.limit.unwrap_or(50).clamp(1, 200);
    let offset = options.offset.unwrap_or(0).max(0);
    let order = normalized_optional(options.order)
        .map(|value| {
            if value.eq_ignore_ascii_case("asc") {
                "asc".to_string()
            } else {
                "desc".to_string()
            }
        })
        .unwrap_or_else(|| "desc".to_string());

    TaskMemoryRecordsOptions {
        role: normalized_optional(options.role),
        record_type: normalized_optional(options.record_type),
        summary_status: normalized_optional(options.summary_status),
        limit: Some(limit),
        offset: Some(offset),
        order: Some(order),
    }
}
