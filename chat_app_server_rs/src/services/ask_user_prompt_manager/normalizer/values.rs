// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use serde_json::Value;

use super::fields::KvField;

pub fn normalize_kv_values(values: Option<&Value>, fields: &[KvField]) -> HashMap<String, String> {
    let mut out: HashMap<String, String> = HashMap::new();
    let field_map: HashMap<String, &KvField> = fields
        .iter()
        .map(|field| (field.key.clone(), field))
        .collect();

    if let Some(Value::Object(map)) = values {
        for (key, value) in map {
            let normalized_key = key.trim().to_string();
            if normalized_key.is_empty() || !field_map.contains_key(&normalized_key) {
                continue;
            }

            let normalized_value = if let Some(raw) = value.as_str() {
                raw.to_string()
            } else if value.is_null() {
                String::new()
            } else {
                value.to_string()
            };

            out.insert(normalized_key, normalized_value);
        }
    }

    for field in fields {
        let current = out.get(&field.key).map(|value| value.trim()).unwrap_or("");
        if current.is_empty() && !field.default_value.is_empty() {
            out.insert(field.key.clone(), field.default_value.clone());
        }
    }

    out
}
