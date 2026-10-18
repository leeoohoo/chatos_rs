// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[path = "task_graph_builder.rs"]
pub(super) mod task_graph_builder;

use super::{CreateTaskArgs, CreateTasksArgs};
use crate::application::task_tool_support::validate_nonempty_text;
use chatos_local_agent_protocol::LocalAgentRunRecord;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::application) struct RequiredTaskPolicy {
    #[serde(default)]
    pub(super) max_iterations: Option<u32>,
    #[serde(default)]
    pub(super) enabled_builtin_kinds: Vec<String>,
    #[serde(default)]
    pub(super) external_mcp_config_ids: Vec<String>,
    #[serde(default)]
    pub(super) plugin_components: Vec<TaskPluginHint>,
    #[serde(default)]
    pub(super) plugin_release_snapshots: Option<Vec<TaskPluginReleaseSnapshot>>,
    #[serde(default)]
    pub(super) allowed_model_config_ids: Option<Vec<String>>,
    #[serde(skip)]
    pub(super) explicit_plugin_selection: Option<(String, String)>,
}

impl RequiredTaskPolicy {
    pub(super) fn max_iterations(&self, fallback: u32) -> u32 {
        self.max_iterations.unwrap_or(fallback).max(1)
    }

    pub(super) fn apply_single(&self, args: &mut CreateTaskArgs) -> Result<(), String> {
        self.validate_explicit_plugin_hints(&args.plugin_hints)?;
        extend_unique(&mut args.enabled_builtin_kinds, &self.enabled_builtin_kinds);
        extend_unique(
            &mut args.external_mcp_config_ids,
            &self.external_mcp_config_ids,
        );
        extend_required_plugin_hints(&mut args.plugin_hints, &self.plugin_components);
        args.plugin_release_snapshots = selected_plugin_release_snapshots(
            &args.plugin_hints,
            self.plugin_release_snapshots.as_deref(),
        )?;
        Ok(())
    }

    pub(super) fn apply_batch(&self, args: &mut CreateTasksArgs) -> Result<(), String> {
        for task in &mut args.tasks {
            self.validate_explicit_plugin_hints(&task.plugin_hints)?;
            extend_unique(&mut task.enabled_builtin_kinds, &self.enabled_builtin_kinds);
            extend_unique(
                &mut task.external_mcp_config_ids,
                &self.external_mcp_config_ids,
            );
            extend_required_plugin_hints(&mut task.plugin_hints, &self.plugin_components);
            task.plugin_release_snapshots = selected_plugin_release_snapshots(
                &task.plugin_hints,
                self.plugin_release_snapshots.as_deref(),
            )?;
        }
        Ok(())
    }

    pub(in crate::application) fn bind_explicit_selection(
        &mut self,
        parent: &LocalAgentRunRecord,
    ) -> Result<(), String> {
        let Some(selection) = parent.input.get("explicit_capability_selection") else {
            return Ok(());
        };
        match selection.get("kind").and_then(Value::as_str) {
            Some("plugin") => {
                let plugin_key = selection
                    .get("plugin_key")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or("explicit Plugin selection is missing plugin_key")?
                    .to_string();
                let component_key = selection
                    .get("component_key")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or("explicit Plugin selection is missing component_key")?
                    .to_string();
                let frozen = self
                    .plugin_release_snapshots
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .any(|snapshot| snapshot.plugin_key == plugin_key && snapshot.component_key == component_key);
                if !frozen {
                    return Err(format!(
                        "the Plugin component selected in the composer is not available in the current frozen catalog: {plugin_key}/{component_key}"
                    ));
                }
                extend_required_plugin_hints(&mut self.plugin_components, &[TaskPluginHint {
                    plugin_key: plugin_key.clone(),
                    component_key: component_key.clone(),
                    reason: "selected explicitly in Main Chat".to_string(),
                }]);
                self.explicit_plugin_selection = Some((plugin_key, component_key));
                Ok(())
            }
            Some("requirement_survey") => Err(
                "Requirement Survey was selected in the composer; call requirement_survey_create directly instead of creating a Task"
                    .to_string(),
            ),
            Some(kind) => Err(format!(
                "unsupported explicit capability selection for Task creation: {kind}"
            )),
            None => Err("explicit capability selection is missing kind".to_string()),
        }
    }

    fn validate_explicit_plugin_hints(&self, hints: &[TaskPluginHint]) -> Result<(), String> {
        let Some((selected_plugin, selected_component)) = self.explicit_plugin_selection.as_ref()
        else {
            return Ok(());
        };
        let allowed = self
            .plugin_components
            .iter()
            .map(|hint| (hint.plugin_key.trim(), hint.component_key.trim()))
            .collect::<HashSet<_>>();
        if let Some(unexpected) = hints
            .iter()
            .map(|hint| (hint.plugin_key.trim(), hint.component_key.trim()))
            .find(|pair| !allowed.contains(pair))
        {
            return Err(format!(
                "Task Plugin {}/{} is outside the user's explicit composer selection {}/{}",
                unexpected.0, unexpected.1, selected_plugin, selected_component
            ));
        }
        Ok(())
    }
}

fn extend_unique(target: &mut Vec<String>, required: &[String]) {
    let mut seen = target
        .iter()
        .map(|value| value.trim().to_string())
        .collect::<HashSet<_>>();
    for value in required {
        let value = value.trim();
        if !value.is_empty() && seen.insert(value.to_string()) {
            target.push(value.to_string());
        }
    }
}

fn extend_required_plugin_hints(target: &mut Vec<TaskPluginHint>, required: &[TaskPluginHint]) {
    let mut seen = target
        .iter()
        .map(|hint| {
            (
                hint.plugin_key.trim().to_string(),
                hint.component_key.trim().to_string(),
            )
        })
        .collect::<HashSet<_>>();
    for required_hint in required {
        let key = required_hint.plugin_key.trim();
        let component = required_hint.component_key.trim();
        if !key.is_empty()
            && !component.is_empty()
            && seen.insert((key.to_string(), component.to_string()))
        {
            target.push(required_hint.clone());
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::application) struct CreateTaskItem {
    pub(in crate::application) client_ref: String,
    title: String,
    objective: String,
    expected_outputs: Vec<String>,
    acceptance_criteria: Vec<String>,
    #[serde(default)]
    final_response_max_chars: Option<u64>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    input_payload: Value,
    #[serde(default)]
    default_model_config_id: Option<String>,
    #[serde(default)]
    thinking_level: Option<String>,
    requires_execution: bool,
    pub(in crate::application) enabled_builtin_kinds: Vec<String>,
    #[serde(default)]
    pub(in crate::application) notepad_user_request_quote: Option<String>,
    #[serde(default)]
    external_mcp_config_ids: Vec<String>,
    #[serde(default)]
    plugin_hints: Vec<TaskPluginHint>,
    #[serde(skip)]
    plugin_release_snapshots: Option<Vec<TaskPluginReleaseSnapshot>>,
    #[serde(default)]
    priority: Option<i64>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    owned_paths: Vec<String>,
    #[serde(default)]
    notification_milestone: bool,
    #[serde(default)]
    pub(in crate::application) prerequisite_refs: Vec<String>,
    #[serde(default)]
    pub(in crate::application) context_refs: Vec<String>,
    #[serde(default)]
    pub(in crate::application) prerequisite_task_ids: Vec<String>,
    #[serde(default)]
    schedule: Option<TaskScheduleArgs>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::application) struct TaskScheduleArgs {
    #[serde(default)]
    pub(in crate::application) mode: Option<String>,
    #[serde(default)]
    pub(in crate::application) run_at: Option<String>,
    #[serde(default)]
    pub(in crate::application) interval_seconds: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::application) struct TaskPluginHint {
    pub(in crate::application) plugin_key: String,
    pub(in crate::application) component_key: String,
    #[serde(default)]
    pub(in crate::application) reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct TaskPluginReleaseSnapshot {
    pub(super) plugin_key: String,
    pub(super) component_key: String,
    pub(super) plugin_id: String,
    pub(super) release_id: String,
    pub(super) version: String,
    pub(super) artifact_sha256: String,
}

fn selected_plugin_release_snapshots(
    hints: &[TaskPluginHint],
    catalog: Option<&[TaskPluginReleaseSnapshot]>,
) -> Result<Option<Vec<TaskPluginReleaseSnapshot>>, String> {
    if hints.is_empty() {
        return Ok(Some(Vec::new()));
    }
    let catalog = catalog.ok_or("Task capability policy must include plugin_release_snapshots")?;
    let mut by_key = HashMap::new();
    for snapshot in catalog {
        for (field, value) in [
            ("plugin_key", snapshot.plugin_key.as_str()),
            ("component_key", snapshot.component_key.as_str()),
            ("plugin_id", snapshot.plugin_id.as_str()),
            ("release_id", snapshot.release_id.as_str()),
            ("version", snapshot.version.as_str()),
            ("artifact_sha256", snapshot.artifact_sha256.as_str()),
        ] {
            validate_nonempty_text(value, field)?;
        }
        if by_key
            .insert(
                (
                    snapshot.plugin_key.as_str(),
                    snapshot.component_key.as_str(),
                ),
                snapshot,
            )
            .is_some()
        {
            return Err(format!(
                "required Task policy contains duplicate Plugin component: {}/{}",
                snapshot.plugin_key, snapshot.component_key
            ));
        }
    }
    let mut selected = Vec::with_capacity(hints.len());
    for hint in hints {
        let key = hint.plugin_key.trim();
        let component = hint.component_key.trim();
        let snapshot = by_key.get(&(key, component)).ok_or_else(|| {
            format!("Plugin component is not frozen in the current capability policy: {key}/{component}")
        })?;
        selected.push((*snapshot).clone());
    }
    selected.sort_by(|left, right| {
        (&left.plugin_key, &left.component_key).cmp(&(&right.plugin_key, &right.component_key))
    });
    Ok(Some(selected))
}

pub(super) fn validate_optional_notepad_selection(
    enabled_builtin_kinds: &[String],
    request_quote: Option<&str>,
    source_user_message: Option<&str>,
) -> Result<(), String> {
    let selects_notepad = enabled_builtin_kinds
        .iter()
        .any(|kind| kind.trim() == "Notepad");
    let quote = request_quote
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if !selects_notepad {
        return if quote.is_none() {
            Ok(())
        } else {
            Err(
                "notepad_user_request_quote requires enabled_builtin_kinds to contain Notepad"
                    .to_string(),
            )
        };
    }
    let quote = quote.ok_or_else(|| {
        "Selecting optional Notepad requires notepad_user_request_quote copied exactly from the current user message".to_string()
    })?;
    if quote.chars().count() < 2 || quote.chars().count() > 500 {
        return Err("notepad_user_request_quote must contain 2..=500 characters".to_string());
    }
    let source = source_user_message.ok_or_else(|| {
        "Selecting optional Notepad requires an available current user message".to_string()
    })?;
    if !source.contains(quote) {
        return Err(
            "notepad_user_request_quote must be an exact substring of the current user message"
                .to_string(),
        );
    }
    Ok(())
}
