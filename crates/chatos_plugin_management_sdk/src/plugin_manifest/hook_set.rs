// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use super::{normalize_plugin_relative_path, PluginPathRef};

pub const PLUGIN_HOOK_SET_SCHEMA_VERSION_V1: u32 = 1;
pub const PLUGIN_HOOK_MAX_DEFINITIONS: usize = 64;
pub const PLUGIN_HOOK_DEFAULT_TIMEOUT_MS: u64 = 5_000;
pub const PLUGIN_HOOK_MAX_TIMEOUT_MS: u64 = 30_000;
pub const PLUGIN_HOOK_DEFAULT_OUTPUT_BYTES: usize = 64 * 1024;
pub const PLUGIN_HOOK_MAX_OUTPUT_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum PluginHookEvent {
    SessionStart,
    BeforePluginPrepare,
    PreToolUse,
    PostToolUse,
    RunCompleted,
    RunFailed,
    PluginDisabled,
}

impl PluginHookEvent {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SessionStart => "SessionStart",
            Self::BeforePluginPrepare => "BeforePluginPrepare",
            Self::PreToolUse => "PreToolUse",
            Self::PostToolUse => "PostToolUse",
            Self::RunCompleted => "RunCompleted",
            Self::RunFailed => "RunFailed",
            Self::PluginDisabled => "PluginDisabled",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginHookFailurePolicy {
    #[default]
    Continue,
    FailRun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginHookOutcome {
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginHookMatcher {
    #[serde(default)]
    pub tool_names: Vec<String>,
    #[serde(default)]
    pub tool_kinds: Vec<String>,
    #[serde(default)]
    pub agent_keys: Vec<String>,
    #[serde(default)]
    pub component_keys: Vec<String>,
    #[serde(default)]
    pub outcomes: Vec<PluginHookOutcome>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginHookEventContext {
    #[serde(default)]
    pub agent_key: Option<String>,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub tool_kind: Option<String>,
    #[serde(default)]
    pub component_key: Option<String>,
    #[serde(default)]
    pub outcome: Option<PluginHookOutcome>,
    #[serde(default)]
    pub summary_sha256: Option<String>,
}

impl PluginHookMatcher {
    pub fn matches(&self, context: &PluginHookEventContext) -> bool {
        matches_optional(&self.tool_names, context.tool_name.as_deref())
            && matches_optional(&self.tool_kinds, context.tool_kind.as_deref())
            && matches_optional(&self.agent_keys, context.agent_key.as_deref())
            && matches_optional(&self.component_keys, context.component_key.as_deref())
            && (self.outcomes.is_empty()
                || context
                    .outcome
                    .is_some_and(|outcome| self.outcomes.contains(&outcome)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PluginHookEntrypoint {
    Command {
        command: PluginPathRef,
        #[serde(default)]
        args: Vec<String>,
    },
}

impl PluginHookEntrypoint {
    pub fn command(&self) -> &PluginPathRef {
        match self {
            Self::Command { command, .. } => command,
        }
    }

    pub fn args(&self) -> &[String] {
        match self {
            Self::Command { args, .. } => args,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginHookDefinition {
    pub id: String,
    pub events: Vec<PluginHookEvent>,
    #[serde(default)]
    pub matcher: PluginHookMatcher,
    pub entrypoint: PluginHookEntrypoint,
    #[serde(default = "default_hook_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_hook_output_bytes")]
    pub max_output_bytes: usize,
    #[serde(default)]
    pub failure_policy: PluginHookFailurePolicy,
    #[serde(default)]
    pub workspace_write: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginHookSet {
    #[serde(default = "default_hook_schema_version")]
    pub schema_version: u32,
    pub hooks: Vec<PluginHookDefinition>,
}

#[derive(Debug, Error)]
pub enum PluginHookSetError {
    #[error("invalid Plugin Hook set JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid Plugin Hook set field {field}: {message}")]
    InvalidField { field: String, message: String },
}

pub fn parse_plugin_hook_set(raw: &str) -> Result<PluginHookSet, PluginHookSetError> {
    let mut hook_set: PluginHookSet = serde_json::from_str(raw)?;
    for (index, hook) in hook_set.hooks.iter_mut().enumerate() {
        hook.id = hook.id.trim().to_string();
        hook.events.sort();
        normalize_matcher(&mut hook.matcher);
        let command = hook.entrypoint.command().path.clone();
        let command = normalize_plugin_relative_path(command.as_str()).map_err(|message| {
            PluginHookSetError::InvalidField {
                field: format!("hooks[{index}].entrypoint.command"),
                message,
            }
        })?;
        match &mut hook.entrypoint {
            PluginHookEntrypoint::Command { command: path, .. } => {
                *path = PluginPathRef::new(command);
            }
        }
    }
    hook_set.hooks.sort_by(|left, right| left.id.cmp(&right.id));
    validate_plugin_hook_set(&hook_set)?;
    Ok(hook_set)
}

pub fn validate_plugin_hook_set(hook_set: &PluginHookSet) -> Result<(), PluginHookSetError> {
    if hook_set.schema_version != PLUGIN_HOOK_SET_SCHEMA_VERSION_V1 {
        return invalid(
            "schemaVersion",
            format!(
                "unsupported schema version {}; expected {PLUGIN_HOOK_SET_SCHEMA_VERSION_V1}",
                hook_set.schema_version
            ),
        );
    }
    if hook_set.hooks.is_empty() || hook_set.hooks.len() > PLUGIN_HOOK_MAX_DEFINITIONS {
        return invalid(
            "hooks",
            format!("must contain between 1 and {PLUGIN_HOOK_MAX_DEFINITIONS} Hook definitions"),
        );
    }
    let mut ids = HashSet::new();
    for (index, hook) in hook_set.hooks.iter().enumerate() {
        let field = format!("hooks[{index}]");
        validate_identifier(format!("{field}.id").as_str(), hook.id.as_str())?;
        if !ids.insert(hook.id.as_str()) {
            return invalid(format!("{field}.id"), "duplicate Hook id");
        }
        if hook.events.is_empty() || hook.events.len() > 7 {
            return invalid(
                format!("{field}.events"),
                "must contain between 1 and 7 supported lifecycle events",
            );
        }
        if hook.events.windows(2).any(|events| events[0] == events[1]) {
            return invalid(format!("{field}.events"), "contains a duplicate event");
        }
        validate_matcher(format!("{field}.matcher").as_str(), &hook.matcher)?;
        let command = hook.entrypoint.command().path.trim_start_matches("./");
        if !command.starts_with("scripts/") && !command.starts_with("binaries/") {
            return invalid(
                format!("{field}.entrypoint.command"),
                "must point inside scripts/ or binaries/",
            );
        }
        if hook.entrypoint.args().len() > 32 {
            return invalid(
                format!("{field}.entrypoint.args"),
                "must contain at most 32 arguments",
            );
        }
        for (argument_index, argument) in hook.entrypoint.args().iter().enumerate() {
            if argument.len() > 4096 || argument.contains('\0') {
                return invalid(
                    format!("{field}.entrypoint.args[{argument_index}]"),
                    "must be at most 4096 bytes and cannot contain NUL",
                );
            }
        }
        if !(100..=PLUGIN_HOOK_MAX_TIMEOUT_MS).contains(&hook.timeout_ms) {
            return invalid(
                format!("{field}.timeoutMs"),
                format!("must be between 100 and {PLUGIN_HOOK_MAX_TIMEOUT_MS}"),
            );
        }
        if !(1024..=PLUGIN_HOOK_MAX_OUTPUT_BYTES).contains(&hook.max_output_bytes) {
            return invalid(
                format!("{field}.maxOutputBytes"),
                format!("must be between 1024 and {PLUGIN_HOOK_MAX_OUTPUT_BYTES}"),
            );
        }
    }
    Ok(())
}

pub fn normalized_plugin_hook_set_sha256(
    hook_set: &PluginHookSet,
) -> Result<String, serde_json::Error> {
    serde_json::to_vec(hook_set).map(|bytes| hex::encode(Sha256::digest(bytes)))
}

#[allow(clippy::too_many_arguments)]
pub fn plugin_hook_snapshot_sha256(
    plugin_id: &str,
    release_id: &str,
    component_key: &str,
    source_path: &str,
    content_sha256: &str,
    hook_set_sha256: &str,
    command_sha256_by_hook: &BTreeMap<String, String>,
) -> Result<String, serde_json::Error> {
    #[derive(Serialize)]
    struct Snapshot<'a> {
        purpose: &'static str,
        plugin_id: &'a str,
        release_id: &'a str,
        component_key: &'a str,
        source_path: &'a str,
        content_sha256: &'a str,
        hook_set_sha256: &'a str,
        command_sha256_by_hook: &'a BTreeMap<String, String>,
    }
    serde_json::to_vec(&Snapshot {
        purpose: "chatos.plugin.hook-set.snapshot.v1",
        plugin_id,
        release_id,
        component_key,
        source_path,
        content_sha256,
        hook_set_sha256,
        command_sha256_by_hook,
    })
    .map(|bytes| hex::encode(Sha256::digest(bytes)))
}

fn normalize_matcher(matcher: &mut PluginHookMatcher) {
    for values in [
        &mut matcher.tool_names,
        &mut matcher.tool_kinds,
        &mut matcher.agent_keys,
        &mut matcher.component_keys,
    ] {
        for value in values.iter_mut() {
            *value = value.trim().to_string();
        }
        values.sort();
    }
    matcher.outcomes.sort();
}

fn matches_optional(values: &[String], actual: Option<&str>) -> bool {
    values.is_empty() || actual.is_some_and(|actual| values.iter().any(|value| value == actual))
}

fn validate_matcher(field: &str, matcher: &PluginHookMatcher) -> Result<(), PluginHookSetError> {
    for (name, values) in [
        ("toolNames", matcher.tool_names.as_slice()),
        ("toolKinds", matcher.tool_kinds.as_slice()),
        ("agentKeys", matcher.agent_keys.as_slice()),
        ("componentKeys", matcher.component_keys.as_slice()),
    ] {
        if values.len() > 128 {
            return invalid(format!("{field}.{name}"), "must contain at most 128 items");
        }
        for (index, value) in values.iter().enumerate() {
            validate_identifier(format!("{field}.{name}[{index}]").as_str(), value)?;
        }
        if values.windows(2).any(|items| items[0] == items[1]) {
            return invalid(format!("{field}.{name}"), "contains a duplicate item");
        }
    }
    if matcher.outcomes.len() > 3
        || matcher
            .outcomes
            .windows(2)
            .any(|items| items[0] == items[1])
    {
        return invalid(
            format!("{field}.outcomes"),
            "contains too many or duplicate outcomes",
        );
    }
    Ok(())
}

fn validate_identifier(field: &str, value: &str) -> Result<(), PluginHookSetError> {
    let valid = !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'));
    if valid {
        Ok(())
    } else {
        invalid(
            field,
            "must be a 1-256 byte canonical identifier using letters, digits, '_' or '-'",
        )
    }
}

fn invalid<T>(
    field: impl Into<String>,
    message: impl Into<String>,
) -> Result<T, PluginHookSetError> {
    Err(PluginHookSetError::InvalidField {
        field: field.into(),
        message: message.into(),
    })
}

const fn default_hook_schema_version() -> u32 {
    PLUGIN_HOOK_SET_SCHEMA_VERSION_V1
}

const fn default_hook_timeout_ms() -> u64 {
    PLUGIN_HOOK_DEFAULT_TIMEOUT_MS
}

const fn default_hook_output_bytes() -> usize {
    PLUGIN_HOOK_DEFAULT_OUTPUT_BYTES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_set_normalizes_structured_matchers_and_hashes_stably() {
        let raw = r#"{
          "hooks": [{
            "id": " audit-run ",
            "events": ["RunFailed", "RunCompleted"],
            "matcher": {"agentKeys": ["task_runner_run_phase"]},
            "entrypoint": {"type": "command", "command": "scripts/audit", "args": ["--json"]},
            "failurePolicy": "continue",
            "workspaceWrite": true
          }]
        }"#;
        let hook_set = parse_plugin_hook_set(raw).expect("Hook set");
        assert_eq!(hook_set.hooks[0].id, "audit-run");
        assert_eq!(
            hook_set.hooks[0].entrypoint.command().path,
            "./scripts/audit"
        );
        assert_eq!(hook_set.hooks[0].events[0], PluginHookEvent::RunCompleted);
        assert!(hook_set.hooks[0].workspace_write);
        assert_eq!(
            normalized_plugin_hook_set_sha256(&hook_set).expect("hash"),
            normalized_plugin_hook_set_sha256(&hook_set).expect("hash")
        );
        let mut read_only = hook_set.clone();
        read_only.hooks[0].workspace_write = false;
        assert_ne!(
            normalized_plugin_hook_set_sha256(&hook_set).expect("writable hash"),
            normalized_plugin_hook_set_sha256(&read_only).expect("read-only hash")
        );
    }

    #[test]
    fn hook_set_rejects_expression_matchers_and_unsigned_command_locations() {
        let expression = r#"{
          "hooks": [{
            "id": "audit",
            "events": ["RunCompleted"],
            "matcher": {"expression": "tool.name == 'shell'"},
            "entrypoint": {"type": "command", "command": "./scripts/audit"}
          }]
        }"#;
        assert!(matches!(
            parse_plugin_hook_set(expression),
            Err(PluginHookSetError::Json(_))
        ));
        let outside = expression
            .replace(
                "\"matcher\": {\"expression\": \"tool.name == 'shell'\"},",
                "\"matcher\": {},",
            )
            .replace("./scripts/audit", "./audit");
        assert!(parse_plugin_hook_set(outside.as_str()).is_err());
    }
}
