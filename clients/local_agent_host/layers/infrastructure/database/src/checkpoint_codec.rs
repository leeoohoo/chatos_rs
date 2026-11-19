// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_local_agent_ports::ClientStorageError;
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use serde_json::Value;
use std::io::{Read, Write};

const COMPRESSION_THRESHOLD_BYTES: usize = 768 * 1_024;
const MAXIMUM_DECOMPRESSED_BYTES: usize = 128 * 1_024 * 1_024;
const HEADER: &[u8] = b"chatos-checkpoint-gzip-v1\0";

pub(super) fn encode(value: &Value) -> Result<Vec<u8>, ClientStorageError> {
    let raw = serde_json::to_vec(value)?;
    if raw.len() <= COMPRESSION_THRESHOLD_BYTES {
        return Ok(raw);
    }
    if raw.len() > MAXIMUM_DECOMPRESSED_BYTES {
        return Err(ClientStorageError::InvalidState(format!(
            "checkpoint exceeds {} bytes",
            MAXIMUM_DECOMPRESSED_BYTES
        )));
    }
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder
        .write_all(&raw)
        .map_err(ClientStorageError::database)?;
    let compressed = encoder.finish().map_err(ClientStorageError::database)?;
    let mut stored = Vec::with_capacity(HEADER.len() + 8 + compressed.len());
    stored.extend_from_slice(HEADER);
    stored.extend_from_slice(&(raw.len() as u64).to_le_bytes());
    stored.extend_from_slice(&compressed);
    Ok(if stored.len() < raw.len() {
        stored
    } else {
        raw
    })
}

pub(super) fn decode(stored: &[u8]) -> Result<Value, ClientStorageError> {
    let Some(encoded) = stored.strip_prefix(HEADER) else {
        return serde_json::from_slice(stored).map_err(Into::into);
    };
    let size = encoded.get(..8).ok_or_else(|| {
        ClientStorageError::InvalidState("missing checkpoint uncompressed size".into())
    })?;
    let expected_bytes = usize::try_from(u64::from_le_bytes(size.try_into().unwrap()))
        .ok()
        .filter(|bytes| *bytes <= MAXIMUM_DECOMPRESSED_BYTES)
        .ok_or_else(|| {
            ClientStorageError::InvalidState("invalid checkpoint uncompressed size".into())
        })?;
    let mut raw = Vec::with_capacity(expected_bytes);
    GzDecoder::new(&encoded[8..])
        .take(expected_bytes.saturating_add(1) as u64)
        .read_to_end(&mut raw)
        .map_err(|error| {
            ClientStorageError::InvalidState(format!("invalid gzip checkpoint data: {error}"))
        })?;
    if raw.len() != expected_bytes {
        return Err(ClientStorageError::InvalidState(format!(
            "checkpoint size mismatch: expected {expected_bytes}, decoded {}",
            raw.len()
        )));
    }
    serde_json::from_slice(&raw).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn large_checkpoint_is_compressed_and_restored_exactly() {
        let checkpoint = json!({
            "response": {
                "request_input_items": [{
                    "type": "function_call_output",
                    "call_id": "call-1",
                    "output": "T".repeat(1_000_000)
                }]
            }
        });

        let stored = encode(&checkpoint).expect("encode checkpoint");

        assert!(stored.starts_with(HEADER));
        assert!(stored.len() < COMPRESSION_THRESHOLD_BYTES);
        assert_eq!(decode(&stored).expect("decode checkpoint"), checkpoint);
    }

    #[test]
    fn legacy_json_checkpoint_remains_compatible() {
        let checkpoint = json!({"response": {"request_input_items": []}});
        let stored = serde_json::to_vec(&checkpoint).unwrap();

        assert_eq!(encode(&checkpoint).unwrap(), stored);
        assert_eq!(decode(&stored).unwrap(), checkpoint);
    }
}
