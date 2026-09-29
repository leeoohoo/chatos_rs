// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::validate_identifier;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetMemorySyncStatusCommand {
    pub tenant_id: String,
    pub source_id: String,
}

impl GetMemorySyncStatusCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("tenant_id", &self.tenant_id)?;
        validate_identifier("source_id", &self.source_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalMemorySyncStatus {
    pub tenant_id: String,
    pub source_id: String,
    pub pending_count: u64,
    pub syncing_count: u64,
    pub retry_scheduled_count: u64,
    pub synced_count: u64,
    pub oldest_unsynced_at_unix_ms: Option<i64>,
    pub next_retry_at_unix_ms: Option<i64>,
    pub last_synced_at_unix_ms: Option<i64>,
    pub last_error: Option<String>,
    pub last_error_at_unix_ms: Option<i64>,
}

impl LocalMemorySyncStatus {
    pub const fn unsynced_count(&self) -> u64 {
        self.pending_count
            .saturating_add(self.syncing_count)
            .saturating_add(self.retry_scheduled_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_scope_and_counts_unsynced_records() {
        let command = GetMemorySyncStatusCommand {
            tenant_id: "user-1".to_string(),
            source_id: "local_agent".to_string(),
        };
        assert!(command.validate().is_ok());
        assert!(GetMemorySyncStatusCommand {
            tenant_id: " ".to_string(),
            source_id: "local_agent".to_string(),
        }
        .validate()
        .is_err());

        let status = LocalMemorySyncStatus {
            tenant_id: command.tenant_id,
            source_id: command.source_id,
            pending_count: 2,
            syncing_count: 1,
            retry_scheduled_count: 3,
            synced_count: 5,
            oldest_unsynced_at_unix_ms: Some(1),
            next_retry_at_unix_ms: Some(2),
            last_synced_at_unix_ms: Some(3),
            last_error: None,
            last_error_at_unix_ms: None,
        };
        assert_eq!(status.unsynced_count(), 6);
    }
}
