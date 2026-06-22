use tracing::info;

use crate::db::Db;
use crate::models::EngineThread;
use crate::repositories::{records, threads};

use super::super::super::selectors::select_records_for_repair;
use super::super::super::settings::load_summary_job_settings;
use super::super::super::{
    RepairRecordSelection, SummaryJobSettings, DEFAULT_PENDING_RECORD_SCAN_LIMIT,
};
use super::THREAD_REPAIR_JOB_TYPE;

pub(crate) struct RepairSummaryPreparation {
    pub(crate) thread: EngineThread,
    pub(crate) settings: SummaryJobSettings,
    pub(crate) pending_before_count: i64,
    pub(crate) selection: RepairRecordSelection,
}

pub(crate) async fn load_repair_summary_preparation(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
) -> Result<RepairSummaryPreparation, String> {
    let thread = threads::get_thread_by_id(db, tenant_id, source_id, thread_id)
        .await?
        .ok_or_else(|| "thread not found".to_string())?;
    let settings = load_summary_job_settings(db, THREAD_REPAIR_JOB_TYPE).await?;
    let pending_before_count =
        count_pending_thread_records(db, tenant_id, source_id, thread_id, None).await?;
    let pending_records = records::list_pending_records(
        db,
        tenant_id,
        source_id,
        thread_id,
        DEFAULT_PENDING_RECORD_SCAN_LIMIT,
    )
    .await?;
    info!(
        "[MEMORY-ENGINE-REPAIR] start tenant_id={} source_id={} thread_id={} pending_before_count={} scanned_pending_records={} token_limit={} target_summary_tokens=disabled",
        tenant_id,
        source_id,
        thread_id,
        pending_before_count,
        pending_records.len(),
        settings.token_limit
    );
    let selection = select_records_for_repair(pending_records);
    log_repair_selection(
        tenant_id,
        source_id,
        thread_id,
        pending_before_count,
        &selection,
    );

    Ok(RepairSummaryPreparation {
        thread,
        settings,
        pending_before_count,
        selection,
    })
}

pub(super) async fn count_pending_thread_records(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    record_type: Option<&str>,
) -> Result<i64, String> {
    records::count_records(
        db,
        thread_id,
        Some(tenant_id),
        Some(source_id),
        None,
        record_type,
        Some("pending"),
    )
    .await
}

fn log_repair_selection(
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    pending_before_count: i64,
    selection: &RepairRecordSelection,
) {
    if selection.selected.is_empty() {
        info!(
            "[MEMORY-ENGINE-REPAIR] noop tenant_id={} source_id={} thread_id={} pending_before_count={}",
            tenant_id, source_id, thread_id, pending_before_count
        );
        return;
    }

    let selected_tool_records = selection
        .selected
        .iter()
        .filter(|item| item.role == "tool")
        .count();
    let empty_content_records = selection
        .selected
        .iter()
        .filter(|item| item.content.trim().is_empty())
        .count();
    let selected_content_chars = selection
        .selected
        .iter()
        .map(|item| item.content.chars().count())
        .sum::<usize>();
    let max_record_chars = selection
        .selected
        .iter()
        .map(|item| item.content.chars().count())
        .max()
        .unwrap_or(0);
    info!(
        "[MEMORY-ENGINE-REPAIR] selected tenant_id={} source_id={} thread_id={} selected_count={} selected_token_count={} selected_tool_records={} empty_content_records={} total_content_chars={} max_record_chars={}",
        tenant_id,
        source_id,
        thread_id,
        selection.selected.len(),
        selection.selected_token_count,
        selected_tool_records,
        empty_content_records,
        selected_content_chars,
        max_record_chars
    );
}
