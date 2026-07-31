// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet, HashSet};

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use super::format_helpers::required_json_text;

const MAX_TEMPLATE_PLACEHOLDERS: usize = 100;
const MAX_TEMPLATE_VALUE_CHARS: usize = 500_000;
pub(super) const MAX_TEMPLATE_ZIP_ENTRIES: usize = 10_000;

#[derive(Clone, Debug)]
pub(super) struct TemplatePlaceholder {
    pub(super) name: String,
    pub(super) token: String,
    description: String,
    required: bool,
    default: Option<String>,
    max_length: usize,
    pub(super) occurrences: usize,
}

impl TemplatePlaceholder {
    pub(super) fn manifest_value(&self) -> Value {
        json!({
            "name":self.name,
            "token":self.token,
            "description":self.description,
            "required":self.required,
            "default":self.default,
            "max_length":self.max_length,
            "occurrences":self.occurrences
        })
    }
}

pub(super) fn template_argument_placeholders(
    arguments: &Value,
) -> Result<Vec<TemplatePlaceholder>> {
    let Some(value) = arguments.get("placeholders") else {
        return Ok(Vec::new());
    };
    let items = value
        .as_array()
        .ok_or_else(|| anyhow!("placeholders must be an array"))?;
    if items.len() > MAX_TEMPLATE_PLACEHOLDERS {
        return Err(anyhow!("template contains too many placeholders"));
    }
    let mut names = BTreeSet::new();
    items
        .iter()
        .map(|item| {
            let object = item
                .as_object()
                .ok_or_else(|| anyhow!("each placeholder must be an object"))?;
            let name = object
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("placeholder name is required"))?;
            validate_placeholder_name(name)?;
            if !names.insert(name.to_string()) {
                return Err(anyhow!("placeholder names must be unique"));
            }
            let max_length = object
                .get("max_length")
                .map(|value| {
                    value
                        .as_u64()
                        .ok_or_else(|| anyhow!("placeholder max_length must be an integer"))
                })
                .transpose()?
                .unwrap_or(100_000);
            let max_length = usize::try_from(max_length)
                .ok()
                .filter(|value| (1..=100_000).contains(value))
                .ok_or_else(|| anyhow!("placeholder max_length must be between 1 and 100000"))?;
            let default = object
                .get("default")
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_string)
                        .ok_or_else(|| anyhow!("placeholder default must be a string"))
                })
                .transpose()?;
            if default
                .as_ref()
                .is_some_and(|value| value.chars().count() > max_length)
            {
                return Err(anyhow!("placeholder default exceeds max_length"));
            }
            Ok(TemplatePlaceholder {
                name: name.to_string(),
                token: format!("{{{{{name}}}}}"),
                description: object
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                required: object
                    .get("required")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                default,
                max_length,
                occurrences: 0,
            })
        })
        .collect()
}

pub(super) fn template_manifest_placeholders(manifest: &Value) -> Result<Vec<TemplatePlaceholder>> {
    if manifest.get("schema_version").and_then(Value::as_u64) == Some(1) {
        return Ok(Vec::new());
    }
    let items = manifest
        .get("placeholders")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("template manifest is missing placeholders"))?;
    if items.len() > MAX_TEMPLATE_PLACEHOLDERS {
        return Err(anyhow!("template manifest contains too many placeholders"));
    }
    let mut names = BTreeSet::new();
    items
        .iter()
        .map(|item| {
            let name = required_json_text(item, "name")?;
            validate_placeholder_name(name)?;
            if !names.insert(name.to_string()) {
                return Err(anyhow!("template manifest contains duplicate placeholders"));
            }
            let token = required_json_text(item, "token")?;
            if token != format!("{{{{{name}}}}}") {
                return Err(anyhow!(
                    "template manifest contains an invalid placeholder token"
                ));
            }
            let max_length = item
                .get("max_length")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .filter(|value| (1..=100_000).contains(value))
                .ok_or_else(|| anyhow!("template manifest placeholder max_length is invalid"))?;
            let occurrences = item
                .get("occurrences")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .filter(|value| *value > 0)
                .ok_or_else(|| anyhow!("template manifest placeholder occurrences is invalid"))?;
            Ok(TemplatePlaceholder {
                name: name.to_string(),
                token: token.to_string(),
                description: item
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                required: item
                    .get("required")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                default: item
                    .get("default")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                max_length,
                occurrences,
            })
        })
        .collect()
}

fn validate_placeholder_name(name: &str) -> Result<()> {
    let mut characters = name.chars();
    let first = characters
        .next()
        .ok_or_else(|| anyhow!("placeholder name cannot be empty"))?;
    if !first.is_ascii_alphabetic()
        || name.len() > 64
        || characters.any(|character| !(character.is_ascii_alphanumeric() || character == '_'))
    {
        return Err(anyhow!(
            "placeholder name must match [A-Za-z][A-Za-z0-9_]{{0,63}}"
        ));
    }
    Ok(())
}

pub(super) fn template_values(
    arguments: &Value,
    placeholders: &[TemplatePlaceholder],
) -> Result<BTreeMap<String, String>> {
    let values = arguments
        .get("values")
        .map(|value| {
            value
                .as_object()
                .ok_or_else(|| anyhow!("values must be an object of string values"))
        })
        .transpose()?
        .cloned()
        .unwrap_or_default();
    if values.len() > MAX_TEMPLATE_PLACEHOLDERS {
        return Err(anyhow!("template values contain too many properties"));
    }
    let known = placeholders
        .iter()
        .map(|placeholder| placeholder.name.as_str())
        .collect::<HashSet<_>>();
    if let Some(unknown) = values.keys().find(|name| !known.contains(name.as_str())) {
        return Err(anyhow!(
            "template value was provided for unknown placeholder: {unknown}"
        ));
    }
    let mut total_chars = 0usize;
    let mut output = BTreeMap::new();
    for placeholder in placeholders {
        let value = values
            .get(placeholder.name.as_str())
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_string)
                    .ok_or_else(|| anyhow!("template placeholder values must be strings"))
            })
            .transpose()?
            .or_else(|| placeholder.default.clone())
            .or_else(|| (!placeholder.required).then(String::new))
            .ok_or_else(|| {
                anyhow!(
                    "required template placeholder value is missing: {}",
                    placeholder.name
                )
            })?;
        let chars = value.chars().count();
        if chars > placeholder.max_length {
            return Err(anyhow!(
                "template placeholder value exceeds max_length: {}",
                placeholder.name
            ));
        }
        if value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\t' | '\r' | '\n'))
        {
            return Err(anyhow!(
                "template placeholder value contains XML-incompatible control characters"
            ));
        }
        total_chars = total_chars.saturating_add(chars);
        if total_chars > MAX_TEMPLATE_VALUE_CHARS {
            return Err(anyhow!(
                "template values exceed the {MAX_TEMPLATE_VALUE_CHARS} character safety limit"
            ));
        }
        output.insert(placeholder.name.clone(), value);
    }
    Ok(output)
}
