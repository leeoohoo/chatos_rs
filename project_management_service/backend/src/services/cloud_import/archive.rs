// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs::{self, File};
use std::io::{self, Cursor};
use std::path::Path;

use walkdir::WalkDir;
use zip::ZipArchive;

use super::archive_policy::{classify_archive_path, ArchivePathAction};

pub(super) fn unpack_zip_safely(
    zip_bytes: Vec<u8>,
    target_dir: &Path,
    max_files: usize,
    max_unpacked_bytes: u64,
) -> Result<(), String> {
    let cursor = Cursor::new(zip_bytes);
    let mut archive = ZipArchive::new(cursor).map_err(|err| format!("open zip failed: {err}"))?;
    let mut file_count = 0usize;
    let mut unpacked_bytes = 0u64;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|err| format!("read zip entry failed: {err}"))?;
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| format!("zip entry has unsafe path: {}", entry.name()))?
            .to_path_buf();

        match classify_archive_path(enclosed.as_path()) {
            ArchivePathAction::Ignore => continue,
            ArchivePathAction::Include => {}
        }

        if entry.is_dir() {
            fs::create_dir_all(target_dir.join(enclosed)).map_err(|err| err.to_string())?;
            continue;
        }

        file_count += 1;
        if file_count > max_files {
            return Err(format!(
                "zip contains too many importable files: {file_count} > {max_files}"
            ));
        }
        unpacked_bytes = unpacked_bytes.saturating_add(entry.size());
        if unpacked_bytes > max_unpacked_bytes {
            return Err(format!(
                "zip importable content is too large: {unpacked_bytes} bytes > {max_unpacked_bytes} bytes"
            ));
        }

        let out_path = target_dir.join(enclosed);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let mut out_file = File::create(out_path).map_err(|err| err.to_string())?;
        io::copy(&mut entry, &mut out_file).map_err(|err| err.to_string())?;
    }
    Ok(())
}

pub(super) fn flatten_single_project_directory(root: &Path) -> Result<(), String> {
    let entries = fs::read_dir(root)
        .map_err(|err| err.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())?;
    let [only] = entries.as_slice() else {
        return Ok(());
    };
    if !only.file_type().map_err(|err| err.to_string())?.is_dir() {
        return Ok(());
    }
    let nested_root = only.path();
    let nested_entries = fs::read_dir(nested_root.as_path())
        .map_err(|err| err.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())?;
    for entry in nested_entries {
        let destination = root.join(entry.file_name());
        if destination.exists() {
            return Err(format!(
                "flatten ZIP project root failed because {} already exists",
                destination.display()
            ));
        }
        fs::rename(entry.path(), destination).map_err(|err| err.to_string())?;
    }
    fs::remove_dir(nested_root).map_err(|err| err.to_string())
}

pub(super) fn has_importable_files(path: &Path) -> bool {
    WalkDir::new(path)
        .into_iter()
        .filter_map(Result::ok)
        .any(|entry| entry.file_type().is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn zip_with_files(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = ZipWriter::new(&mut cursor);
            for (path, content) in files {
                writer
                    .start_file(path, SimpleFileOptions::default())
                    .expect("start zip file");
                writer.write_all(content).expect("write zip file");
            }
            writer.finish().expect("finish zip");
        }
        cursor.into_inner()
    }

    #[test]
    fn ignores_dependencies_build_outputs_and_caches() {
        let target = std::env::temp_dir().join(format!(
            "chatos-cloud-archive-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&target).expect("create test target");
        let zip = zip_with_files(&[
            ("repo/src/main.rs", b"fn main() {}"),
            ("repo/Cargo.lock", b"lock"),
            ("repo/src/bin/worker.rs", b"fn main() {}"),
            ("repo/target/debug/app", b"binary"),
            ("repo/node_modules/pkg/index.js", b"dependency"),
            ("repo/__pycache__/main.pyc", b"cache"),
            ("repo/build/output.jar", b"compiled"),
        ]);

        unpack_zip_safely(zip, &target, 3, 1024).expect("unpack filtered zip");

        assert!(target.join("repo/src/main.rs").is_file());
        assert!(target.join("repo/Cargo.lock").is_file());
        assert!(target.join("repo/src/bin/worker.rs").is_file());
        assert!(!target.join("repo/target").exists());
        assert!(!target.join("repo/node_modules").exists());
        assert!(!target.join("repo/__pycache__").exists());
        assert!(!target.join("repo/build").exists());
        let _ = fs::remove_dir_all(target);
    }

    #[test]
    fn ignores_embedded_git_metadata_but_keeps_source_files() {
        let target = std::env::temp_dir().join(format!(
            "chatos-cloud-archive-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&target).expect("create test target");
        let zip = zip_with_files(&[
            ("repo/.git/config", b"ignored"),
            ("repo/src/main.rs", b"fn main() {}"),
        ]);

        unpack_zip_safely(zip, &target, 10, 1024).expect("ignore .git");

        assert!(!target.join("repo/.git").exists());
        assert!(target.join("repo/src/main.rs").is_file());
        let _ = fs::remove_dir_all(target);
    }

    #[test]
    fn flattens_the_common_single_project_directory_wrapper() {
        let target = std::env::temp_dir().join(format!(
            "chatos-cloud-archive-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(target.join("project/src")).expect("create project root");
        fs::write(target.join("project/Cargo.toml"), "[package]").expect("write manifest");
        fs::write(target.join("project/src/main.rs"), "fn main() {}").expect("write source");

        flatten_single_project_directory(&target).expect("flatten project directory");

        assert!(target.join("Cargo.toml").is_file());
        assert!(target.join("src/main.rs").is_file());
        assert!(!target.join("project").exists());
        let _ = fs::remove_dir_all(target);
    }
}
