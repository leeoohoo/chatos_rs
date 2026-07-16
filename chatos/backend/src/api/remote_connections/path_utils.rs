// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub(super) fn shell_quote(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('\'');
    for ch in value.chars() {
        if ch == '\'' {
            escaped.push_str("'\\''");
        } else {
            escaped.push(ch);
        }
    }
    escaped.push('\'');
    escaped
}

pub(super) use chatos_remote_runtime::{
    join_remote_path, normalize_remote_path, remote_parent_path,
};

pub(super) fn input_triggers_busy(data: &str) -> bool {
    if data.is_empty() {
        return false;
    }
    if data.contains('\r') || data.contains('\n') {
        return true;
    }
    data.as_bytes()
        .iter()
        .any(|b| matches!(*b, 0x03 | 0x04 | 0x1A))
}
