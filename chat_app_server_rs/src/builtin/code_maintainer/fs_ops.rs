use super::utils::{ensure_path_inside_root, is_binary_buffer, sha256_bytes};
use crate::services::workspace_search::{
    search_text as search_workspace_text, TextSearchRequest, DEFAULT_MAX_VISITS,
};
use std::fs;
use std::path::{Path, PathBuf};

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
        let (path, size, hash, content) = self.read_file_raw(rel_path)?;
        let lines: Vec<String> = content
            .split('\n')
            .map(|line| line.trim_end_matches('\r').to_string())
            .collect();
        let total_lines = lines.len();
        let start = start_line.max(1);
        let end = end_line.min(total_lines.max(1));
        let slice: Vec<String> = if start > end || total_lines == 0 {
            Vec::new()
        } else {
            lines[start - 1..end]
                .iter()
                .enumerate()
                .map(|(idx, line)| {
                    if with_numbers {
                        format!("{}: {}", start + idx, line)
                    } else {
                        line.to_string()
                    }
                })
                .collect()
        };
        Ok((path, size, hash, start, end, total_lines, slice.join("\n")))
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
        let outcome = search_workspace_text(&TextSearchRequest {
            root,
            query: pattern.to_string(),
            max_results: max_results.unwrap_or(self.search_limit),
            max_file_bytes: self.max_file_bytes.max(0) as u64,
            max_visits: DEFAULT_MAX_VISITS,
            case_sensitive: true,
            whole_word: false,
        })?;
        Ok(outcome
            .entries
            .into_iter()
            .map(|entry| SearchResult {
                path: entry.relative_path,
                line: entry.line,
                text: entry.text,
            })
            .collect())
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
}
