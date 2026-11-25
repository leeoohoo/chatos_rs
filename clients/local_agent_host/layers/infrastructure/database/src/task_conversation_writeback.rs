// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    conversation_store::next_message_ordinal,
    memory_outbox_store,
    task_graph_store::fetch_graph,
    task_writeback_source::{callback_source, CallbackSource},
    task_writeback_summary::{
        graph_delivery_text, graph_summary_text, graph_task_summaries, task_display_status,
    },
    ClientStorageError, SqliteClientStorage, SqliteResultExt,
};
use chatos_local_agent_protocol::{
    resolve_local_task_graph_notification_policy, LocalAgentRunRecord, LocalAgentRunStatus,
    LocalTaskGraph, LocalTaskGraphNotificationPolicy, LocalTaskGraphStatus, LocalTaskStatus,
};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::{Row, SqliteConnection};
use uuid::Uuid;

#[cfg(test)]
use super::task_callback_display::sanitize_visible_detail;
#[cfg(test)]
use super::task_callback_display::{callback_content, full_delivery_detail};
use super::task_callback_display::{
    callback_content_with_status, contains_cjk, progress_callback_content_with_status,
    truncate_chars, user_visible_callback_detail,
};
#[cfg(test)]
use super::task_writeback_source::callback_prefers_english;

include!("task_conversation_writeback/entrypoints.rs");
include!("task_conversation_writeback/callbacks.rs");
include!("task_conversation_writeback/memory_projection.rs");

#[cfg(test)]
#[path = "task_conversation_writeback_tests.rs"]
mod tests;
