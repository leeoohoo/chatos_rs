use super::*;

#[path = "mcp_management/async_dispatch.rs"]
mod async_dispatch;
#[path = "mcp_management/downstream.rs"]
mod downstream;
#[path = "mcp_management/security_runtime.rs"]
mod security_runtime;

pub(super) fn definitions(now: &str) -> Vec<ConfigDefinitionRecord> {
    let mut definitions = Vec::new();
    definitions.extend(async_dispatch::definitions(now));
    definitions.extend(security_runtime::definitions(now));
    definitions.extend(downstream::definitions(now));
    definitions
}
