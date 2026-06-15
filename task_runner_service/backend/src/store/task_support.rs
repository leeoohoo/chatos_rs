use chrono::{DateTime, Utc};

use crate::models::{
    PaginatedResponse, TaskRecord, TaskScheduleMode, TaskStatsResponse, TaskStatus,
};

pub(super) const DEFAULT_PAGE_LIMIT: usize = 20;

pub(super) fn task_matches_keyword(task: &TaskRecord, keyword: &str) -> bool {
    let contains = |value: &str| value.to_ascii_lowercase().contains(keyword);
    contains(&task.title)
        || contains(&task.objective)
        || task.description.as_deref().is_some_and(contains)
        || task.result_summary.as_deref().is_some_and(contains)
        || contains(&task.id)
        || task.tags.iter().any(|tag| contains(tag))
}

pub(super) fn empty_task_stats() -> TaskStatsResponse {
    TaskStatsResponse {
        total: 0,
        scheduled: 0,
        follow_up: 0,
        draft: 0,
        ready: 0,
        queued: 0,
        running: 0,
        succeeded: 0,
        failed: 0,
        blocked: 0,
        cancelled: 0,
        archived: 0,
    }
}

pub(super) fn task_due_for_scheduler(task: &TaskRecord, now: &DateTime<Utc>) -> bool {
    if matches!(
        task.status,
        TaskStatus::Archived | TaskStatus::Queued | TaskStatus::Running
    ) {
        return false;
    }
    if matches!(task.schedule.mode, TaskScheduleMode::Manual) {
        return false;
    }
    task_due_at(task).is_some_and(|value| value <= now.to_owned())
}

pub(super) fn task_due_at(task: &TaskRecord) -> Option<DateTime<Utc>> {
    task.schedule
        .next_run_at
        .as_deref()
        .and_then(parse_rfc3339_utc)
}

fn parse_rfc3339_utc(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|item| item.with_timezone(&Utc))
}

pub(super) fn apply_offset_limit<T>(
    items: &mut Vec<T>,
    offset: Option<usize>,
    limit: Option<usize>,
) {
    let offset = offset.unwrap_or(0);
    if offset >= items.len() {
        items.clear();
        return;
    }
    if offset > 0 {
        items.drain(0..offset);
    }
    if let Some(limit) = limit {
        items.truncate(limit);
    }
}

pub(super) fn slice_page_items<T>(items: Vec<T>, offset: usize, limit: usize) -> Vec<T> {
    items.into_iter().skip(offset).take(limit).collect()
}

pub(super) fn build_page_response<T>(
    items: Vec<T>,
    total: usize,
    limit: usize,
    offset: usize,
) -> PaginatedResponse<T> {
    let has_more = offset.saturating_add(items.len()) < total;
    PaginatedResponse {
        items,
        total,
        limit,
        offset,
        has_more,
    }
}
