use serde_json::{Map, Value};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TaskToolAuthorization {
    requires_execution: bool,
    enabled_builtin_kinds: HashSet<String>,
}

impl TaskToolAuthorization {
    pub(crate) fn resolve(input: &Map<String, Value>) -> Result<Self, String> {
        validate_owned_paths(input.get("owned_paths"))?;
        let options = input
            .get("tool_options")
            .and_then(Value::as_object)
            .ok_or_else(invalid_context)?;
        let requires_execution = options
            .get("requires_execution")
            .and_then(Value::as_bool)
            .ok_or_else(invalid_context)?;
        let enabled_builtin_kinds = unique_strings(
            options.get("enabled_builtin_kinds"),
            "enabled_builtin_kinds",
        )?;
        let plugin_selections = plugin_selection_ids(options.get("plugin_hints"))?;
        let release_selections = plugin_release_ids(options.get("plugin_release_snapshots"))?;
        if plugin_selections != release_selections {
            return Err(invalid_context());
        }
        if let Some(external) = options.get("external_mcp_config_ids") {
            unique_strings(Some(external), "external_mcp_config_ids")?;
        }
        Ok(Self {
            requires_execution,
            enabled_builtin_kinds,
        })
    }

    pub(crate) fn allows_terminal(&self) -> bool {
        self.requires_execution && self.enabled_builtin_kinds.contains("TerminalController")
    }

    pub(crate) fn allows_project_read(&self) -> bool {
        self.enabled_builtin_kinds.contains("CodeMaintainerRead")
    }
}

fn validate_owned_paths(value: Option<&Value>) -> Result<(), String> {
    let Some(value) = value else { return Ok(()) };
    let paths = value.as_array().ok_or_else(invalid_context)?;
    if paths.len() > 200 {
        return Err(invalid_context());
    }
    let mut unique = HashSet::new();
    for value in paths {
        let path = value.as_str().ok_or_else(invalid_context)?;
        let first = path.split('/').next().unwrap_or_default().as_bytes();
        let drive_prefixed = first.len() >= 2 && first[1] == b':' && first[0].is_ascii_alphabetic();
        if path.is_empty()
            || path.len() > 4_096
            || path.trim() != path
            || path.starts_with('~')
            || path.contains('\\')
            || path.contains('\0')
            || path
                .split('/')
                .any(|component| component.is_empty() || component == "." || component == "..")
            || drive_prefixed
            || !unique.insert(path)
        {
            return Err(invalid_context());
        }
    }
    Ok(())
}

fn unique_strings(value: Option<&Value>, _field: &str) -> Result<HashSet<String>, String> {
    let values = value
        .and_then(Value::as_array)
        .ok_or_else(invalid_context)?;
    if values.len() > 1_024 {
        return Err(invalid_context());
    }
    let mut unique = HashSet::new();
    for value in values {
        let value = value.as_str().ok_or_else(invalid_context)?;
        let trimmed = value.trim();
        if trimmed.is_empty()
            || trimmed != value
            || trimmed.len() > 1_024
            || trimmed.chars().any(char::is_control)
            || !unique.insert(trimmed.to_owned())
        {
            return Err(invalid_context());
        }
    }
    Ok(unique)
}

fn plugin_selection_ids(value: Option<&Value>) -> Result<HashSet<String>, String> {
    let hints = value
        .and_then(Value::as_array)
        .ok_or_else(invalid_context)?;
    let mut identifiers = HashSet::new();
    for hint in hints {
        let hint = hint.as_object().ok_or_else(invalid_context)?;
        let id = selection_id(hint, "plugin_key", "component_key")?;
        if !identifiers.insert(id) {
            return Err(invalid_context());
        }
    }
    Ok(identifiers)
}

fn plugin_release_ids(value: Option<&Value>) -> Result<HashSet<String>, String> {
    let snapshots = value
        .and_then(Value::as_array)
        .ok_or_else(invalid_context)?;
    let mut identifiers = HashSet::new();
    for snapshot in snapshots {
        let snapshot = snapshot.as_object().ok_or_else(invalid_context)?;
        let id = selection_id(snapshot, "plugin_key", "component_key")?;
        for key in ["plugin_id", "release_id", "version", "artifact_sha256"] {
            required_string(snapshot, key)?;
        }
        if !identifiers.insert(id) {
            return Err(invalid_context());
        }
    }
    Ok(identifiers)
}

fn selection_id(
    object: &Map<String, Value>,
    plugin_key: &str,
    component_key: &str,
) -> Result<String, String> {
    Ok(format!(
        "{}\0{}",
        required_string(object, plugin_key)?,
        required_string(object, component_key)?
    ))
}

fn required_string(object: &Map<String, Value>, key: &str) -> Result<String, String> {
    let value = object
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(invalid_context)?;
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed != value
        || trimmed.len() > 1_024
        || trimmed.chars().any(char::is_control)
    {
        Err(invalid_context())
    } else {
        Ok(trimmed.to_owned())
    }
}

fn invalid_context() -> String {
    "The local platform tool Run context is invalid.".into()
}
