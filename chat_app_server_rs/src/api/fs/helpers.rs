use serde_json::{Value, json};
use std::fs;
use std::io::{Seek, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use walkdir::WalkDir;
use zip::write::FileOptions;

use super::policy::normalize_path_for_compare;

const MAX_DOWNLOAD_FILES: usize = 5_000;
const MAX_DOWNLOAD_DEPTH: usize = 32;
const MAX_DOWNLOAD_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
const MAX_DOWNLOAD_SINGLE_FILE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Default)]
struct DownloadBudget {
    file_count: usize,
    total_bytes: u64,
}

impl DownloadBudget {
    fn register_file(&mut self, file_len: u64) -> Result<(), String> {
        self.file_count += 1;
        if self.file_count > MAX_DOWNLOAD_FILES {
            return Err(format!(
                "目录文件数过多，最多允许 {} 个文件打包下载",
                MAX_DOWNLOAD_FILES
            ));
        }
        if file_len > MAX_DOWNLOAD_SINGLE_FILE_BYTES {
            return Err(format!(
                "目录中存在超大文件，单文件最多允许 {} 字节",
                MAX_DOWNLOAD_SINGLE_FILE_BYTES
            ));
        }
        self.total_bytes = self.total_bytes.saturating_add(file_len);
        if self.total_bytes > MAX_DOWNLOAD_TOTAL_BYTES {
            return Err(format!(
                "目录内容过大，最多允许 {} 字节打包下载",
                MAX_DOWNLOAD_TOTAL_BYTES
            ));
        }
        Ok(())
    }
}

pub(super) fn read_dir_entries(
    path: &Path,
    navigation_root: &Path,
    include_files: bool,
) -> Result<Vec<Value>, String> {
    let mut out = Vec::new();
    let iter = fs::read_dir(path).map_err(|e| e.to_string())?;
    for entry in iter {
        let entry = match entry {
            Ok(v) => v,
            Err(_) => continue,
        };
        let entry_path = entry.path();
        let file_type = match entry.file_type() {
            Ok(value) => value,
            Err(_) => continue,
        };
        let canonical = if file_type.is_symlink() {
            match fs::canonicalize(&entry_path) {
                Ok(value) => value,
                Err(_) => continue,
            }
        } else {
            entry_path.clone()
        };
        if !path_is_within_root(canonical.as_path(), navigation_root) {
            continue;
        }
        let meta = match fs::metadata(&entry_path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let is_dir = meta.is_dir();
        if !is_dir && !include_files {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let p = entry_path.to_string_lossy().to_string();
        let size = if is_dir { None } else { Some(meta.len()) };
        let modified_at = meta.modified().ok().and_then(format_system_time);
        out.push(json!({
            "name": name,
            "path": p,
            "is_dir": is_dir,
            "size": size,
            "modified_at": modified_at
        }));
    }
    out.sort_by(|a, b| {
        let ad = a.get("is_dir").and_then(|v| v.as_bool()).unwrap_or(false);
        let bd = b.get("is_dir").and_then(|v| v.as_bool()).unwrap_or(false);
        if ad != bd {
            return bd.cmp(&ad);
        }
        let an = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let bn = b.get("name").and_then(|v| v.as_str()).unwrap_or("");
        an.to_lowercase().cmp(&bn.to_lowercase())
    });
    Ok(out)
}

pub(super) fn is_valid_entry_name(name: &str) -> bool {
    !(name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.contains('\0'))
}

pub(super) fn infer_download_name(path: &Path) -> String {
    path.file_name()
        .and_then(|v| v.to_str())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "download".to_string())
}

pub(super) fn zip_directory_to_temp_file(
    path: &Path,
    navigation_root: &Path,
) -> Result<PathBuf, String> {
    let temp_path =
        std::env::temp_dir().join(format!("chatos-fs-download-{}.zip", uuid::Uuid::new_v4()));
    let result = (|| -> Result<(), String> {
        let file = fs::File::create(&temp_path).map_err(|err| err.to_string())?;
        let _file = write_zip_directory(file, path, navigation_root)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result.map(|_| temp_path)
}

fn write_zip_directory<W>(writer: W, path: &Path, navigation_root: &Path) -> Result<W, String>
where
    W: Write + Seek,
{
    let root_name = infer_download_name(path);
    let mut zip = zip::ZipWriter::new(writer);
    let dir_options = FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755);
    let file_options = FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    let root_dir = format!("{}/", path_to_zip_name(Path::new(&root_name)));
    zip.add_directory(root_dir.clone(), dir_options)
        .map_err(|err| err.to_string())?;

    let mut budget = DownloadBudget::default();

    for entry in WalkDir::new(path).follow_links(false) {
        let entry = entry.map_err(|err| err.to_string())?;
        let current = entry.path();
        if current == path {
            continue;
        }
        if entry.depth() > MAX_DOWNLOAD_DEPTH {
            return Err(format!("目录层级过深，最多允许 {} 层", MAX_DOWNLOAD_DEPTH));
        }
        let relative = current.strip_prefix(path).map_err(|err| err.to_string())?;
        let relative_zip_path = path_to_zip_name(relative);
        if relative_zip_path.is_empty() {
            continue;
        }
        let zip_path = format!("{root_name}/{relative_zip_path}");
        if entry.file_type().is_symlink() {
            let canonical = match fs::canonicalize(current) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if !path_is_within_root(canonical.as_path(), navigation_root) {
                return Err("目录包含超出授权根目录的符号链接".to_string());
            }
            continue;
        }
        if entry.file_type().is_dir() {
            zip.add_directory(format!("{zip_path}/"), dir_options)
                .map_err(|err| err.to_string())?;
            continue;
        }
        if entry.file_type().is_file() {
            let metadata = fs::metadata(current).map_err(|err| err.to_string())?;
            let file_len = metadata.len();
            budget.register_file(file_len)?;
            zip.start_file(zip_path, file_options)
                .map_err(|err| err.to_string())?;
            let mut file = fs::File::open(current).map_err(|err| err.to_string())?;
            std::io::copy(&mut file, &mut zip).map_err(|err| err.to_string())?;
        }
    }

    zip.finish().map_err(|err| err.to_string())
}

fn path_to_zip_name(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

pub(super) fn format_system_time(time: SystemTime) -> Option<String> {
    let dt: chrono::DateTime<chrono::Utc> = time.into();
    Some(dt.to_rfc3339())
}

fn path_is_within_root(candidate: &Path, root: &Path) -> bool {
    let candidate_norm = normalize_path_for_compare(candidate);
    let root_norm = normalize_path_for_compare(root);

    if candidate_norm == root_norm {
        return true;
    }

    let prefix = format!("{root_norm}/");
    candidate_norm.starts_with(&prefix)
}

#[cfg(test)]
mod tests {
    use super::{
        DownloadBudget, MAX_DOWNLOAD_DEPTH, MAX_DOWNLOAD_FILES, MAX_DOWNLOAD_SINGLE_FILE_BYTES,
        MAX_DOWNLOAD_TOTAL_BYTES, zip_directory_to_temp_file,
    };
    use std::fs;
    use std::path::PathBuf;

    fn make_temp_dir(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("{}_{}", name, uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create temp dir");
        root
    }

    #[cfg(unix)]
    #[test]
    fn zip_directory_rejects_symlink_outside_navigation_root() {
        use std::os::unix::fs::symlink;

        let root = make_temp_dir("zip_dir_root");
        let outside = make_temp_dir("zip_dir_outside");
        let outside_file = outside.join("secret.txt");
        fs::write(&outside_file, "secret").expect("write outside file");
        symlink(&outside_file, root.join("secret-link")).expect("create symlink");

        let err = zip_directory_to_temp_file(root.as_path(), root.as_path())
            .expect_err("should reject zip");
        assert!(err.contains("符号链接"));

        fs::remove_dir_all(root).expect("cleanup root");
        fs::remove_dir_all(outside).expect("cleanup outside");
    }

    #[test]
    fn zip_directory_rejects_single_file_over_limit() {
        let root = make_temp_dir("zip_dir_big_file");
        let big_file = root.join("too-large.bin");
        let file = fs::File::create(&big_file).expect("create sparse file");
        file.set_len(MAX_DOWNLOAD_SINGLE_FILE_BYTES + 1)
            .expect("extend sparse file");

        let err = zip_directory_to_temp_file(root.as_path(), root.as_path())
            .expect_err("should reject oversized file");
        assert!(err.contains("超大文件"));

        fs::remove_dir_all(root).expect("cleanup root");
    }

    #[test]
    fn zip_directory_rejects_excessive_depth() {
        let root = make_temp_dir("zip_dir_too_deep");
        let mut current = root.clone();
        for index in 0..=MAX_DOWNLOAD_DEPTH {
            current = current.join(format!("level_{index}"));
        }
        fs::create_dir_all(&current).expect("create nested directories");
        fs::write(current.join("note.txt"), "deep").expect("write nested file");

        let err = zip_directory_to_temp_file(root.as_path(), root.as_path())
            .expect_err("should reject excessive depth");
        assert!(err.contains("目录层级过深"));

        fs::remove_dir_all(root).expect("cleanup root");
    }

    #[test]
    fn zip_directory_rejects_excessive_file_count() {
        let root = make_temp_dir("zip_dir_too_many_files");
        for index in 0..=MAX_DOWNLOAD_FILES {
            fs::write(root.join(format!("file_{index}.txt")), "x").expect("write file");
        }

        let err = zip_directory_to_temp_file(root.as_path(), root.as_path())
            .expect_err("should reject excessive file count");
        assert!(err.contains("目录文件数过多"));

        fs::remove_dir_all(root).expect("cleanup root");
    }

    #[test]
    fn download_budget_rejects_total_bytes_over_limit() {
        let mut budget = DownloadBudget::default();
        let file_count = (MAX_DOWNLOAD_TOTAL_BYTES / MAX_DOWNLOAD_SINGLE_FILE_BYTES) as usize;
        for _ in 0..file_count {
            budget
                .register_file(MAX_DOWNLOAD_SINGLE_FILE_BYTES)
                .expect("file should fit within single-file and total budget");
        }

        let err = budget
            .register_file(16)
            .expect_err("total bytes should exceed limit");
        assert!(err.contains("目录内容过大"));
    }
}
