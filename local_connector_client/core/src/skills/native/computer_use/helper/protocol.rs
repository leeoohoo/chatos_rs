// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::io::{Read, Write};
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use super::super::WindowLayoutSnapshot;

pub(super) const HELPER_PROTOCOL_VERSION: u32 = 1;
pub(super) const MAX_REQUEST_BYTES: usize = 256 * 1024;
pub(super) const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Serialize)]
pub(super) struct HelperRequest {
    pub(super) protocol_version: u32,
    #[serde(flatten)]
    pub(super) command: HelperCommand,
}

impl<'de> Deserialize<'de> for HelperRequest {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        validate_helper_request_fields(&value).map_err(serde::de::Error::custom)?;
        let protocol_version = value
            .get("protocol_version")
            .and_then(Value::as_u64)
            .ok_or_else(|| serde::de::Error::custom("protocol_version is required"))?;
        let protocol_version = u32::try_from(protocol_version)
            .map_err(|_| serde::de::Error::custom("protocol_version is invalid"))?;
        let mut command_value = value;
        if let Some(object) = command_value.as_object_mut() {
            object.remove("protocol_version");
        }
        let command = serde_json::from_value::<HelperCommand>(command_value)
            .map_err(serde::de::Error::custom)?;
        Ok(Self {
            protocol_version,
            command,
        })
    }
}

fn validate_helper_request_fields(value: &Value) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| "Computer Use helper request must be an object".to_string())?;
    let kind = object
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| "kind is required".to_string())?;
    let allowed = match kind {
        "protocol_probe" | "frontmost_window_control_target" => &["protocol_version", "kind"][..],
        "dependency_probe" => &["protocol_version", "kind", "screen_capture_only"][..],
        "request_permission" => &["protocol_version", "kind", "permission_id"][..],
        "window_layout_preflight" => &["protocol_version", "kind", "snapshot"][..],
        "execute" => &["protocol_version", "kind", "operation", "arguments"][..],
        "execute_approved" => &[
            "protocol_version",
            "kind",
            "operation",
            "arguments",
            "approved_command_args",
            "cancellation_marker",
        ][..],
        _ => return Err(format!("unknown Computer Use helper request kind: {kind}")),
    };
    for field in object.keys() {
        if !allowed.contains(&field.as_str()) {
            return Err(format!("unknown field `{field}`"));
        }
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum HelperCommand {
    ProtocolProbe,
    FrontmostWindowControlTarget,
    WindowLayoutPreflight {
        snapshot: WindowLayoutSnapshot,
    },
    DependencyProbe {
        screen_capture_only: bool,
    },
    RequestPermission {
        permission_id: String,
    },
    Execute {
        operation: String,
        arguments: Value,
    },
    ExecuteApproved {
        operation: String,
        arguments: Value,
        approved_command_args: Option<Vec<String>>,
        cancellation_marker: PathBuf,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HelperResponse {
    pub(super) protocol_version: u32,
    pub(super) success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<String>,
}

impl HelperResponse {
    pub(super) fn success(result: Value) -> Self {
        Self {
            protocol_version: HELPER_PROTOCOL_VERSION,
            success: true,
            result: Some(result),
            error: None,
        }
    }

    pub(super) fn error(error: impl Into<String>) -> Self {
        Self {
            protocol_version: HELPER_PROTOCOL_VERSION,
            success: false,
            result: None,
            error: Some(error.into()),
        }
    }
}

pub(super) fn write_frame<W: Write>(writer: &mut W, payload: &[u8], limit: usize) -> Result<()> {
    if payload.len() > limit || payload.len() > u32::MAX as usize {
        bail!("Computer Use helper frame exceeded {limit} bytes");
    }
    writer
        .write_all(&(payload.len() as u32).to_le_bytes())
        .context("write Computer Use helper frame length")?;
    writer
        .write_all(payload)
        .context("write Computer Use helper frame payload")?;
    writer.flush().context("flush Computer Use helper frame")
}

pub(super) fn read_frame<R: Read>(reader: &mut R, limit: usize) -> Result<Vec<u8>> {
    let mut length = [0_u8; 4];
    reader
        .read_exact(&mut length)
        .context("read Computer Use helper frame length")?;
    let length = u32::from_le_bytes(length) as usize;
    if length > limit {
        bail!("Computer Use helper frame exceeded {limit} bytes");
    }
    let mut payload = vec![0_u8; length];
    reader
        .read_exact(payload.as_mut_slice())
        .context("read Computer Use helper frame payload")?;
    let mut trailing = [0_u8; 1];
    if reader
        .read(&mut trailing)
        .context("read Computer Use helper trailing data")?
        != 0
    {
        bail!("Computer Use helper received trailing protocol data");
    }
    Ok(payload)
}

pub(super) fn decode_frame(bytes: &[u8], limit: usize) -> Result<HelperResponse> {
    let mut cursor = std::io::Cursor::new(bytes);
    let payload = read_frame(&mut cursor, limit)?;
    serde_json::from_slice(payload.as_slice()).context("decode Computer Use helper JSON frame")
}
