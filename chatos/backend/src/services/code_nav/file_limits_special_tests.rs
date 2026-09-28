// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{open_code_nav_file, read_code_nav_file_to_string, read_code_nav_line_preview};
use crate::services::code_nav::fallback::fallback_document_symbols;
use crate::services::code_nav::types::DocumentSymbolsRequest;
use crate::services::code_nav::workspace::build_project_context;
use std::ffi::CString;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileTypeExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        // Keep the socket fixture below sockaddr_un's path limit even when
        // the platform's default temporary directory has a long prefix.
        let path = Path::new("/tmp").join(format!("code-nav-special-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(fs::canonicalize(path).unwrap())
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
fn code_nav_regular_files_retain_content_and_size_limits() {
    let fixture = Fixture::new();
    let source = fixture.0.join("main.rs");
    for content in ["", "fn allowed_symbol() {}\n// 你好\n"] {
        fs::write(&source, content).unwrap();
        assert!(open_code_nav_file(&source)
            .unwrap()
            .metadata()
            .unwrap()
            .is_file());
        assert_eq!(read_code_nav_file_to_string(&source).unwrap(), content);
    }
    assert_eq!(read_code_nav_line_preview(&source, 2, 4).unwrap(), "// 你");
    let content = "x".repeat(super::CODE_NAV_MAX_FILE_BYTES as usize);
    fs::write(&source, &content).unwrap();
    assert_eq!(read_code_nav_file_to_string(&source).unwrap(), content);
    assert_eq!(read_code_nav_line_preview(&source, 1, 4).unwrap(), "xxxx");
    fs::write(&source, format!("{content}x")).unwrap();
    assert!(open_code_nav_file(&source)
        .unwrap_err()
        .contains("exceeds limit"));
    assert!(read_code_nav_file_to_string(&source)
        .unwrap_err()
        .contains("exceeds limit"));
    assert!(read_code_nav_line_preview(&source, 1, 4)
        .unwrap_err()
        .contains("exceeds limit"));
}

#[test]
fn code_nav_open_rejects_non_regular_replacements() {
    for replacement in ["fifo", "directory", "socket"] {
        let fixture = Fixture::new();
        let source = fixture.0.join("main.rs");
        fs::write(&source, "fn allowed_symbol() {}\n").unwrap();
        let context =
            build_project_context(fixture.0.to_str().unwrap(), source.to_str().unwrap()).unwrap();
        let original = fixture.0.join("original.rs");
        fs::rename(&source, &original).unwrap();
        let mut keeper = None;
        let mut listener = None;
        match replacement {
            "fifo" => {
                make_fifo(&source);
                // Keep a writer attached so even the old blocking open completes.
                // The separate subprocess regression covers the no-peer case.
                keeper = Some(
                    fs::OpenOptions::new()
                        .read(true)
                        .write(true)
                        .custom_flags(libc::O_NONBLOCK)
                        .open(&source)
                        .unwrap(),
                );
            }
            "directory" => fs::create_dir(&source).unwrap(),
            "socket" => {
                listener = Some(std::os::unix::net::UnixListener::bind(&source).unwrap());
            }
            _ => unreachable!(),
        }
        assert!(
            open_code_nav_file(&context.file_path).is_err(),
            "accepted {replacement} after regular-file validation"
        );
        assert_eq!(
            fs::read_to_string(original).unwrap(),
            "fn allowed_symbol() {}\n"
        );
        drop(keeper);
        drop(listener);
    }
}

#[test]
fn code_nav_reads_reject_fifo_without_waiting_for_peer() {
    const CHILD_ROOT: &str = "CHATOS_TEST_CODE_NAV_FIFO_ROOT";
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let root = PathBuf::from(root);
        let source = root.join("main.rs");
        let content = "fn allowed_symbol() {}\n";
        fs::write(&source, content).unwrap();
        let request = DocumentSymbolsRequest {
            project_root: root.to_str().unwrap().to_string(),
            file_path: source.to_str().unwrap().to_string(),
        };
        let context = build_project_context(&request.project_root, &request.file_path).unwrap();
        assert_eq!(read_code_nav_file_to_string(&source).unwrap(), content);
        let original = root.join("original.rs");
        fs::rename(&source, &original).unwrap();
        make_fifo(&source);
        assert!(fallback_document_symbols(&context, &request, "test").is_err());
        assert!(read_code_nav_file_to_string(&source).is_err());
        assert!(read_code_nav_line_preview(&source, 1, 100).is_err());
        assert!(open_code_nav_file(&source).is_err());
        assert!(fs::symlink_metadata(&source).unwrap().file_type().is_fifo());
        assert_eq!(fs::read_to_string(original).unwrap(), content);
        return;
    }

    let fixture = Fixture::new();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "services::code_nav::file_limits::special_tests::code_nav_reads_reject_fifo_without_waiting_for_peer",
            "--nocapture",
        ])
        .env(CHILD_ROOT, &fixture.0)
        .spawn()
        .unwrap();
    // A watchdog bounds regressions that would otherwise hang the test suite.
    // This is a deadlock guard, not a machine-dependent performance benchmark.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "FIFO read regression failed: {status}");
            assert!(fixture.0.join("original.rs").is_file(), "child did not run");
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("code navigation blocked on a FIFO with no peer");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
