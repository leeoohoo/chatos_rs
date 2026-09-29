// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Infrastructure adapters for durable storage and retained control planes.

mod control_plane;
mod mcp;

pub use chatos_client_storage::SqliteClientStorage;
pub use chatos_local_agent_ports::{ClientStorageError, LocalAgentStore};
pub use control_plane::LocalControlPlaneSnapshot;
pub use mcp::{
    LocalMcpServerConfig, LocalMcpStdioSession, LocalMcpToolDefinition, LocalMcpToolSet,
};
