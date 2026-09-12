// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::{StorageError, StorageResult};

pub(crate) struct CanonicalRecord {
    pub(crate) json: String,
    pub(crate) digest: String,
}

pub(crate) fn encode_canonical<T: Serialize>(value: &T) -> StorageResult<CanonicalRecord> {
    let value = serde_json::to_value(value).map_err(serialization_error)?;
    encode_value(value)
}

pub(crate) fn canonicalize_encoded(encoded: &str) -> StorageResult<CanonicalRecord> {
    let value = serde_json::from_str(encoded).map_err(serialization_error)?;
    encode_value(value)
}

pub(crate) fn verify_canonical(
    table: &'static str,
    id: &str,
    encoded: &str,
    expected_digest: &str,
) -> StorageResult<()> {
    let canonical = canonicalize_encoded(encoded).map_err(|_| StorageError::RecordIntegrity {
        table,
        id: id.to_string(),
    })?;
    if canonical.json != encoded || canonical.digest != expected_digest {
        return Err(StorageError::RecordIntegrity {
            table,
            id: id.to_string(),
        });
    }
    Ok(())
}

fn encode_value(mut value: Value) -> StorageResult<CanonicalRecord> {
    sort_value(&mut value);
    let json = serde_json::to_string(&value).map_err(serialization_error)?;
    let digest = format!("sha256:{:x}", Sha256::digest(json.as_bytes()));
    Ok(CanonicalRecord { json, digest })
}

fn sort_value(value: &mut Value) {
    match value {
        Value::Array(values) => values.iter_mut().for_each(sort_value),
        Value::Object(object) => {
            let old = std::mem::take(object);
            let mut entries: Vec<_> = old.into_iter().collect();
            entries.sort_unstable_by(|left, right| left.0.cmp(&right.0));
            let mut sorted = Map::new();
            for (key, mut child) in entries {
                sort_value(&mut child);
                sorted.insert(key, child);
            }
            *object = sorted;
        }
        _ => {}
    }
}

fn serialization_error(error: serde_json::Error) -> StorageError {
    StorageError::InvalidData {
        reason: format!("record JSON is invalid: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recursively_orders_object_keys_and_has_a_stable_digest() {
        let first = canonicalize_encoded(r#"{"z":1,"a":{"y":2,"b":3}}"#).unwrap();
        let second = canonicalize_encoded(r#"{"a":{"b":3,"y":2},"z":1}"#).unwrap();

        assert_eq!(first.json, r#"{"a":{"b":3,"y":2},"z":1}"#);
        assert_eq!(first.json, second.json);
        assert_eq!(first.digest, second.digest);
        assert!(first.digest.starts_with("sha256:"));
    }

    #[test]
    fn rejects_noncanonical_or_modified_records() {
        let canonical = canonicalize_encoded(r#"{"a":1,"b":2}"#).unwrap();
        assert!(verify_canonical(
            "client_projects",
            "project-1",
            &canonical.json,
            &canonical.digest
        )
        .is_ok());
        assert!(matches!(
            verify_canonical(
                "client_projects",
                "project-1",
                r#"{"b":2,"a":1}"#,
                &canonical.digest
            ),
            Err(StorageError::RecordIntegrity { .. })
        ));
    }
}
