// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

mod dependencies;
mod listing;
mod memory;
mod mutations;

pub(super) use self::dependencies::{
    get_task_dependency_graph, list_task_prerequisites, set_task_prerequisites,
};
pub(super) use self::listing::{
    get_task_index, get_task_stats, list_task_summaries, list_tasks, list_tasks_page,
};
pub(super) use self::memory::{
    get_task_memory_context, get_task_memory_records, summarize_task_memory,
};
pub(super) use self::mutations::{
    batch_delete_tasks, batch_start_task_runs, batch_update_task_status, cancel_task, create_task,
    delete_task, get_task, get_task_mcp_resolution, preview_task_mcp_prompt, record_task_process,
    update_task, update_task_mcp,
};

#[derive(Debug, Default, Deserialize)]
pub(super) struct TaskListQuery {
    status: Option<TaskStatus>,
    keyword: Option<String>,
    tag: Option<String>,
    model_config_id: Option<String>,
    project_id: Option<String>,
    scheduled_only: Option<bool>,
    parent_task_id: Option<String>,
    include_subtasks: Option<bool>,
    source_run_id: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
}

impl TaskListQuery {
    fn into_filters(self) -> TaskListFilters {
        TaskListFilters {
            status: self.status,
            keyword: self.keyword,
            tag: self.tag,
            model_config_id: self.model_config_id,
            project_id: self.project_id,
            creator_user_id: None,
            scheduled_only: self.scheduled_only,
            parent_task_id: self.parent_task_id,
            include_subtasks: Some(self.include_subtasks.unwrap_or(false)),
            source_run_id: self.source_run_id,
            limit: self.limit,
            offset: self.offset,
            ..TaskListFilters::default()
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct TaskSummaryQuery {
    ids: Option<String>,
    keyword: Option<String>,
    status: Option<TaskStatus>,
    project_id: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct TaskMemoryContextQuery {
    include_recent_records: Option<bool>,
    include_thread_summary: Option<bool>,
    include_subject_memory: Option<bool>,
    recent_record_limit: Option<usize>,
    summary_limit: Option<usize>,
}

impl TaskMemoryContextQuery {
    fn into_options(self) -> TaskMemoryContextOptions {
        TaskMemoryContextOptions {
            include_recent_records: self.include_recent_records,
            include_thread_summary: self.include_thread_summary,
            include_subject_memory: self.include_subject_memory,
            recent_record_limit: self.recent_record_limit,
            summary_limit: self.summary_limit,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct TaskMemoryRecordsQuery {
    role: Option<String>,
    record_type: Option<String>,
    summary_status: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
    order: Option<String>,
}

impl TaskMemoryRecordsQuery {
    fn into_options(self) -> TaskMemoryRecordsOptions {
        TaskMemoryRecordsOptions {
            role: self.role,
            record_type: self.record_type,
            summary_status: self.summary_status,
            limit: self.limit,
            offset: self.offset,
            order: self.order,
        }
    }
}
