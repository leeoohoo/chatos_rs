// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod common;
mod dispatch;
mod queries;
mod status;
mod subject_dispatch;
mod writes;

pub use dispatch::{
    get_pending_rollup_dispatch, get_rollup_dispatch_state, list_pending_rollup_dispatches,
    mark_rollup_dispatch_consumed, mark_rollup_dispatch_dead_lettered, mark_rollup_dispatch_failed,
    mark_rollup_dispatch_published, rearm_rollup_dispatch_if_eligible,
    replay_dead_lettered_rollup_dispatch, RollupDispatchOutbox,
};
pub use subject_dispatch::{
    get_pending_subject_memory_source_dispatch, get_subject_memory_source_dispatch_state,
    list_pending_subject_memory_source_dispatches, mark_subject_memory_source_dispatch_consumed,
    mark_subject_memory_source_dispatch_dead_lettered, mark_subject_memory_source_dispatch_failed,
    mark_subject_memory_source_dispatch_published,
    replay_dead_lettered_subject_memory_source_dispatch, SubjectMemorySourceDispatchOutbox,
};

#[allow(unused_imports)]
pub use queries::{
    find_summary_by_source_digest, list_latest_thread_summaries,
    list_latest_thread_summaries_at_level, list_latest_thread_summaries_by_type,
    list_pending_summaries_by_level, list_summaries_by_thread_label,
    list_summaries_by_thread_label_for_subject_memory_scope, list_thread_summaries,
    list_threads_with_pending_rollups,
};
#[allow(unused_imports)]
pub use status::{
    mark_summaries_rolled_up, mark_summaries_subject_memory_summarized,
    mark_summaries_subject_memory_summarized_for_scope,
};
#[allow(unused_imports)]
pub use writes::{
    create_rollup_summary, create_thread_summary, create_thread_summary_with_type,
    delete_thread_summary, upsert_thread_summary,
};
