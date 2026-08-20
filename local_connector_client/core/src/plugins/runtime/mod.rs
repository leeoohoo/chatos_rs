// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod agent_loader;
mod artifact_store;
mod command_loader;
mod hook_loader;
mod host;
mod mcp_runtime;
mod oauth_broker;
mod protocol;
mod skill_document;
mod skill_loader;
mod telemetry;
mod ui_loader;

pub use agent_loader::{PluginAgentLoader, PluginAgentSnapshot};
pub use command_loader::{PluginCommandLoader, PluginCommandSnapshot};
pub use hook_loader::{
    PluginHookDispatchResult, PluginHookExecutionRecord, PluginHookLoader, PluginHookSetSnapshot,
};
pub use host::{PluginDisabledHookReport, PluginRuntimeHost};
pub use mcp_runtime::{PluginMcpAdapter, PluginMcpHealthSnapshot, PluginMcpSnapshot};
pub use oauth_broker::{
    LocalPluginOAuthConnection, PluginOAuthAppManifest, PluginOAuthAuthorizationStart,
    PluginOAuthBroker,
};
pub use skill_loader::{
    PluginSkillLoader, PluginSkillLoaderLimits, PluginSkillMetadata, PluginSkillResourceDescriptor,
    PluginSkillResourceKind, PluginSkillSnapshot,
};
pub use telemetry::{
    PluginRuntimeSessionStatus, PluginRuntimeSessionTelemetry, PluginRuntimeTelemetryEvent,
    PluginRuntimeTelemetryEventStatus, PluginRuntimeTelemetryPhase, PluginRuntimeTelemetrySnapshot,
};
pub use ui_loader::PluginUiLoader;

#[cfg(test)]
mod tests;
