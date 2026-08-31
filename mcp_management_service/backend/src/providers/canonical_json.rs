// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;
use sha2::{Digest, Sha256};

pub(super) fn canonical_json_bytes(value: &Value) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    write_canonical_json(value, &mut output)?;
    Ok(output)
}

pub(super) fn canonical_json_sha256(value: &Value) -> Result<String, String> {
    canonical_json_bytes(value).map(|bytes| hex::encode(Sha256::digest(bytes)))
}

fn write_canonical_json(value: &Value, output: &mut Vec<u8>) -> Result<(), String> {
    match value {
        Value::Null => output.extend_from_slice(b"null"),
        Value::Bool(value) => output.extend_from_slice(if *value { b"true" } else { b"false" }),
        Value::Number(value) => output.extend_from_slice(value.to_string().as_bytes()),
        Value::String(value) => output.extend_from_slice(
            serde_json::to_string(value)
                .map_err(|error| format!("encode canonical JSON string failed: {error}"))?
                .as_bytes(),
        ),
        Value::Array(values) => {
            output.push(b'[');
            for (index, item) in values.iter().enumerate() {
                if index > 0 {
                    output.push(b',');
                }
                write_canonical_json(item, output)?;
            }
            output.push(b']');
        }
        Value::Object(map) => {
            output.push(b'{');
            let mut entries = map.iter().collect::<Vec<_>>();
            entries.sort_by(|(left, _), (right, _)| left.cmp(right));
            for (index, (key, item)) in entries.into_iter().enumerate() {
                if index > 0 {
                    output.push(b',');
                }
                output.extend_from_slice(
                    serde_json::to_string(key)
                        .map_err(|error| {
                            format!("encode canonical JSON object key failed: {error}")
                        })?
                        .as_bytes(),
                );
                output.push(b':');
                write_canonical_json(item, output)?;
            }
            output.push(b'}');
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonical_json_sorts_nested_object_keys_without_reordering_arrays() {
        let first = json!({"z": [{"b": 2, "a": 1}], "a": true});
        let mut top = serde_json::Map::new();
        top.insert("a".to_string(), Value::Bool(true));
        top.insert(
            "z".to_string(),
            Value::Array(vec![Value::Object(serde_json::Map::from_iter([
                ("a".to_string(), json!(1)),
                ("b".to_string(), json!(2)),
            ]))]),
        );
        let second = Value::Object(top);

        assert_eq!(
            canonical_json_bytes(&first).unwrap(),
            br#"{"a":true,"z":[{"a":1,"b":2}]}"#
        );
        assert_eq!(
            canonical_json_sha256(&first).unwrap(),
            canonical_json_sha256(&second).unwrap()
        );
    }
}
