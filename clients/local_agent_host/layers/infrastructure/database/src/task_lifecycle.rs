// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{ClientStorageError, SqliteClientStorage, SqliteResultExt};
use chatos_local_agent_protocol::{
    local_task_runtime_write_scope, local_task_write_scope_overlap,
    task_acceptance_report_is_blocked, LocalAgentRunRecord, LocalAgentRunStatus,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection};

include!("task_lifecycle/sql.rs");
include!("task_lifecycle/start.rs");
include!("task_lifecycle/prerequisites.rs");
include!("task_lifecycle/reconciliation.rs");

#[cfg(test)]
#[path = "task_lifecycle_tests.rs"]
mod tests;
