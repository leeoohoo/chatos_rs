use crate::models::{normalize_project_id, PromptListFilters, RunListFilters, TaskListFilters};

use super::normalized_optional;

pub(super) fn sanitize_task_list_filters(mut filters: TaskListFilters) -> TaskListFilters {
    filters.keyword = normalized_optional(filters.keyword).map(|value| value.to_ascii_lowercase());
    filters.tag = normalized_optional(filters.tag);
    filters.model_config_id = normalized_optional(filters.model_config_id);
    filters.project_id =
        normalized_optional(filters.project_id).map(|value| normalize_project_id(Some(value)));
    filters.creator_user_id = normalized_optional(filters.creator_user_id);
    filters.parent_task_id = normalized_optional(filters.parent_task_id);
    filters.source_run_id = normalized_optional(filters.source_run_id);
    filters.source_session_id = normalized_optional(filters.source_session_id);
    filters.source_user_message_ids = filters
        .source_user_message_ids
        .into_iter()
        .filter_map(|value| normalized_optional(Some(value)))
        .collect();
    filters.source_turn_ids = filters
        .source_turn_ids
        .into_iter()
        .filter_map(|value| normalized_optional(Some(value)))
        .collect();
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
