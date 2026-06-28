use std::fs;
use std::path::Path;

pub(in crate::services::project_run) const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
pub(in crate::services::project_run) const MAX_SOURCE_PROBE_BYTES: u64 = 512 * 1024;
pub(in crate::services::project_run) const MAX_CONFIG_PREVIEW_BYTES: u64 = 256 * 1024;

pub(in crate::services::project_run) fn read_to_string_limited(
    path: &Path,
    max_bytes: u64,
) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    if metadata.len() > max_bytes {
        return None;
    }
    fs::read_to_string(path).ok()
}
