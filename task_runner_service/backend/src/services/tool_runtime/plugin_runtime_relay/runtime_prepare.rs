// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::{PluginComponentKind, PluginHookEvent, PluginHookEventContext};

use super::{hook_lifecycle_error, prepare_component, PluginRelayClient, PreparedPluginRuntime};
use crate::models::{TaskRecord, TaskRunRecord};
use crate::services::plugin_cloud_runtime::{component_uses_local, run_requires_local_relay};
use crate::services::RunService;

impl RunService {
    pub(in crate::services) async fn prepare_plugin_runtime(
        &self,
        task: &TaskRecord,
        run: &TaskRunRecord,
        effective_workspace_dir: &str,
    ) -> Result<PreparedPluginRuntime, String> {
        if run.plugin_snapshots.is_empty() {
            return Ok(PreparedPluginRuntime::default());
        }
        let mut prepared = self.prepare_cloud_plugin_runtime(run).await?;
        let has_local_components = run_requires_local_relay(run);
        if !has_local_components {
            prepared.sort_prompt_items();
            return Ok(prepared);
        }
        let relay = PluginRelayClient::from_task(self, task, run)?;
        ensure_local_plugin_snapshots_match_relay(&prepared, &relay, run).await?;
        prepare_local_hook_components(&mut prepared, &relay, run, effective_workspace_dir).await?;
        let agent_key = crate::models::task_runner_agent_key_for(
            task.task_profile.as_str(),
            task.mcp_config.requires_execution,
        );
        dispatch_required_hook_stage(
            &prepared,
            PluginHookEvent::BeforePluginPrepare,
            agent_key.as_str(),
        )
        .await?;
        prepare_local_non_hook_components(&mut prepared, &relay, run, effective_workspace_dir)
            .await?;
        dispatch_required_hook_stage(&prepared, PluginHookEvent::SessionStart, agent_key.as_str())
            .await?;
        for session in &prepared.sessions {
            session.record_ui_ready();
        }
        prepared.sort_prompt_items();
        Ok(prepared)
    }
}

async fn ensure_local_plugin_snapshots_match_relay(
    prepared: &PreparedPluginRuntime,
    relay: &PluginRelayClient,
    run: &TaskRunRecord,
) -> Result<(), String> {
    for plugin in &run.plugin_snapshots {
        if plugin
            .component_snapshots
            .iter()
            .any(|component| component_uses_local(plugin, component))
            && (plugin.device_id.as_deref() != Some(relay.device_id.as_str())
                || plugin.workspace_id.as_deref() != relay.workspace_id.as_deref())
        {
            prepared.cancel_all().await;
            return Err(format!(
                "Run Plugin snapshot does not match selected device/workspace: {}",
                plugin.plugin_id
            ));
        }
    }
    Ok(())
}

async fn prepare_local_hook_components(
    prepared: &mut PreparedPluginRuntime,
    relay: &PluginRelayClient,
    run: &TaskRunRecord,
    effective_workspace_dir: &str,
) -> Result<(), String> {
    for plugin in &run.plugin_snapshots {
        for component in plugin.component_snapshots.iter().filter(|component| {
            component.kind == PluginComponentKind::HookSet
                && component_uses_local(plugin, component)
        }) {
            match prepare_component(relay.clone(), plugin, component, effective_workspace_dir).await
            {
                Ok(component) => prepared.extend(component),
                Err(error) => {
                    prepared.cancel_all().await;
                    return Err(error);
                }
            }
        }
    }
    Ok(())
}

async fn prepare_local_non_hook_components(
    prepared: &mut PreparedPluginRuntime,
    relay: &PluginRelayClient,
    run: &TaskRunRecord,
    effective_workspace_dir: &str,
) -> Result<(), String> {
    for plugin in &run.plugin_snapshots {
        for component in plugin.component_snapshots.iter().filter(|component| {
            component.kind != PluginComponentKind::HookSet
                && component_uses_local(plugin, component)
        }) {
            match prepare_component(relay.clone(), plugin, component, effective_workspace_dir).await
            {
                Ok(component) => prepared.extend(component),
                Err(error) => {
                    prepared.cancel_all().await;
                    return Err(error);
                }
            }
        }
    }
    Ok(())
}

async fn dispatch_required_hook_stage(
    prepared: &PreparedPluginRuntime,
    event: PluginHookEvent,
    agent_key: &str,
) -> Result<(), String> {
    let outcome = prepared
        .dispatch_hook_event(
            event,
            &PluginHookEventContext {
                agent_key: Some(agent_key.to_string()),
                ..PluginHookEventContext::default()
            },
        )
        .await;
    if outcome.blocking_failure {
        prepared.cancel_all().await;
        return Err(hook_lifecycle_error(event, &outcome));
    }
    Ok(())
}
