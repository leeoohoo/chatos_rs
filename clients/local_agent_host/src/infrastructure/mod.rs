// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Infrastructure adapters for durable storage and retained control planes.

mod control_plane;
#[cfg(test)]
mod control_plane_tests;
mod mcp;

pub use chatos_client_storage::SqliteClientStorage;
pub use chatos_local_agent_ports::{
    ClientStorageError, LocalAgentStore, LocalCapabilityPolicySnapshot,
    LocalCapabilitySnapshotStore, LocalJsonSchemaOutputFormat, LocalModelConfigSnapshot,
    LocalModelConfigSnapshotStore,
};
pub use control_plane::{LocalControlPlaneSnapshot, LocalModelCredentialResolver};
pub use mcp::{
    LocalMcpServerConfig, LocalMcpStdioSession, LocalMcpToolDefinition, LocalMcpToolSet,
    LocalPluginSecretResolver,
};
