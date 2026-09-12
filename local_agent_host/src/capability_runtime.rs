// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use chatos_agent_profiles::{TaskRunnerCapabilitySnapshot, TaskRunnerExecutionTool};
use chatos_mcp_runtime::McpExecutor;
use parking_lot::RwLock;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::{
    FrozenMcpExecutor, FrozenMcpExecutorProvider, FrozenMcpExecutorRequest,
    LocalTaskCapabilityRequest, LocalTaskCapabilityResolution, LocalTaskCapabilityResolver,
};

pub struct RegisteredLocalCapabilityBundle {
    pub owner_user_id: String,
    pub project_id: String,
    pub resolution_revision: String,
    pub plugin_release_snapshot: Value,
    pub execution_tools: Vec<TaskRunnerExecutionTool>,
    pub executor: Arc<McpExecutor>,
}

/// A complete Registry image that has already passed every structural and
/// MCP schema check. Capability persistence accepts only this value, which
/// lets the subsequent in-memory swap be infallible and non-poisoning.
pub struct ValidatedLocalCapabilityReplacement {
    bundles: Vec<RegisteredLocalCapabilityBundle>,
}

impl ValidatedLocalCapabilityReplacement {
    pub fn len(&self) -> usize {
        self.bundles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bundles.is_empty()
    }
}

/// Single trusted registry shared by Task planning and tool execution. Native
/// plugin installation code registers only bundles whose releases, artifact
/// hashes, permissions, project scope, and runtime configuration have already
/// been verified locally.
#[derive(Default)]
pub struct RegisteredLocalCapabilityRuntime {
    bundles: RwLock<Vec<RegisteredLocalCapabilityBundle>>,
}

impl RegisteredLocalCapabilityRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, bundle: RegisteredLocalCapabilityBundle) -> Result<(), String> {
        validate_bundle(&bundle)?;
        let mut bundles = self.bundles.write();
        if let Some(existing) = bundles.iter_mut().find(|existing| {
            existing.owner_user_id == bundle.owner_user_id
                && existing.project_id == bundle.project_id
        }) {
            *existing = bundle;
        } else {
            bundles.push(bundle);
        }
        Ok(())
    }

    pub fn replace_all(&self, bundles: Vec<RegisteredLocalCapabilityBundle>) -> Result<(), String> {
        let replacement = Self::validate_replacement(bundles)?;
        self.replace_validated(replacement);
        Ok(())
    }

    pub fn validate_replacement(
        bundles: Vec<RegisteredLocalCapabilityBundle>,
    ) -> Result<ValidatedLocalCapabilityReplacement, String> {
        let mut identities = HashSet::new();
        for bundle in &bundles {
            validate_bundle(bundle)?;
            if !identities.insert((bundle.owner_user_id.as_str(), bundle.project_id.as_str())) {
                return Err(format!(
                    "local capability registry contains duplicate project {}",
                    bundle.project_id
                ));
            }
        }
        Ok(ValidatedLocalCapabilityReplacement { bundles })
    }

    pub fn replace_validated(&self, replacement: ValidatedLocalCapabilityReplacement) {
        *self.bundles.write() = replacement.bundles;
    }

    pub fn remove_project(&self, owner_user_id: &str, project_id: &str) -> Result<(), String> {
        let mut bundles = self.bundles.write();
        bundles.retain(|bundle| {
            bundle.owner_user_id != owner_user_id || bundle.project_id != project_id
        });
        Ok(())
    }

    pub fn is_empty(&self) -> Result<bool, String> {
        Ok(self.bundles.read().is_empty())
    }
}

#[async_trait]
impl LocalTaskCapabilityResolver for RegisteredLocalCapabilityRuntime {
    async fn resolve_capabilities(
        &self,
        request: &LocalTaskCapabilityRequest,
        cancellation: CancellationToken,
    ) -> Result<LocalTaskCapabilityResolution, String> {
        if cancellation.is_cancelled() {
            return Err("local capability resolution was cancelled".to_string());
        }
        if request.owner_user_id.trim().is_empty()
            || request.project_snapshot.project_id != request.project_snapshot.project_id.trim()
            || request.project_snapshot.project_id.is_empty()
        {
            return Err("local capability request identity is invalid".to_string());
        }
        let bundles = self.bundles.read();
        let bundle = bundles
            .iter()
            .find(|bundle| {
                bundle.owner_user_id == request.owner_user_id
                    && bundle.project_id == request.project_snapshot.project_id
            })
            .ok_or_else(|| {
                "no verified local capability bundle is registered for this project".to_string()
            })?;
        Ok(LocalTaskCapabilityResolution {
            resolution_revision: bundle.resolution_revision.clone(),
            plugin_release_snapshot: bundle.plugin_release_snapshot.clone(),
            execution_tools: bundle.execution_tools.clone(),
        })
    }
}

#[async_trait]
impl FrozenMcpExecutorProvider for RegisteredLocalCapabilityRuntime {
    async fn resolve(
        &self,
        request: &FrozenMcpExecutorRequest,
        cancellation: CancellationToken,
    ) -> Result<FrozenMcpExecutor, String> {
        if cancellation.is_cancelled() {
            return Err("local MCP executor resolution was cancelled".to_string());
        }
        let bundles = self.bundles.read();
        let bundle = bundles
            .iter()
            .find(|bundle| {
                bundle.owner_user_id == request.owner_user_id
                    && bundle.project_id == request.project_id
                    && bundle.plugin_release_snapshot == request.plugin_release_snapshot
            })
            .ok_or_else(|| {
                "no project-scoped local MCP executor matches the frozen plugin releases"
                    .to_string()
            })?;
        Ok(FrozenMcpExecutor {
            plugin_release_snapshot: bundle.plugin_release_snapshot.clone(),
            executor: bundle.executor.clone(),
        })
    }
}

fn validate_bundle(bundle: &RegisteredLocalCapabilityBundle) -> Result<(), String> {
    for (field, value) in [
        ("owner_user_id", bundle.owner_user_id.as_str()),
        ("project_id", bundle.project_id.as_str()),
        ("resolution_revision", bundle.resolution_revision.as_str()),
    ] {
        if value.trim().is_empty() || value.trim() != value {
            return Err(format!("local capability bundle {field} is invalid"));
        }
    }
    let snapshot = TaskRunnerCapabilitySnapshot {
        snapshot_ref: "registration-validation".to_string(),
        plugin_release_snapshot: bundle.plugin_release_snapshot.clone(),
        execution_tools: bundle.execution_tools.clone(),
    };
    snapshot.validate()?;
    let available = bundle
        .executor
        .available_tools()
        .into_iter()
        .filter_map(|schema| {
            let name = schema
                .get("name")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)?;
            Some((name, schema))
        })
        .collect::<Vec<_>>();
    let frozen_names = bundle
        .execution_tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect::<HashSet<_>>();
    for tool in &bundle.execution_tools {
        let schema = available
            .iter()
            .find_map(|(name, schema)| (name == &tool.name).then_some(schema))
            .ok_or_else(|| {
                format!(
                    "registered MCP executor does not expose frozen tool {}",
                    tool.name
                )
            })?;
        if *schema != tool.schema {
            return Err(format!(
                "registered MCP executor schema differs for frozen tool {}",
                tool.name
            ));
        }
    }
    if available
        .iter()
        .any(|(name, _)| !frozen_names.contains(name.as_str()))
    {
        return Err(
            "registered MCP executor exposes tools outside the frozen capability set".to_string(),
        );
    }
    Ok(())
}
