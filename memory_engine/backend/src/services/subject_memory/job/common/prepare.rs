use tracing::info;

use crate::db::Db;
use crate::models::{RunSubjectMemoryJobRequest, RunSubjectMemoryJobResponse};
use crate::repositories::summaries;

use super::super::super::selectors::{select_rollup_batch, select_summary_batch};
use super::super::super::settings::build_settings_with_policy;
use super::super::super::{
    PendingSourceSummary, RollupSelection, SubjectMemoryJobSettings, DEFAULT_MAX_SOURCE_SUMMARIES,
};

pub(crate) struct SubjectMemoryPreparation {
    pub(crate) settings: SubjectMemoryJobSettings,
    pub(crate) summary_selection: Option<Vec<PendingSourceSummary>>,
    pub(crate) rollup_selection: Option<RollupSelection>,
    pub(crate) input_count: usize,
}

pub(crate) async fn prepare_subject_memory_job(
    db: &Db,
    req: &RunSubjectMemoryJobRequest,
) -> Result<SubjectMemoryPreparation, String> {
    let settings = build_settings_with_policy(db, req).await?;
    let pending_summaries = summaries::list_summaries_by_thread_label(
        db,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        req.source_thread_label.as_str(),
        Some(settings.source_summary_type.as_str()),
        Some("done"),
        None,
        Some(0),
        DEFAULT_MAX_SOURCE_SUMMARIES,
        0,
    )
    .await?
    .into_iter()
    .map(|item| PendingSourceSummary {
        id: item.id,
        tenant_id: item.tenant_id,
        source_id: item.source_id,
        thread_id: item.thread_id,
        summary_type: item.summary_type,
        level: item.level,
        summary_text: item.summary_text,
        created_at: item.created_at,
        metadata: item.metadata,
    })
    .collect::<Vec<_>>();
    let input_count = pending_summaries.len();
    let summary_selection = select_summary_batch(
        pending_summaries.as_slice(),
        settings.token_limit,
        settings.count_limit,
    );
    let rollup_selection = select_rollup_batch(
        db,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        req.subject_id.as_str(),
        settings.relation_subject_id.as_str(),
        req.memory_type.as_str(),
        settings.token_limit,
        settings.count_limit,
        settings.keep_level0_count,
        settings.max_level,
    )
    .await?;

    Ok(SubjectMemoryPreparation {
        settings,
        summary_selection,
        rollup_selection,
        input_count,
    })
}

pub(crate) fn empty_subject_memory_response(subject_id: String) -> RunSubjectMemoryJobResponse {
    RunSubjectMemoryJobResponse {
        subject_id,
        generated_level0: 0,
        generated_rollups: 0,
        generated_memories: 0,
        marked_source_summaries: 0,
        marked_source_memories: 0,
    }
}

pub(crate) fn log_noop(req: &RunSubjectMemoryJobRequest) {
    info!(
        "[MEMORY-ENGINE-SUBJECT] noop subject_id={} memory_type={} source_id={} thread_label={}",
        req.subject_id, req.memory_type, req.source_id, req.source_thread_label
    );
}
