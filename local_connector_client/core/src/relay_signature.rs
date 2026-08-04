// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use crate::relay::RelayRequest;

pub(crate) fn relay_request_signature_payload(request: &RelayRequest) -> Result<Vec<u8>, String> {
    let key_id = required_text(
        request.platform_signature_key_id.as_deref(),
        "relay platform_signature_key_id",
    )?;
    let algorithm = required_text(
        request.platform_signature_alg.as_deref(),
        "relay platform_signature_alg",
    )?;
    let timestamp = request
        .platform_timestamp
        .ok_or_else(|| "relay platform_timestamp is required".to_string())?;
    let nonce = required_text(request.platform_nonce.as_deref(), "relay platform_nonce")?;
    let headers = canonical_json_string(&Value::Object(
        request
            .headers
            .iter()
            .map(|(key, value)| (key.clone(), Value::String(value.clone())))
            .collect(),
    ))?;
    let body = canonical_json_string(&request.body)?;
    Ok(format!(
        "v1\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        request._message_type,
        request.request_id,
        request.owner_user_id.as_deref().unwrap_or_default(),
        request.device_id.as_deref().unwrap_or_default(),
        request.workspace_id,
        request.method.as_deref().unwrap_or_default(),
        request.path.as_deref().unwrap_or_default(),
        key_id,
        algorithm,
        timestamp,
        nonce,
        headers,
        body,
    )
    .into_bytes())
}

fn required_text(value: Option<&str>, label: &str) -> Result<String, String> {
    let value = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{label} is required"))?;
    Ok(value.to_string())
}

fn canonical_json_string(value: &Value) -> Result<String, String> {
    let mut output = String::new();
    write_canonical_json(value, &mut output)?;
    Ok(output)
}

fn write_canonical_json(value: &Value, output: &mut String) -> Result<(), String> {
    match value {
        Value::Null => output.push_str("null"),
        Value::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
        Value::Number(value) => output.push_str(value.to_string().as_str()),
        Value::String(value) => {
            let encoded = serde_json::to_string(value)
                .map_err(|err| format!("encode relay signature string failed: {err}"))?;
            output.push_str(encoded.as_str());
        }
        Value::Array(values) => {
            output.push('[');
            for (index, item) in values.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                write_canonical_json(item, output)?;
            }
            output.push(']');
        }
        Value::Object(map) => {
            output.push('{');
            let mut entries = map.iter().collect::<Vec<_>>();
            entries.sort_by(|(left, _), (right, _)| left.cmp(right));
            for (index, (key, item)) in entries.into_iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                let encoded_key = serde_json::to_string(key)
                    .map_err(|err| format!("encode relay signature object key failed: {err}"))?;
                output.push_str(encoded_key.as_str());
                output.push(':');
                write_canonical_json(item, output)?;
            }
            output.push('}');
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_json_sorts_object_keys_recursively() {
        let value = serde_json::json!({
            "z": 1,
            "a": {
                "d": [3, {"y": false, "b": true}],
                "b": "x"
            }
        });
        let canonical = canonical_json_string(&value).expect("canonical JSON");
        assert_eq!(
            canonical,
            r#"{"a":{"b":"x","d":[3,{"b":true,"y":false}]},"z":1}"#
        );
    }
}
