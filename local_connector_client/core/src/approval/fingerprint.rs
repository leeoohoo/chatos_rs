// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sha2::{Digest, Sha256};

pub(crate) fn normalized_command(command: &str, args: &[String]) -> String {
    let mut parts = Vec::with_capacity(args.len() + 1);
    parts.push(command.trim().to_string());
    parts.extend(args.iter().map(|arg| arg.trim().to_string()));
    parts
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn command_fingerprint(command: &str, args: &[String], cwd: &str) -> String {
    let payload = serde_json::json!({
        "command": normalized_command(command, args),
        "cwd": cwd.trim(),
    });
    let mut hasher = Sha256::new();
    hasher.update(payload.to_string().as_bytes());
    hex::encode(hasher.finalize())
}
