// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::services::code_nav::languages::rust::RustCodeNavProvider;
use crate::services::code_nav::manager::CodeNavManager;
use crate::services::code_nav::symbol_index::invalidate_project_symbol_indexes_for_path;
use crate::services::code_nav::types::NavPositionRequest;
use std::ffi::CString;
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        // Keep socket paths within sockaddr_un's limit on macOS as well.
        let root = Path::new("/tmp").join(format!("cws-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join(".chatos/cache/code_nav")).unwrap();
        Self(fs::canonicalize(root).unwrap())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn make_fifo(path: &Path) {
    let name = CString::new(path.as_os_str().as_bytes()).unwrap();
    // SAFETY: name is a live NUL-terminated path to a fresh test fixture.
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    assert!(fs::symlink_metadata(path).unwrap().file_type().is_fifo());
}

#[test]
fn cache_read_open_rejects_special_replacements() {
    for kind in ["fifo", "directory", "socket"] {
        let fixture = Fixture::new();
        let relative = "code_nav/index.json";
        let root = fixture.0.to_str().unwrap();
        let path = project_cache_file_path(root, relative).unwrap();
        let content = br#"{"cache":"original"}"#;
        fs::write(&path, content).unwrap();
        // Reproduce the gap between read_cache_json's is_file precheck and
        // its production open helper, without relying on thread scheduling.
        assert!(path.is_file());
        let original = fixture.0.join("original.json");
        fs::rename(&path, &original).unwrap();
        let mut keeper = None;
        let mut listener = None;
        match kind {
            "fifo" => {
                make_fifo(&path);
                // Let the old blocking open complete so type rejection can
                // be tested independently of the no-peer watchdog below.
                keeper = Some(
                    fs::OpenOptions::new()
                        .read(true)
                        .write(true)
                        .custom_flags(libc::O_NONBLOCK)
                        .open(&path)
                        .unwrap(),
                );
            }
            "directory" => {
                fs::create_dir(&path).unwrap();
                fs::write(path.join("sentinel"), b"unchanged").unwrap();
            }
            "socket" => {
                listener = Some(std::os::unix::net::UnixListener::bind(&path).unwrap());
            }
            _ => unreachable!(),
        }
        let before = fs::symlink_metadata(&path).unwrap();
        assert!(
            open_cache_file_without_symlinks(&path).is_err(),
            "cache reader accepted {kind} after regular-file precheck"
        );
        let after = fs::symlink_metadata(&path).unwrap();
        assert_eq!(
            (before.dev(), before.ino(), before.mode(), before.nlink()),
            (after.dev(), after.ino(), after.mode(), after.nlink())
        );
        assert_eq!(fs::read(&original).unwrap(), content);
        if kind == "directory" {
            assert_eq!(fs::read(path.join("sentinel")).unwrap(), b"unchanged");
        }
        // Existing non-file paths still have the public API's cache-miss behavior.
        assert!(read_cache_json::<serde_json::Value>(root, relative)
            .unwrap()
            .is_none());
        drop(keeper);
        drop(listener);
    }
}

#[test]
fn cache_read_open_rejects_fifo_without_waiting_for_peer() {
    const CHILD_ROOT: &str = "CHATOS_TEST_CACHE_READ_FIFO_ROOT";
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let root = PathBuf::from(root);
        let path = project_cache_file_path(root.to_str().unwrap(), "code_nav/index.json").unwrap();
        let content = br#"{"cache":"original"}"#;
        fs::write(&path, content).unwrap();
        assert!(path.is_file());
        let original = root.join("original.json");
        fs::rename(&path, &original).unwrap();
        make_fifo(&path);
        let before = fs::symlink_metadata(&path).unwrap();
        assert!(open_cache_file_without_symlinks(&path).is_err());
        let after = fs::symlink_metadata(&path).unwrap();
        assert!(after.file_type().is_fifo());
        assert_eq!(
            (before.dev(), before.ino(), before.mode(), before.nlink()),
            (after.dev(), after.ino(), after.mode(), after.nlink())
        );
        assert_eq!(fs::read(&original).unwrap(), content);
        fs::write(root.join("completed"), b"ok").unwrap();
        return;
    }
    let fixture = Fixture::new();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "services::project_local_cache::special_tests::cache_read_open_rejects_fifo_without_waiting_for_peer",
            "--nocapture",
        ])
        .env(CHILD_ROOT, &fixture.0)
        .spawn()
        .unwrap();
    // Deadlock watchdog only: terminate and reap a blocked regression child.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "cache FIFO read child failed: {status}");
            assert!(
                fixture.0.join("completed").is_file(),
                "child did not finish"
            );
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("cache read blocked opening a FIFO without a writer");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn cache_write_rejects_special_files_without_writing() {
    for kind in ["fifo", "directory", "socket"] {
        let fixture = Fixture::new();
        let root = fixture.0.to_str().unwrap();
        let relative = "code_nav/index.json";
        let path = project_cache_file_path(root, relative).unwrap();
        let mut reader = None;
        let mut listener = None;
        match kind {
            "fifo" => {
                make_fifo(&path);
                // A reader allows the old blocking writer to open. Assert that
                // even a connected FIFO receives no cache bytes.
                reader = Some(
                    fs::OpenOptions::new()
                        .read(true)
                        .custom_flags(libc::O_NONBLOCK)
                        .open(&path)
                        .unwrap(),
                );
            }
            "directory" => {
                fs::create_dir(&path).unwrap();
                fs::write(path.join("sentinel"), b"unchanged").unwrap();
            }
            "socket" => {
                listener = Some(std::os::unix::net::UnixListener::bind(&path).unwrap());
            }
            _ => unreachable!(),
        }
        let before = fs::symlink_metadata(&path).unwrap();
        let result = write_cache_json(root, relative, &serde_json::json!({"cache": true}));
        assert!(result.is_err(), "cache writer accepted {kind}");
        let after = fs::symlink_metadata(&path).unwrap();
        assert_eq!(
            (before.dev(), before.ino(), before.mode()),
            (after.dev(), after.ino(), after.mode())
        );
        if let Some(mut reader) = reader.take() {
            let mut bytes = [0; 64];
            match reader.read(&mut bytes) {
                Ok(count) => assert_eq!(count, 0, "cache bytes entered FIFO"),
                Err(error) => assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock),
            }
        }
        if kind == "directory" {
            assert_eq!(fs::read(path.join("sentinel")).unwrap(), b"unchanged");
        }
        drop(listener);
    }
}

#[test]
fn cache_write_fifo_does_not_block_manager_rebuild() {
    const CHILD_ROOT: &str = "CHATOS_TEST_CACHE_WRITE_FIFO_ROOT";
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let root = PathBuf::from(root);
        let source = root.join("main.rs");
        let valid = root.join("valid.rs");
        fs::write(&source, "fn main() { greet(); }\n").unwrap();
        fs::write(&valid, "fn greet() {}\n").unwrap();
        let request = NavPositionRequest {
            project_root: root.to_str().unwrap().into(),
            file_path: source.to_str().unwrap().into(),
            line: 1,
            column: 14,
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let manager = CodeNavManager::new(vec![Arc::new(RustCodeNavProvider)]);
            assert!(!manager
                .definition(&request)
                .await
                .unwrap()
                .locations
                .is_empty());
            let entries = fs::read_dir(root.join(".chatos/cache/code_nav"))
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect::<Vec<_>>();
            assert_eq!(entries.len(), 1);
            let cache = &entries[0];
            assert!(cache.is_file());
            fs::remove_file(cache).unwrap();
            make_fifo(cache);
            // Dirty rebuild goes directly to the production cache writer.
            assert!(invalidate_project_symbol_indexes_for_path(&valid) > 0);
            let response = manager.definition(&request).await.unwrap();
            assert!(response
                .locations
                .iter()
                .any(|location| location.path == valid.to_str().unwrap()));
            assert!(fs::symlink_metadata(cache).unwrap().file_type().is_fifo());
            assert_eq!(fs::read(&source).unwrap(), b"fn main() { greet(); }\n");
            assert_eq!(fs::read(&valid).unwrap(), b"fn greet() {}\n");
            fs::write(root.join("completed"), b"ok").unwrap();
        });
        return;
    }
    let fixture = Fixture::new();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "services::project_local_cache::special_tests::cache_write_fifo_does_not_block_manager_rebuild", "--nocapture"])
        .env(CHILD_ROOT, &fixture.0).spawn().unwrap();
    // Deadlock watchdog, not a timing benchmark. Kill and reap on regression.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "cache FIFO child failed: {status}");
            assert!(
                fixture.0.join("completed").is_file(),
                "child did not finish"
            );
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("index rebuild blocked writing a FIFO without a reader");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
