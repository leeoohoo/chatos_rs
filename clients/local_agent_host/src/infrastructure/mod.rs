// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Infrastructure adapters for durable storage and retained control planes.

mod control_plane;
#[cfg(test)]
mod control_plane_tests;
mod hook_runner;
mod mcp;
mod memory;
mod model_credentials;

pub use chatos_client_storage::SqliteClientStorage;
pub use chatos_local_agent_ports::{
    ClientStorageError, LocalAgentStore, LocalCapabilityPolicySnapshot,
    LocalCapabilitySnapshotStore, LocalJsonSchemaOutputFormat, LocalModelConfigSnapshot,
    LocalModelConfigSnapshotStore,
};
#[cfg(windows)]
pub use chatos_windows_appcontainer::{SandboxedChild, WorkspaceAccess, WorkspaceLeaseManager};
pub use control_plane::{LocalControlPlaneSnapshot, LocalModelCredentialResolver};
#[cfg(windows)]
pub use hook_runner::WindowsAppContainerHookRunner;
pub use hook_runner::{
    LocalHookInvocation, LocalHookInvocationResult, LocalHookRunner, MacOsSeatbeltHookRunner,
};
pub use mcp::{
    LocalMcpServerConfig, LocalMcpStdioSession, LocalMcpToolDefinition, LocalMcpToolSet,
    LocalPluginSecretResolver,
};
pub use memory::{LocalMemoryRuntimeConfig, LocalMemoryRuntimeServices};
pub use model_credentials::ChildEnvironmentModelCredentialResolver;
