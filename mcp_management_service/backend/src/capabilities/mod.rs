// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod materializer;
mod product_skill_control_plane;
mod tools;

pub use materializer::{materialize_mcp_candidates, MaterializedAgentMcps};
pub use product_skill_control_plane::append_product_skill_control_plane;
#[cfg(test)]
pub(crate) use tools::product_skill_binding_for_route;
pub(crate) use tools::validate_product_skill_binding;
pub use tools::{
    materialize_runtime_tools, materialize_runtime_tools_with_plugin_components,
    materialize_runtime_tools_with_plugins, route_allows_system_tool, runtime_route_revision,
    MaterializedRuntimeTools,
};
