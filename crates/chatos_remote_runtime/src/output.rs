// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fmt;
use std::io::Read;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundedReadError {
    Read(String),
    LimitExceeded {
        stream_label: String,
        actual_bytes: usize,
        limit_bytes: usize,
    },
}

impl BoundedReadError {
    pub fn with_read_context(&self, context: &str) -> String {
        match self {
            Self::Read(error) => format!("{context}: {error}"),
            Self::LimitExceeded { .. } => self.to_string(),
        }
    }
}

impl fmt::Display for BoundedReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => formatter.write_str(error),
            Self::LimitExceeded {
                stream_label,
                actual_bytes,
                limit_bytes,
            } => write!(
                formatter,
                "SSH {stream_label} exceeded output limit: {actual_bytes} bytes > {limit_bytes} bytes"
            ),
        }
    }
}

impl std::error::Error for BoundedReadError {}

pub fn read_stream_limited<R: Read>(
    reader: &mut R,
    stream_label: &str,
    limit_bytes: usize,
) -> Result<Vec<u8>, BoundedReadError> {
    let mut output = Vec::new();
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| BoundedReadError::Read(error.to_string()))?;
        if read == 0 {
            return Ok(output);
        }
        let next_len = output.len().saturating_add(read);
        if next_len > limit_bytes {
            return Err(BoundedReadError::LimitExceeded {
                stream_label: stream_label.to_string(),
                actual_bytes: next_len,
                limit_bytes,
            });
        }
        output.extend_from_slice(&buffer[..read]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_read_accepts_boundary_size() {
        let mut input = &b"1234"[..];
        assert_eq!(
            read_stream_limited(&mut input, "stdout", 4).expect("bounded output"),
            b"1234"
        );
    }

    #[test]
    fn bounded_read_rejects_oversized_output() {
        let mut input = &b"12345"[..];
        let error =
            read_stream_limited(&mut input, "stderr", 4).expect_err("oversized output should fail");
        assert_eq!(
            error.to_string(),
            "SSH stderr exceeded output limit: 5 bytes > 4 bytes"
        );
    }
}
