// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
