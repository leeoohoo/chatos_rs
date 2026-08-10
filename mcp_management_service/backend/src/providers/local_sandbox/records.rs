// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct SandboxPairingRecord {
    pub(super) id: String,
    pub(super) device_id: String,
    pub(super) workspace_id: String,
    pub(super) enabled: bool,
    #[serde(default)]
    pub(super) sandbox_readiness: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct LocalSandboxLeaseBinding {
    pub(super) id: String,
    pub(super) sandbox_id: String,
    pub(super) tenant_id: String,
    pub(super) project_id: String,
    pub(super) run_id: String,
    pub(super) status: String,
}
