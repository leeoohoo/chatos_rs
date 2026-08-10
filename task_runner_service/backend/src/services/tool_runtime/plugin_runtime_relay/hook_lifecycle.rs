// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use chatos_mcp_runtime::{ToolLifecycleEvent, ToolLifecycleHook, ToolLifecycleOutcome};
use chatos_plugin_management_sdk::{PluginComponentKind, PluginHookEvent, PluginHookEventContext};
use serde_json::{json, Value};

use crate::models::TaskRunEventRecord;

use super::relay_client::sanitize_runtime_error;
use super::{plugin_server_name_from_identity, PreparedPluginRuntime, PreparedPluginSession};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::services) struct PluginHookLifecycleOutcome {
    pub blocking_failure: bool,
    pub errors: Vec<String>,
}

impl PreparedPluginRuntime {
    pub(in crate::services) async fn dispatch_hook_event(
        &self,
        event: PluginHookEvent,
        context: &PluginHookEventContext,
    ) -> PluginHookLifecycleOutcome {
        let mut outcome = PluginHookLifecycleOutcome::default();
        for session in self
            .sessions
            .iter()
            .filter(|session| session.component_kind == PluginComponentKind::HookSet)
        {
            match session.dispatch_hook_event(event, context).await {
                Ok(blocking_failure) => outcome.blocking_failure |= blocking_failure,
                Err(error) => {
                    outcome.blocking_failure = true;
                    outcome.errors.push(error);
                }
            }
        }
        outcome
    }

    pub(in crate::services) fn tool_lifecycle_hook(
        &self,
        agent_key: &str,
    ) -> Option<Arc<dyn ToolLifecycleHook>> {
        self.sessions
            .iter()
            .any(|session| session.component_kind == PluginComponentKind::HookSet)
            .then(|| {
                Arc::new(PluginToolLifecycleHook {
                    sessions: self.sessions.clone(),
                    agent_key: agent_key.to_string(),
                    component_by_server: self
                        .sessions
                        .iter()
                        .filter(|session| session.component_kind != PluginComponentKind::HookSet)
                        .map(|session| {
                            (
                                plugin_server_name_from_identity(
                                    session.plugin_id.as_str(),
                                    session.component_key.as_str(),
                                ),
                                session.component_key.clone(),
                            )
                        })
                        .collect(),
                }) as Arc<dyn ToolLifecycleHook>
            })
    }
}

pub(in crate::services) async fn dispatch_prepared_plugin_hooks(
    sessions: &[PreparedPluginSession],
    event: PluginHookEvent,
    context: &PluginHookEventContext,
) -> PluginHookLifecycleOutcome {
    let runtime = PreparedPluginRuntime {
        sessions: sessions.to_vec(),
        ..PreparedPluginRuntime::default()
    };
    runtime.dispatch_hook_event(event, context).await
}

#[derive(Clone)]
pub(super) struct PluginToolLifecycleHook {
    pub(super) sessions: Vec<PreparedPluginSession>,
    pub(super) agent_key: String,
    pub(super) component_by_server: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PluginToolLifecycleStage {
    Pre,
    Post,
}

impl std::fmt::Debug for PluginToolLifecycleHook {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PluginToolLifecycleHook")
            .field("session_count", &self.sessions.len())
            .field("agent_key", &self.agent_key)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl ToolLifecycleHook for PluginToolLifecycleHook {
    async fn before_tool_use(&self, event: &ToolLifecycleEvent) -> Result<(), String> {
        let (hook_event, context) = self.map_event(event, PluginToolLifecycleStage::Pre);
        self.dispatch(hook_event, context).await
    }

    async fn after_tool_use(&self, event: &ToolLifecycleEvent) -> Result<(), String> {
        let (hook_event, context) = self.map_event(event, PluginToolLifecycleStage::Post);
        self.dispatch(hook_event, context).await
    }
}

impl PluginToolLifecycleHook {
    pub(super) fn map_event(
        &self,
        event: &ToolLifecycleEvent,
        stage: PluginToolLifecycleStage,
    ) -> (PluginHookEvent, PluginHookEventContext) {
        let (hook_event, outcome, summary_sha256) = match stage {
            PluginToolLifecycleStage::Pre => (
                PluginHookEvent::PreToolUse,
                None,
                Some(event.arguments_sha256.clone()),
            ),
            PluginToolLifecycleStage::Post => (
                PluginHookEvent::PostToolUse,
                event.outcome.map(|outcome| match outcome {
                    ToolLifecycleOutcome::Succeeded => {
                        chatos_plugin_management_sdk::PluginHookOutcome::Succeeded
                    }
                    ToolLifecycleOutcome::Failed => {
                        chatos_plugin_management_sdk::PluginHookOutcome::Failed
                    }
                }),
                event.result_sha256.clone(),
            ),
        };
        (
            hook_event,
            PluginHookEventContext {
                agent_key: Some(self.agent_key.clone()),
                tool_name: Some(event.tool_name.clone()),
                tool_kind: Some(event.server_type.clone()),
                component_key: self.component_by_server.get(&event.server_name).cloned(),
                outcome,
                summary_sha256,
            },
        )
    }

    async fn dispatch(
        &self,
        event: PluginHookEvent,
        context: PluginHookEventContext,
    ) -> Result<(), String> {
        let outcome =
            dispatch_prepared_plugin_hooks(self.sessions.as_slice(), event, &context).await;
        if outcome.blocking_failure {
            let message = sanitize_runtime_error(hook_lifecycle_error(event, &outcome).as_str());
            if let Some(session) = self.sessions.first() {
                session
                    .relay
                    .store
                    .append_run_event_sync(TaskRunEventRecord::new(
                        session.relay.run_id.clone(),
                        "plugin_hook_blocked",
                        Some(message.clone()),
                        Some(json!({
                            "event": event,
                            "blocking_failure": true,
                            "tool_name": context.tool_name,
                            "tool_kind": context.tool_kind,
                            "component_key": context.component_key,
                            "summary_sha256": context.summary_sha256,
                        })),
                    ));
            }
            Err(message)
        } else {
            Ok(())
        }
    }
}

impl PreparedPluginSession {
    async fn dispatch_hook_event(
        &self,
        event: PluginHookEvent,
        context: &PluginHookEventContext,
    ) -> Result<bool, String> {
        if !self.operations.contains("dispatch_hook_event") {
            return Err(format!(
                "Plugin Hook session did not publish dispatch_hook_event: {}:{}",
                self.plugin_id, self.component_key
            ));
        }
        let mut body = self.identity_body();
        body.insert("operation".to_string(), json!("dispatch_hook_event"));
        body.insert("event".to_string(), json!(event));
        body.insert("context".to_string(), json!(context));
        let response = self.relay.request("execute", Value::Object(body)).await?;
        let result = response
            .get("result")
            .and_then(Value::as_object)
            .ok_or_else(|| "Plugin Hook execute response is missing result".to_string())?;
        if result.get("event") != Some(&json!(event))
            || result.get("snapshot_sha256").and_then(Value::as_str)
                != self.hook_snapshot_sha256.as_deref()
        {
            return Err(
                "Plugin Hook execute response does not match the prepared Hook snapshot"
                    .to_string(),
            );
        }
        result
            .get("blocking_failure")
            .and_then(Value::as_bool)
            .ok_or_else(|| "Plugin Hook execute response is missing blocking_failure".to_string())
    }
}

pub(super) fn hook_lifecycle_error(
    event: PluginHookEvent,
    outcome: &PluginHookLifecycleOutcome,
) -> String {
    if outcome.errors.is_empty() {
        format!("Plugin Hook {} failed with fail_run policy", event.as_str())
    } else {
        format!(
            "Plugin Hook {} dispatch failed: {}",
            event.as_str(),
            outcome.errors.join("; ")
        )
    }
}
