// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Stable, transport-neutral contracts shared by the native clients and the
//! Local Agent Host. This crate contains no model, database, queue, or tool
//! execution implementation.

mod event;
mod ipc;
mod message;
mod run;
mod tool;

pub use event::*;
pub use ipc::*;
pub use message::*;
pub use run::*;
pub use tool::*;

pub const LOCAL_AGENT_PROTOCOL_VERSION: u32 = 1;
pub const MAX_BOUNDED_JSON_BYTES: usize = 64 * 1024;

pub(crate) fn require_identifier(field: &'static str, value: &str) -> Result<(), ProtocolError> {
    if value.trim().is_empty() {
        return Err(ProtocolError::EmptyIdentifier { field });
    }
    if value.len() > 512 {
        return Err(ProtocolError::IdentifierTooLong { field });
    }
    Ok(())
}

pub(crate) fn require_digest(field: &'static str, value: &str) -> Result<(), ProtocolError> {
    require_identifier(field, value)?;
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(ProtocolError::InvalidDigest { field });
    };
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ProtocolError::InvalidDigest { field });
    }
    Ok(())
}

pub(crate) fn require_bounded_json(
    field: &'static str,
    value: &serde_json::Value,
) -> Result<(), ProtocolError> {
    let length = serde_json::to_vec(value)
        .map_err(|_| ProtocolError::InvalidJson { field })?
        .len();
    if length > MAX_BOUNDED_JSON_BYTES {
        return Err(ProtocolError::PayloadTooLarge {
            field,
            bytes: length,
            maximum: MAX_BOUNDED_JSON_BYTES,
        });
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProtocolError {
    #[error("{field} must not be empty")]
    EmptyIdentifier { field: &'static str },
    #[error("{field} exceeds the identifier length limit")]
    IdentifierTooLong { field: &'static str },
    #[error("{field} must be a SHA-256 digest")]
    InvalidDigest { field: &'static str },
    #[error("{field} is not valid JSON")]
    InvalidJson { field: &'static str },
    #[error("{field} is {bytes} bytes; maximum is {maximum}")]
    PayloadTooLarge {
        field: &'static str,
        bytes: usize,
        maximum: usize,
    },
    #[error("invalid protocol state: {reason}")]
    InvalidState { reason: &'static str },
}
