// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod archive;
mod auto_update;
mod bundled;
mod catalog;
mod credentials;
mod installer;
mod journal;
mod lifecycle;
mod oauth_sync;
mod recovery;
mod runtime;
mod state;
mod status_sync;
mod verifier;

pub use archive::{PluginArchiveLimits, VerifiedArchiveFiles};
pub(crate) use auto_update::{evaluate_auto_update, PluginAutoUpdateDecision};
pub use auto_update::{PluginAutoUpdateRecord, PluginAutoUpdateState};
pub use catalog::{
    local_plugin_store_snapshot, merge_auto_update_state, merge_network_plugin_sources,
    LocalPluginStoreItem, LocalPluginStoreSnapshot,
};
pub use credentials::{
    PluginCredentialMetadata, PluginCredentialScope, PluginCredentialVault, ResolvedPluginSecret,
};
pub(crate) use installer::PendingPluginInstall;
pub use installer::{
    ActivePluginInstallation, PluginInstallOutcome, PluginInstallRequest, PluginInstaller,
};
pub use journal::{
    LocalPluginStatusSnapshot, PluginRecoveryReport, PluginTransactionJournal,
    PluginTransactionOperation, PluginTransactionRecord,
};
pub(crate) use oauth_sync::oauth_status_message;
pub use runtime::{
    LocalPluginOAuthConnection, PluginCommandLoader, PluginCommandSnapshot,
    PluginDisabledHookReport, PluginHookDispatchResult, PluginHookExecutionRecord,
    PluginHookLoader, PluginHookSetSnapshot, PluginMcpAdapter, PluginMcpHealthSnapshot,
    PluginMcpSnapshot, PluginOAuthAppManifest, PluginOAuthAuthorizationStart, PluginOAuthBroker,
    PluginRuntimeHost, PluginRuntimeSessionStatus, PluginRuntimeSessionTelemetry,
    PluginRuntimeTelemetryEvent, PluginRuntimeTelemetryEventStatus, PluginRuntimeTelemetryPhase,
    PluginRuntimeTelemetrySnapshot, PluginSkillLoader, PluginSkillLoaderLimits,
    PluginSkillMetadata, PluginSkillResourceDescriptor, PluginSkillResourceKind,
    PluginSkillSnapshot,
};
pub use state::{InstalledPluginVersion, LocalInstalledPlugin, LocalPluginRegistry};
pub(crate) use status_sync::installation_status_message;
pub use verifier::{
    verify_plugin_install_source, PluginArtifactVerificationRequest, PluginRequirementInventory,
    VerifiedPluginArtifact,
};

#[cfg(test)]
mod tests;
