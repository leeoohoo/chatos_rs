use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

pub(crate) const CODE_NAV_MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

pub(crate) fn read_code_nav_file_to_string(path: &Path) -> Result<String, String> {
    let file = open_code_nav_file(path)?;
    let mut bytes = Vec::new();
    let mut reader = BufReader::new(file).take(CODE_NAV_MAX_FILE_BYTES.saturating_add(1));
    reader
        .read_to_end(&mut bytes)
        .map_err(|err| format!("read code-nav file failed: {err}"))?;
    ensure_code_nav_file_within_limit(path, bytes.len() as u64)?;
    String::from_utf8(bytes).map_err(|err| format!("read code-nav file as utf-8 failed: {err}"))
}

pub(crate) fn read_code_nav_line_preview(
    path: &Path,
    line: usize,
    max_chars: usize,
) -> Result<String, String> {
    if line == 0 {
        return Ok(String::new());
    }

    let file = open_code_nav_file(path)?;
    let mut reader = BufReader::new(file);
    for current_line in 1..=line {
        let mut bytes = Vec::new();
        let read = reader
            .read_until(b'\n', &mut bytes)
            .map_err(|err| format!("read code-nav line failed: {err}"))?;
        if read == 0 {
            return Ok(String::new());
        }
        if current_line == line {
            while matches!(bytes.last(), Some(b'\n' | b'\r')) {
                bytes.pop();
            }
            let line = String::from_utf8_lossy(bytes.as_slice());
            return Ok(truncate_preview(line.as_ref(), max_chars));
        }
    }

    Ok(String::new())
}

pub(crate) fn truncate_preview(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn open_code_nav_file(path: &Path) -> Result<File, String> {
    let file = File::open(path).map_err(|err| err.to_string())?;
    if let Ok(metadata) = file.metadata() {
        ensure_code_nav_file_within_limit(path, metadata.len())?;
    }
    Ok(file)
}

fn ensure_code_nav_file_within_limit(path: &Path, actual_bytes: u64) -> Result<(), String> {
    if actual_bytes > CODE_NAV_MAX_FILE_BYTES {
        return Err(format!(
            "code-nav file exceeds limit: {} bytes > {} bytes ({})",
            actual_bytes,
            CODE_NAV_MAX_FILE_BYTES,
            path.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_code_nav_file_within_limit, read_code_nav_line_preview, truncate_preview,
        CODE_NAV_MAX_FILE_BYTES,
    };
    use std::fs;
    use std::path::PathBuf;

    fn make_temp_file(content: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "code_nav_file_limits_test_{}.txt",
            uuid::Uuid::new_v4()
        ));
        fs::write(&path, content).expect("write temp file");
        path
    }

    #[test]
    fn code_nav_file_limit_accepts_boundary_size() {
        assert!(ensure_code_nav_file_within_limit(
            PathBuf::from("source.rs").as_path(),
            CODE_NAV_MAX_FILE_BYTES
        )
        .is_ok());
    }

    #[test]
    fn code_nav_file_limit_rejects_oversized_file() {
        let err = ensure_code_nav_file_within_limit(
            PathBuf::from("source.rs").as_path(),
            CODE_NAV_MAX_FILE_BYTES + 1,
        )
        .expect_err("oversized file should fail");

        assert!(err.contains("code-nav file exceeds limit"));
    }

    #[test]
    fn line_preview_reads_requested_line_only() {
        let path = make_temp_file("first\nsecond line\nthird\n");
        let preview = read_code_nav_line_preview(&path, 2, 6).expect("read line preview");

        assert_eq!(preview, "second");

        fs::remove_file(path).ok();
    }

    #[test]
    fn preview_truncates_on_char_boundary() {
        assert_eq!(truncate_preview("你好世界", 2), "你好");
    }
}
