mod active_summary;
mod client;
mod mappers;
mod mapping;
mod memories;
mod review_repair;
mod sessions;
mod snapshots;
mod types;

pub use self::active_summary::{
    try_start_chatos_active_summary, wait_for_existing_chatos_active_summary_completion,
};
pub(crate) use self::mappers::engine_record_to_message;
pub use self::mapping::CHATOS_COMPAT_SOURCE_ID;
pub use self::memories::{
    list_contact_agent_recalls, list_contact_project_memories,
    list_contact_project_memories_by_contact,
};
pub use self::review_repair::{
    get_chatos_review_repair_job_run, get_chatos_review_repair_status, run_chatos_review_repair,
};
pub use self::sessions::{
    archive_chatos_session, compose_chatos_context, create_chatos_session,
    delete_all_chatos_messages, delete_chatos_message_by_id,
    delete_chatos_message_by_id_for_tenant, delete_chatos_summary, get_chatos_message_by_id,
    get_chatos_message_by_id_for_tenant, get_chatos_message_by_id_in_session, get_chatos_session,
    get_chatos_turn_process_records, list_chatos_compact_turns, list_chatos_messages,
    list_chatos_messages_including_hidden, list_chatos_sessions, list_chatos_sessions_by_agent,
    list_chatos_summaries, sync_chatos_session, update_chatos_session, upsert_chatos_message,
};
pub use self::snapshots::{
    get_chatos_turn_runtime_snapshot_by_turn, get_latest_chatos_turn_runtime_snapshot,
    sync_chatos_turn_runtime_snapshot,
};
pub use self::types::{
    ChatosReviewRepairRequest, ComposedChatHistoryContext, ReviewRepairStatusResult,
    ReviewRepairSummaryRunResult,
};

use self::memories::register_subject_memory_scopes;

const CHATOS_TURN_RUNTIME_SNAPSHOT_TYPE: &str = "turn_runtime";

fn normalize_non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
