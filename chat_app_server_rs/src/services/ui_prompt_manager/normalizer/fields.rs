use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::support::trimmed;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvField {
    pub key: String,
    pub label: String,
    pub description: String,
    pub placeholder: String,
    pub default_value: String,
    pub required: bool,
    pub multiline: bool,
    pub secret: bool,
}

pub fn normalize_kv_fields(
    value: Option<&Value>,
    max_fields: usize,
) -> Result<Vec<KvField>, String> {
    let fields = value
        .and_then(Value::as_array)
        .ok_or_else(|| "fields is required".to_string())?;
    if fields.is_empty() {
        return Err("fields is required".to_string());
    }
    if fields.len() > max_fields {
        return Err(format!("fields must be <= {max_fields}"));
    }

    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(fields.len());
    for (index, field) in fields.iter().enumerate() {
        let key = normalize_unique_kv_field_key(field, index, &seen);
        seen.insert(key.clone());

        let label = {
            let value = trimmed(field.get("label").and_then(Value::as_str));
            if value.is_empty() { key.clone() } else { value }
        };

        out.push(KvField {
            key,
            label,
            description: trimmed(field.get("description").and_then(Value::as_str)),
            placeholder: trimmed(field.get("placeholder").and_then(Value::as_str)),
            default_value: field
                .get("default")
                .or_else(|| field.get("default_value"))
                .and_then(Value::as_str)
                .map(|value| value.to_string())
                .unwrap_or_default(),
            required: field
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            multiline: field
                .get("multiline")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            secret: field
                .get("secret")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        });
    }

    Ok(out)
}

fn normalize_unique_kv_field_key(field: &Value, index: usize, seen: &HashSet<String>) -> String {
    let explicit = field
        .get("key")
        .and_then(Value::as_str)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            field
                .get("name")
                .and_then(Value::as_str)
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .or_else(|| {
            field
                .get("id")
                .and_then(Value::as_str)
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        });

    let base_key = explicit
        .or_else(|| {
            field
                .get("label")
                .and_then(Value::as_str)
                .map(slugify_fallback_key)
                .filter(|value| !value.is_empty())
        })
        .or_else(|| {
            field
                .get("placeholder")
                .and_then(Value::as_str)
                .map(slugify_fallback_key)
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| format!("field_{}", index + 1));

    ensure_unique_key(base_key, seen)
}

fn slugify_fallback_key(raw: &str) -> String {
    let mut out = String::new();
    let mut last_sep = false;

    for ch in raw.trim().chars() {
        if ch.is_alphanumeric() {
            if ch.is_ascii() {
                out.push(ch.to_ascii_lowercase());
            } else {
                out.push(ch);
            }
            last_sep = false;
            continue;
        }

        if (ch == '_' || ch == '-' || ch.is_whitespace()) && !out.is_empty() && !last_sep {
            out.push('_');
            last_sep = true;
        }
    }

    out.trim_matches('_').to_string()
}

fn ensure_unique_key(base_key: String, seen: &HashSet<String>) -> String {
    if !seen.contains(&base_key) {
        return base_key;
    }

    let mut idx = 2;
    loop {
        let candidate = format!("{}_{}", base_key, idx);
        if !seen.contains(&candidate) {
            return candidate;
        }
        idx += 1;
    }
}
