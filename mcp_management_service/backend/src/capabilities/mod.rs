// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod materializer;
mod tools;

pub use materializer::{materialize_mcp_candidates, MaterializedAgentMcps};
pub use tools::{materialize_runtime_tools, runtime_route_revision, MaterializedRuntimeTools};
