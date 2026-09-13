// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    require_identifier, require_json_with_limit, ProtocolError, MAX_PLUGIN_CAPABILITY_JSON_BYTES,
};

/// Carries the complete, immutable installation evidence produced by the
/// native installer. The protocol intentionally keeps this as bounded JSON so
/// it does not duplicate Plugin SDK release types; the Host is the only
/// authority that may deserialize and validate the final capability schema.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct InstallProjectPluginCapabilityCommand {
    pub project_id: String,
    pub plugin_id: String,
    pub release_id: String,
    pub capability_record: Value,
}

impl InstallProjectPluginCapabilityCommand {
    pub(crate) fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("project_id", &self.project_id)?;
        require_identifier("plugin_id", &self.plugin_id)?;
        require_identifier("release_id", &self.release_id)?;
        if !self.capability_record.is_object() {
            return Err(ProtocolError::InvalidJson {
                field: "capability_record",
            });
        }
        require_json_with_limit(
            "capability_record",
            &self.capability_record,
            MAX_PLUGIN_CAPABILITY_JSON_BYTES,
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RemoveProjectPluginCapabilityCommand {
    pub project_id: String,
    pub plugin_id: String,
    pub release_id: String,
}

impl RemoveProjectPluginCapabilityCommand {
    pub(crate) fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("project_id", &self.project_id)?;
        require_identifier("plugin_id", &self.plugin_id)?;
        require_identifier("release_id", &self.release_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalInstalledPluginDraft {
    pub plugin_id: String,
    pub release: String,
    pub enabled: bool,
    pub installation: Value,
}

impl LocalInstalledPluginDraft {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("plugin_id", &self.plugin_id)?;
        require_identifier("release", &self.release)?;
        if !self.installation.is_object() {
            return Err(ProtocolError::InvalidJson {
                field: "installation",
            });
        }
        require_json_with_limit(
            "installation",
            &self.installation,
            MAX_PLUGIN_CAPABILITY_JSON_BYTES,
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalInstalledPluginSnapshot {
    pub record_id: String,
    pub owner_user_id: String,
    pub draft: LocalInstalledPluginDraft,
    pub revision: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl LocalInstalledPluginSnapshot {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("record_id", &self.record_id)?;
        require_identifier("owner_user_id", &self.owner_user_id)?;
        self.draft.validate()?;
        if self.revision == 0 || self.updated_at < self.created_at {
            return Err(ProtocolError::InvalidState {
                reason: "installed Plugin revision and timestamps are invalid",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PutInstalledPluginCommand {
    pub expected_revision: Option<u64>,
    pub draft: LocalInstalledPluginDraft,
}

impl PutInstalledPluginCommand {
    pub(crate) fn validate(&self) -> Result<(), ProtocolError> {
        if self.expected_revision == Some(0) {
            return Err(ProtocolError::InvalidState {
                reason: "installed Plugin expected_revision must be positive",
            });
        }
        self.draft.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeleteInstalledPluginCommand {
    pub plugin_id: String,
    pub expected_revision: u64,
}

impl DeleteInstalledPluginCommand {
    pub(crate) fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("plugin_id", &self.plugin_id)?;
        if self.expected_revision == 0 {
            return Err(ProtocolError::InvalidState {
                reason: "installed Plugin expected_revision must be positive",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ListInstalledPluginsCommand {
    pub cursor: Option<String>,
    pub limit: u32,
}

impl ListInstalledPluginsCommand {
    pub(crate) fn validate(&self) -> Result<(), ProtocolError> {
        if let Some(cursor) = &self.cursor {
            require_identifier("cursor", cursor)?;
        }
        if self.limit == 0 || self.limit > 500 {
            return Err(ProtocolError::InvalidState {
                reason: "installed Plugin page limit must be between 1 and 500",
            });
        }
        Ok(())
    }
}
