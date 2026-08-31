// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// Keep plugin runtime's MCP-specific preparation and execution helpers behind
// one module boundary so generic plugin loading code does not depend on
// scattered transport and credential internals.
#[path = "mcp/mod.rs"]
mod adapter;
#[path = "mcp_credentials.rs"]
mod credentials;

pub(in crate::plugins::runtime) use adapter::{
    load_verified_manifest, plugin_mcp_workspace_root_sha256, PluginMcpInvocationCancelOutcome,
    PreparedPluginMcp,
};
pub use adapter::{PluginMcpAdapter, PluginMcpHealthSnapshot, PluginMcpSnapshot};
#[cfg(test)]
pub(in crate::plugins::runtime) use adapter::{PluginMcpInvoker, PreparedPluginMcpTransport};
