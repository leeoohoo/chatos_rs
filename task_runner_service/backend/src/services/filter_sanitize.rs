use crate::models::{PromptListFilters, RunListFilters, TaskListFilters};

use super::normalized_optional;

pub(super) fn sanitize_task_list_filters(mut filters: TaskListFilters) -> TaskListFilters {
    filters.keyword = normalized_optional(filters.keyword).map(|value| value.to_ascii_lowercase());
    filters.tag = normalized_optional(filters.tag);
    filters.model_config_id = normalized_optional(filters.model_config_id);
    filters.creator_user_id = normalized_optional(filters.creator_user_id);
    filters.parent_task_id = normalized_optional(filters.parent_task_id);
    filters.source_run_id = normalized_optional(filters.source_run_id);
    filters.limit = filters.limit.map(|value| value.clamp(1, 500));
    filters.offset = filters.offset.map(|value| value.min(100_000));
    filters
}

pub(super) fn sanitize_run_list_filters(mut filters: RunListFilters) -> RunListFilters {
    filters.task_id = normalized_optional(filters.task_id);
    filters.model_config_id = normalized_optional(filters.model_config_id);
    filters.keyword = normalized_optional(filters.keyword).map(|value| value.to_ascii_lowercase());
    filters.limit = filters.limit.map(|value| value.clamp(1, 500));
    filters.offset = filters.offset.map(|value| value.min(100_000));
    filters
}

pub(crate) fn sanitize_prompt_list_filters(mut filters: PromptListFilters) -> PromptListFilters {
    filters.task_id = normalized_optional(filters.task_id);
    filters.run_id = normalized_optional(filters.run_id);
    filters.limit = Some(filters.limit.unwrap_or(20).clamp(1, 500));
    filters.offset = Some(filters.offset.unwrap_or(0).min(100_000));
    filters
}
