// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub(crate) mod lease;
pub(crate) mod managed_requirements;
pub(crate) mod managed_requirements_cache;
pub(crate) mod pairing;
pub(crate) mod permission_layers;
pub(crate) mod project_permissions;
pub(crate) mod proxy;
pub(crate) mod relay;
pub(crate) mod types;
#[cfg(windows)]
pub(crate) mod windows_security;
pub(crate) mod workspace;

pub(crate) fn local_connector_execution_capability(
) -> chatos_sandbox_contract::SandboxBackendCapability {
    use chatos_sandbox_contract::{
        SandboxBackendCapability, SandboxBackendKind, SandboxBackendReadinessStatus,
    };

    SandboxBackendCapability {
        backend: SandboxBackendKind::LocalProcess,
        status: SandboxBackendReadinessStatus::Ready,
        selectable: true,
        filesystem_isolation: false,
        network_isolation: false,
        process_tree_control: true,
        message: "Local Connector Client executes tools directly in the selected local project"
            .to_string(),
    }
}
