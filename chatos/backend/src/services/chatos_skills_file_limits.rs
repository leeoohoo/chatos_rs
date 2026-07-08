// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

pub(crate) const MAX_PLUGIN_MARKDOWN_BYTES: u64 = 256 * 1024;
pub(crate) const MAX_PLUGIN_JSON_BYTES: u64 = 256 * 1024;
pub(crate) const MAX_PLUGIN_MARKETPLACE_BYTES: u64 = 1024 * 1024;
pub(crate) const MAX_PLUGIN_SCAN_ENTRIES: usize = 20_000;

pub(crate) fn read_plugin_text_limited(path: &Path, max_bytes: u64) -> Result<String, String> {
    let file = File::open(path).map_err(|err| err.to_string())?;
    if let Ok(metadata) = file.metadata() {
        ensure_plugin_file_within_limit(path, metadata.len(), max_bytes)?;
    }

    let mut bytes = Vec::new();
    let mut reader = BufReader::new(file).take(max_bytes.saturating_add(1));
    reader
        .read_to_end(&mut bytes)
        .map_err(|err| format!("read plugin file failed: {err}"))?;
    ensure_plugin_file_within_limit(path, bytes.len() as u64, max_bytes)?;
    String::from_utf8(bytes).map_err(|err| format!("read plugin file as utf-8 failed: {err}"))
}

fn ensure_plugin_file_within_limit(
    path: &Path,
    actual_bytes: u64,
    max_bytes: u64,
) -> Result<(), String> {
    if actual_bytes > max_bytes {
        return Err(format!(
            "plugin file exceeds limit: {} bytes > {} bytes ({})",
            actual_bytes,
            max_bytes,
            path.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ensure_plugin_file_within_limit, MAX_PLUGIN_MARKDOWN_BYTES};
    use std::path::PathBuf;

    #[test]
    fn plugin_file_limit_accepts_boundary_size() {
        assert!(ensure_plugin_file_within_limit(
            PathBuf::from("SKILL.md").as_path(),
            MAX_PLUGIN_MARKDOWN_BYTES,
            MAX_PLUGIN_MARKDOWN_BYTES,
        )
        .is_ok());
    }

    #[test]
    fn plugin_file_limit_rejects_oversized_file() {
        let err = ensure_plugin_file_within_limit(
            PathBuf::from("SKILL.md").as_path(),
            MAX_PLUGIN_MARKDOWN_BYTES + 1,
            MAX_PLUGIN_MARKDOWN_BYTES,
        )
        .expect_err("oversized plugin file should fail");

        assert!(err.contains("plugin file exceeds limit"));
    }
}
