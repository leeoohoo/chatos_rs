use serde::Serialize;
use tracing::{info, warn};

use crate::config::AppConfig;
use crate::models::{now_rfc3339, TaskRecord, TaskRunRecord, TaskRunStatus};
use crate::store::AppStore;

use super::prerequisite_context::extract_report_content;
use super::{RunService, TaskScheduleModeExt, TaskStatusExt};

mod delivery;
mod dispatch;
mod payload;

#[derive(Debug, Clone, Serialize)]
struct ChatosTaskCallbackPayload {
    event: String,
    task_id: String,
    run_id: Option<String>,
    status: String,
    task_title: String,
    result_summary: Option<String>,
    error_message: Option<String>,
    report_content: Option<String>,
    process_log: Option<String>,
    source_session_id: Option<String>,
    source_turn_id: Option<String>,
    source_user_message_id: Option<String>,
    parent_task_id: Option<String>,
    source_run_id: Option<String>,
    prerequisite_task_ids: Vec<String>,
    schedule_mode: String,
    callback_at: String,
}
