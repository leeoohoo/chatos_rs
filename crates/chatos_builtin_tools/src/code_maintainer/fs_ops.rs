// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::bundled_tools::bundled_tool_path;

use super::utils::{ensure_path_inside_root, is_binary_buffer, sha256_bytes};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const SEARCH_DEADLINE: Duration = Duration::from_secs(3);
const SEARCH_MAX_VISITS: usize = 20_000;

#[derive(Clone, Debug)]
pub struct FsOps {
    root: PathBuf,
    allow_writes: bool,
    max_file_bytes: i64,
    max_write_bytes: i64,
    search_limit: usize,
}

#[derive(Debug, serde::Serialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub r#type: String,
    pub size: u64,
    pub mtime_ms: u128,
}

#[derive(Debug, serde::Serialize)]
pub struct DeleteResult {
    pub path: String,
    pub deleted: bool,
}

impl FsOps {
    pub fn new(
        root: PathBuf,
        allow_writes: bool,
        max_file_bytes: i64,
        max_write_bytes: i64,
        search_limit: usize,
    ) -> Self {
        Self {
            root,
            allow_writes,
            max_file_bytes,
            max_write_bytes,
            search_limit,
        }
    }

    pub fn resolve_path(&self, rel_path: &str) -> Result<PathBuf, String> {
        let normalized = rel_path.replace('\\', "/");
        let target = Path::new(&normalized);
        ensure_path_inside_root(&self.root, target)
    }

    pub fn read_file_raw(&self, rel_path: &str) -> Result<(String, u64, String, String), String> {
        let target = self.resolve_path(rel_path)?;
        let metadata = fs::metadata(&target).map_err(|err| err.to_string())?;
        if !metadata.is_file() {
            return Err("Target is not a file.".to_string());
        }
        if metadata.len() as i64 > self.max_file_bytes {
            return Err(format!("File too large ({} bytes).", metadata.len()));
        }
        let buffer = fs::read(&target).map_err(|err| err.to_string())?;
        if is_binary_buffer(&buffer) {
            return Err("Binary file not supported.".to_string());
        }
        let content = String::from_utf8_lossy(&buffer).to_string();
        let hash = sha256_bytes(&buffer);
        Ok((rel_path.to_string(), metadata.len(), hash, content))
    }

    pub fn read_file_range(
        &self,
        rel_path: &str,
        start_line: usize,
        end_line: usize,
        with_numbers: bool,
    ) -> Result<(String, u64, String, usize, usize, usize, String), String> {
        let target = self.resolve_path(rel_path)?;
        let metadata = fs::metadata(&target).map_err(|err| err.to_string())?;
        if !metadata.is_file() {
            return Err("Target is not a file.".to_string());
        }
        if metadata.len() as i64 > self.max_file_bytes {
            return Err(format!("File too large ({} bytes).", metadata.len()));
        }

        let start = start_line.max(1);
        let mut reader = BufReader::new(fs::File::open(&target).map_err(|err| err.to_string())?);
        let mut hasher = Sha256::new();
        let mut selected = Vec::new();
        let mut total_lines = 0usize;
        let mut inspected_bytes = 0usize;
        let mut saw_bytes = false;
        let mut last_byte_was_newline = false;

        loop {
            let mut buffer = Vec::new();
            let bytes_read = reader
                .read_until(b'\n', &mut buffer)
                .map_err(|err| err.to_string())?;
            if bytes_read == 0 {
                break;
            }

            saw_bytes = true;
            last_byte_was_newline = buffer.last() == Some(&b'\n');
            inspect_binary_prefix(&buffer, &mut inspected_bytes)?;
            hasher.update(&buffer);
            total_lines += 1;

            if start <= end_line && total_lines >= start && total_lines <= end_line {
                let line = normalize_range_line(&buffer);
                selected.push(if with_numbers {
                    format!("{}: {}", total_lines, line)
                } else {
                    line
                });
            }
        }

        if !saw_bytes || last_byte_was_newline {
            total_lines += 1;
            if start <= end_line && total_lines >= start && total_lines <= end_line {
                selected.push(if with_numbers {
                    format!("{}: ", total_lines)
                } else {
                    String::new()
                });
            }
        };
        let end = end_line.min(total_lines.max(1));
        let hash = hex::encode(hasher.finalize());

        Ok((
            rel_path.to_string(),
            metadata.len(),
            hash,
            start,
            end,
            total_lines,
            selected.join("\n"),
        ))
    }

    pub fn list_dir(&self, rel_path: &str, max_entries: usize) -> Result<Vec<FileEntry>, String> {
        let target = self.resolve_path(rel_path)?;
        let mut entries = Vec::new();
        let read_dir = fs::read_dir(&target).map_err(|err| err.to_string())?;
        for entry in read_dir.take(max_entries) {
            let entry = entry.map_err(|err| err.to_string())?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(|err| err.to_string())?;
            let file_type = metadata.file_type();
            let kind = if file_type.is_dir() {
                "dir"
            } else if file_type.is_symlink() {
                "symlink"
            } else {
                "file"
            };
            let mtime_ms = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis())
                .unwrap_or(0);
            entries.push(FileEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                path: pathdiff::diff_paths(&path, &self.root)
                    .unwrap_or_else(|| path.to_path_buf())
                    .to_string_lossy()
                    .to_string(),
                r#type: kind.to_string(),
                size: metadata.len(),
                mtime_ms,
            });
        }
        Ok(entries)
    }

    pub fn search_text(
        &self,
        pattern: &str,
        rel_path: &str,
        max_results: Option<usize>,
    ) -> Result<Vec<SearchResult>, String> {
        let root = self.resolve_path(rel_path)?;
        let limit = max_results.unwrap_or(self.search_limit);
        let max_file_bytes = self.max_file_bytes.max(0) as u64;
        if root.is_file() {
            return search_text_in_file(
                root.as_path(),
                self.root.as_path(),
                pattern,
                limit,
                max_file_bytes,
                Instant::now(),
            );
        }

        search_text_in_dir(
            root.as_path(),
            self.root.as_path(),
            pattern,
            limit,
            max_file_bytes,
        )
    }

    pub fn write_file(&self, rel_path: &str, content: &str) -> Result<WriteResult, String> {
        if !self.allow_writes {
            return Err("Writes are disabled.".to_string());
        }
        let target = self.resolve_path(rel_path)?;
        let buffer = content.as_bytes();
        if buffer.len() as i64 > self.max_write_bytes {
            return Err("Write exceeds max-write-bytes limit.".to_string());
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        fs::write(&target, buffer).map_err(|err| err.to_string())?;
        Ok(WriteResult {
            bytes: buffer.len() as i64,
            sha256: sha256_bytes(buffer),
            path: rel_path.to_string(),
        })
    }

    pub fn append_file(&self, rel_path: &str, content: &str) -> Result<WriteResult, String> {
        if !self.allow_writes {
            return Err("Writes are disabled.".to_string());
        }
        let target = self.resolve_path(rel_path)?;
        let buffer = content.as_bytes();
        if buffer.len() as i64 > self.max_write_bytes {
            return Err("Write exceeds max-write-bytes limit.".to_string());
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&target)
            .map_err(|err| err.to_string())?;
        file.write_all(buffer).map_err(|err| err.to_string())?;
        Ok(WriteResult {
            bytes: buffer.len() as i64,
            sha256: sha256_bytes(buffer),
            path: rel_path.to_string(),
        })
    }

    pub fn delete_path(&self, rel_path: &str) -> Result<DeleteResult, String> {
        if !self.allow_writes {
            return Err("Writes are disabled.".to_string());
        }
        let target = self.resolve_path(rel_path)?;
        if target.is_dir() {
            fs::remove_dir_all(&target).map_err(|err| err.to_string())?;
            return Ok(DeleteResult {
                path: rel_path.to_string(),
                deleted: true,
            });
        }

        if let Ok(meta) = fs::symlink_metadata(&target) {
            if meta.file_type().is_symlink() || meta.is_file() {
                fs::remove_file(&target).map_err(|err| err.to_string())?;
                return Ok(DeleteResult {
                    path: rel_path.to_string(),
                    deleted: true,
                });
            }
            return Err("Target path is not a regular file or directory.".to_string());
        }

        Ok(DeleteResult {
            path: rel_path.to_string(),
            deleted: false,
        })
    }
}

#[derive(Debug, serde::Serialize)]
pub struct SearchResult {
    pub path: String,
    pub line: usize,
    pub text: String,
}

#[derive(Debug, serde::Serialize)]
pub struct WriteResult {
    pub bytes: i64,
    pub sha256: String,
    pub path: String,
}

fn inspect_binary_prefix(buffer: &[u8], inspected_bytes: &mut usize) -> Result<(), String> {
    if *inspected_bytes >= 8000 {
        return Ok(());
    }

    let remaining = 8000usize.saturating_sub(*inspected_bytes);
    let sample_len = buffer.len().min(remaining);
    if buffer.iter().take(sample_len).any(|byte| *byte == 0) {
        return Err("Binary file not supported.".to_string());
    }
    *inspected_bytes += sample_len;
    Ok(())
}

fn normalize_range_line(buffer: &[u8]) -> String {
    let mut end = buffer.len();
    if end > 0 && buffer[end - 1] == b'\n' {
        end -= 1;
    }
    if end > 0 && buffer[end - 1] == b'\r' {
        end -= 1;
    }
    String::from_utf8_lossy(&buffer[..end]).to_string()
}

fn search_text_in_file(
    file_path: &Path,
    workspace_root: &Path,
    pattern: &str,
    max_results: usize,
    max_file_bytes: u64,
    started_at: Instant,
) -> Result<Vec<SearchResult>, String> {
    let query = pattern.trim();
    if query.is_empty() {
        return Err("搜索关键字不能为空".to_string());
    }

    if max_file_bytes > 0 {
        let metadata = fs::metadata(file_path).map_err(|err| err.to_string())?;
        if metadata.len() > max_file_bytes {
            return Err(format!("File too large ({} bytes).", metadata.len()));
        }
    }
    let buffer = fs::read(file_path).map_err(|err| err.to_string())?;
    if is_binary_buffer(&buffer) {
        return Err("Binary file not supported.".to_string());
    }

    let content = std::str::from_utf8(&buffer).map_err(|err| err.to_string())?;
    let relative_path = pathdiff::diff_paths(file_path, workspace_root)
        .unwrap_or_else(|| file_path.to_path_buf())
        .to_string_lossy()
        .to_string();
    let mut entries = Vec::new();
    let limit = max_results.clamp(1, 500);

    for (index, line) in content.split('\n').enumerate() {
        if index % 128 == 0 {
            ensure_search_budget(started_at, 0)?;
        }
        if entries.len() >= limit {
            break;
        }
        let normalized = line.trim_end_matches('\r');
        if !normalized.contains(query) {
            continue;
        }
        entries.push(SearchResult {
            line: index + 1,
            path: relative_path.clone(),
            text: truncate_search_text(normalized),
        });
    }

    Ok(entries)
}

fn search_text_in_dir(
    root: &Path,
    workspace_root: &Path,
    pattern: &str,
    max_results: usize,
    max_file_bytes: u64,
) -> Result<Vec<SearchResult>, String> {
    let query = pattern.trim();
    if query.is_empty() {
        return Err("搜索关键字不能为空".to_string());
    }

    let limit = max_results.clamp(1, 500);
    if let Ok(results) =
        search_text_in_dir_with_rg(root, workspace_root, query, limit, max_file_bytes)
    {
        return Ok(results);
    }

    let mut entries = Vec::new();
    let started_at = Instant::now();
    let mut visited_entries = 0usize;
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        visited_entries = visited_entries.saturating_add(1);
        ensure_search_budget(started_at, visited_entries)?;

        if entries.len() >= limit {
            break;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if max_file_bytes > 0 && metadata.len() > max_file_bytes {
            continue;
        }
        let remaining = limit.saturating_sub(entries.len());
        let mut found = match search_text_in_file(
            path,
            workspace_root,
            query,
            remaining,
            max_file_bytes,
            started_at,
        ) {
            Ok(value) => value,
            Err(_) => continue,
        };
        entries.append(&mut found);
    }
    Ok(entries)
}

fn search_text_in_dir_with_rg(
    root: &Path,
    workspace_root: &Path,
    query: &str,
    limit: usize,
    max_file_bytes: u64,
) -> Result<Vec<SearchResult>, String> {
    let rg_path = bundled_tool_path("rg").unwrap_or_else(|| PathBuf::from("rg"));
    let search_path =
        pathdiff::diff_paths(root, workspace_root).unwrap_or_else(|| root.to_path_buf());
    let search_path = if search_path.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        search_path
    };

    let mut command = Command::new(rg_path);
    command
        .current_dir(workspace_root)
        .arg("--json")
        .arg("--fixed-strings")
        .arg("--hidden")
        .arg("--glob")
        .arg("!.git/**")
        .arg("--max-count")
        .arg(limit.to_string())
        .arg("--no-messages");
    if max_file_bytes > 0 {
        command
            .arg("--max-filesize")
            .arg(max_file_bytes.to_string());
    }
    command
        .arg("--")
        .arg(query)
        .arg(search_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    let mut child = command.spawn().map_err(|err| err.to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "failed to capture rg stdout".to_string())?;
    let reader = BufReader::new(stdout);
    let mut entries = Vec::new();
    let mut stopped_at_limit = false;

    for line in reader.lines() {
        let line = line.map_err(|err| err.to_string())?;
        if let Some(entry) = parse_rg_match_line(line.as_str(), workspace_root) {
            entries.push(entry);
            if entries.len() >= limit {
                stopped_at_limit = true;
                let _ = child.kill();
                break;
            }
        }
    }

    let status = child.wait().map_err(|err| err.to_string())?;
    if stopped_at_limit || status.success() || status.code() == Some(1) {
        return Ok(entries);
    }

    Err(format!("rg exited with status {status}"))
}

fn ensure_search_budget(started_at: Instant, visited_entries: usize) -> Result<(), String> {
    if visited_entries > SEARCH_MAX_VISITS {
        return Err(format!(
            "search_text scan exceeded {SEARCH_MAX_VISITS} entries"
        ));
    }
    if started_at.elapsed() >= SEARCH_DEADLINE {
        return Err(format!("search_text scan exceeded {:?}", SEARCH_DEADLINE));
    }
    Ok(())
}

fn parse_rg_match_line(line: &str, workspace_root: &Path) -> Option<SearchResult> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    if value.get("type").and_then(|value| value.as_str()) != Some("match") {
        return None;
    }

    let data = value.get("data")?;
    let raw_path = data
        .get("path")
        .and_then(|value| value.get("text"))
        .and_then(|value| value.as_str())?;
    let path = normalize_rg_result_path(raw_path, workspace_root);
    let line = data
        .get("line_number")
        .and_then(|value| value.as_u64())
        .map(|value| value as usize)?;
    let text = data
        .get("lines")
        .and_then(|value| value.get("text"))
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .trim_end_matches(['\r', '\n'])
        .to_string();

    Some(SearchResult {
        path,
        line,
        text: truncate_search_text(text.as_str()),
    })
}

fn truncate_search_text(value: &str) -> String {
    match value.char_indices().nth(400) {
        Some((boundary, _)) => value[..boundary].to_string(),
        None => value.to_string(),
    }
}

fn normalize_rg_result_path(raw_path: &str, workspace_root: &Path) -> String {
    let path = PathBuf::from(raw_path);
    if path.is_absolute() {
        pathdiff::diff_paths(path.as_path(), workspace_root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string()
    } else {
        raw_path.replace('\\', "/")
    }
}

#[cfg(test)]
mod tests {
    use super::FsOps;
    use std::fs;
    use std::path::PathBuf;

    fn make_temp_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "code_maintainer_fs_ops_test_{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).expect("create temp root");
        root
    }

    #[test]
    fn delete_file_is_idempotent_and_removed_from_list_dir() {
        let root = make_temp_root();
        let file_path = root.join("a.txt");
        fs::write(&file_path, "hello").expect("write file");

        let fs_ops = FsOps::new(root.clone(), true, 1024 * 1024, 1024 * 1024, 100);

        let first = fs_ops.delete_path("a.txt").expect("first delete");
        assert!(first.deleted);

        let entries = fs_ops.list_dir(".", 100).expect("list dir after delete");
        assert!(entries.iter().all(|entry| entry.name != "a.txt"));

        let second = fs_ops.delete_path("a.txt").expect("second delete");
        assert!(!second.deleted);

        fs::remove_dir_all(&root).expect("cleanup temp root");
    }

    #[test]
    fn delete_path_accepts_backslash_separator() {
        let root = make_temp_root();
        let nested = root.join("nested");
        fs::create_dir_all(&nested).expect("create nested dir");
        let file_path = nested.join("b.txt");
        fs::write(&file_path, "hello").expect("write nested file");

        let fs_ops = FsOps::new(root.clone(), true, 1024 * 1024, 1024 * 1024, 100);
        let deleted = fs_ops
            .delete_path("nested\\b.txt")
            .expect("delete with backslash path");
        assert!(deleted.deleted);
        assert!(!file_path.exists());

        fs::remove_dir_all(&root).expect("cleanup temp root");
    }

    #[test]
    fn search_text_accepts_file_path() {
        let root = make_temp_root();
        let file_path = root.join("notes.txt");
        fs::write(&file_path, "alpha\nbeta alias\ngamma alias\n").expect("write search file");

        let fs_ops = FsOps::new(root.clone(), true, 1024 * 1024, 1024 * 1024, 100);
        let results = fs_ops
            .search_text("alias", "notes.txt", Some(10))
            .expect("search file path");

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|entry| entry.path == "notes.txt"));
        assert_eq!(results[0].line, 2);

        fs::remove_dir_all(&root).expect("cleanup temp root");
    }

    #[test]
    fn read_file_range_streams_requested_lines_and_preserves_metadata() {
        let root = make_temp_root();
        let file_path = root.join("notes.txt");
        fs::write(&file_path, "line1\nline2\nline3\n").expect("write range file");

        let fs_ops = FsOps::new(root.clone(), true, 1024 * 1024, 1024 * 1024, 100);
        let (_raw_path, raw_size, raw_hash, _content) = fs_ops
            .read_file_raw("notes.txt")
            .expect("read raw for hash");
        let (path, size, hash, start, end, total, content) = fs_ops
            .read_file_range("notes.txt", 2, 4, true)
            .expect("read file range");

        assert_eq!(path, "notes.txt");
        assert_eq!(size, raw_size);
        assert_eq!(hash, raw_hash);
        assert_eq!(start, 2);
        assert_eq!(end, 4);
        assert_eq!(total, 4);
        assert_eq!(content, "2: line2\n3: line3\n4: ");

        fs::remove_dir_all(&root).expect("cleanup temp root");
    }

    #[test]
    fn search_text_file_path_respects_max_file_bytes() {
        let root = make_temp_root();
        let file_path = root.join("large.txt");
        fs::write(&file_path, "alias alias\n").expect("write search file");

        let fs_ops = FsOps::new(root.clone(), true, 4, 1024 * 1024, 100);
        let err = fs_ops
            .search_text("alias", "large.txt", Some(10))
            .expect_err("large file search should fail");

        assert!(err.contains("File too large"));
        fs::remove_dir_all(&root).expect("cleanup temp root");
    }

    #[test]
    fn search_text_truncates_long_result_lines_safely() {
        let root = make_temp_root();
        let file_path = root.join("notes.txt");
        let long_line = format!("{}alias", "页".repeat(450));
        fs::write(&file_path, format!("{long_line}\n")).expect("write search file");

        let fs_ops = FsOps::new(root.clone(), true, 1024 * 1024, 1024 * 1024, 100);
        let results = fs_ops
            .search_text("alias", "notes.txt", Some(10))
            .expect("search file path");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].text.chars().count(), 400);
        assert!(results[0].text.chars().all(|ch| ch == '页'));

        fs::remove_dir_all(&root).expect("cleanup temp root");
    }
}
