// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{open_code_nav_file, read_code_nav_file_to_string, read_code_nav_line_preview};
use crate::services::code_nav::fallback::fallback_document_symbols;
use crate::services::code_nav::languages::rust::RustCodeNavProvider;
use crate::services::code_nav::manager::CodeNavManager;
use crate::services::code_nav::types::DocumentSymbolsRequest;
use crate::services::code_nav::workspace::build_project_context;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const ALLOWED: &str = "fn allowed_symbol() {}\n";
const PRIVATE: &str = "fn private_target_symbol() {}\n// 私有测试内容\n";

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("code-nav-hardlink-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("project")).unwrap();
        Self(fs::canonicalize(root).unwrap())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn assert_unchanged(path: &Path, before: &fs::Metadata) {
    assert_eq!(fs::read_to_string(path).unwrap(), PRIVATE);
    let after = fs::metadata(path).unwrap();
    assert_eq!(
        (after.dev(), after.ino(), after.mode(), after.nlink()),
        (before.dev(), before.ino(), before.mode(), before.nlink())
    );
}

#[test]
fn code_nav_reads_reject_hardlink_replacement_after_context_validation() {
    for target_name in ["outside.rs", "project/other.rs"] {
        let fixture = Fixture::new();
        let root = fixture.0.join("project");
        let source = root.join("main.rs");
        let original = root.join("original.rs");
        let target = fixture.0.join(target_name);
        fs::write(&source, ALLOWED).unwrap();
        fs::write(&target, PRIVATE).unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        let request = DocumentSymbolsRequest {
            project_root: root.to_str().unwrap().into(),
            file_path: source.to_str().unwrap().into(),
        };
        let context = build_project_context(&request.project_root, &request.file_path).unwrap();
        assert_eq!(
            fallback_document_symbols(&context, &request, "test")
                .unwrap()
                .symbols[0]
                .name,
            "allowed_symbol"
        );
        fs::rename(&source, &original).unwrap();
        fs::hard_link(&target, &source).unwrap();
        let before = fs::metadata(&target).unwrap();
        assert_eq!(before.nlink(), 2);
        assert!(!fs::symlink_metadata(&source).unwrap().is_symlink());

        let symbols = fallback_document_symbols(&context, &request, "test");
        assert!(
            symbols.is_err(),
            "hard link to {target_name} leaked symbols: {symbols:?}"
        );
        for result in [
            symbols.map(|_| ()),
            read_code_nav_file_to_string(&context.file_path).map(|_| ()),
            read_code_nav_line_preview(&context.file_path, 1, 100).map(|_| ()),
            open_code_nav_file(&context.file_path).map(|_| ()),
        ] {
            let error = result.expect_err("shared source inode must be rejected");
            assert!(!error.contains("private_target_symbol"));
            assert!(!error.contains(target.to_str().unwrap()));
        }
        assert_unchanged(&target, &before);
        assert_unchanged(&source, &before);
        assert_eq!(fs::read_to_string(&original).unwrap(), ALLOWED);

        // Once the second name is removed, this is a normal single-link source.
        fs::remove_file(&target).unwrap();
        assert_eq!(fs::metadata(&source).unwrap().nlink(), 1);
        assert_eq!(read_code_nav_file_to_string(&source).unwrap(), PRIVATE);
        assert_eq!(
            read_code_nav_line_preview(&source, 2, 100).unwrap(),
            "// 私有测试内容"
        );
        assert_eq!(
            fallback_document_symbols(&context, &request, "test")
                .unwrap()
                .symbols[0]
                .name,
            "private_target_symbol"
        );
    }
}

#[tokio::test]
async fn code_nav_manager_rejects_existing_hardlinked_source() {
    let fixture = Fixture::new();
    let root = fixture.0.join("project");
    let source = root.join("main.rs");
    let target = fixture.0.join("outside.rs");
    fs::write(&target, PRIVATE).unwrap();
    fs::hard_link(&target, &source).unwrap();
    let before = fs::metadata(&target).unwrap();
    let request = DocumentSymbolsRequest {
        project_root: root.to_str().unwrap().into(),
        file_path: source.to_str().unwrap().into(),
    };
    // Canonical path containment alone accepts a hard link already present
    // before context construction. Both provider and fallback must fail closed.
    let context = build_project_context(&request.project_root, &request.file_path).unwrap();
    assert_eq!(context.file_path, source);
    for manager in [
        CodeNavManager::new(vec![Arc::new(RustCodeNavProvider)]),
        CodeNavManager::new(vec![]),
    ] {
        let response = manager.document_symbols(&request).await;
        assert!(
            response.is_err(),
            "manager loaded hardlinked source: {response:?}"
        );
        assert!(!response.unwrap_err().contains("private_target_symbol"));
    }
    assert_unchanged(&source, &before);
    assert_unchanged(&target, &before);

    fs::remove_file(&target).unwrap();
    let response = CodeNavManager::new(vec![Arc::new(RustCodeNavProvider)])
        .document_symbols(&request)
        .await
        .unwrap();
    assert!(response
        .symbols
        .iter()
        .any(|symbol| symbol.name == "private_target_symbol"));
    assert_eq!(fs::read_to_string(&source).unwrap(), PRIVATE);
}
