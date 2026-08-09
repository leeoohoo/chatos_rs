// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub(super) fn is_ssh_would_block(err: &ssh2::Error) -> bool {
    matches!(err.code(), ssh2::ErrorCode::Session(code) if code == -37)
}

pub(super) fn is_io_would_block(err: &std::io::Error) -> bool {
    matches!(err.kind(), std::io::ErrorKind::WouldBlock)
}
