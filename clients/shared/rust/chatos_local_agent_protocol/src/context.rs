// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    require_digest, require_identifier, require_nonempty_bounded_text, ProtocolError,
    MAX_ENCRYPTED_CONTEXT_BYTES,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProviderContextItem {
    pub item_id: String,
    pub run_id: String,
    pub generation: u64,
    pub sequence: u64,
    pub provider: String,
    pub item_type: String,
    pub encrypted_payload: String,
    pub payload_digest: String,
    pub created_at: DateTime<Utc>,
}

impl ProviderContextItem {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        for (field, value) in [
            ("item_id", self.item_id.as_str()),
            ("run_id", self.run_id.as_str()),
            ("provider", self.provider.as_str()),
            ("item_type", self.item_type.as_str()),
        ] {
            require_identifier(field, value)?;
        }
        require_nonempty_bounded_text(
            "encrypted_payload",
            &self.encrypted_payload,
            MAX_ENCRYPTED_CONTEXT_BYTES,
        )?;
        require_digest("payload_digest", &self.payload_digest)?;
        if self.generation == 0 || self.sequence == 0 {
            return Err(ProtocolError::InvalidState {
                reason: "provider context generation and sequence must be positive",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SyncDestination {
    MemoryEngine,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SyncOutboxStatus {
    Pending,
    InFlight,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SyncOutboxItem {
    pub outbox_id: String,
    pub destination: SyncDestination,
    pub record_id: String,
    pub payload_digest: String,
    pub status: SyncOutboxStatus,
    pub attempt_count: u32,
    pub available_at: DateTime<Utc>,
    pub last_error: Option<String>,
}

impl SyncOutboxItem {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("outbox_id", &self.outbox_id)?;
        require_identifier("record_id", &self.record_id)?;
        require_digest("payload_digest", &self.payload_digest)
    }
}
