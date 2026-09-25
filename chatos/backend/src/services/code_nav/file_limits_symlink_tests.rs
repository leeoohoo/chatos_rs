// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{open_code_nav_file, read_code_nav_file_to_string, read_code_nav_line_preview};
use crate::services::code_nav::fallback::fallback_document_symbols;
use crate::services::code_nav::types::DocumentSymbolsRequest;
use crate::services::code_nav::workspace::build_project_context;
use std::fs;
use std::io::Read;
use std::os::unix::fs::symlink;
use std::path::PathBuf;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("code-nav-symlink-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(path.join("project")).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn code_nav_reads_reject_ancestor_symlink_replacement_after_context_validation() {
    for replaced in ["project", "project/src", "project/src/nested"] {
        for target_exists in [true, false] {
            let fixture = Fixture::new();
            let root = fixture.0.join("project");
            let source = root.join("src/nested/main.rs");
            fs::create_dir_all(source.parent().unwrap()).unwrap();
            fs::write(&source, "fn allowed_symbol() {}\n").unwrap();
            let replaced_path = fixture.0.join(replaced);
            let relative_source = source.strip_prefix(&replaced_path).unwrap();
            let target = fixture.0.join("outside");
            let target_source = target.join(relative_source);
            if target_exists {
                fs::create_dir_all(target_source.parent().unwrap()).unwrap();
                fs::write(&target_source, "fn private_target_symbol() {}\n").unwrap();
            }
            let request = DocumentSymbolsRequest {
                project_root: root.to_str().unwrap().to_string(),
                file_path: source.to_str().unwrap().to_string(),
            };
            let context = build_project_context(&request.project_root, &request.file_path).unwrap();
            assert_eq!(
                fallback_document_symbols(&context, &request, "test")
                    .unwrap()
                    .symbols[0]
                    .name,
                "allowed_symbol"
            );
            let mut opened = open_code_nav_file(&context.file_path).unwrap();
            let original = fixture.0.join("original");
            fs::rename(&replaced_path, &original).unwrap();
            symlink(&target, &replaced_path).unwrap();

            let symbols = fallback_document_symbols(&context, &request, "test");
            assert!(
                symbols.is_err(),
                "ancestor {replaced} leaked symbols: {symbols:?}"
            );
            assert!(read_code_nav_file_to_string(&context.file_path).is_err());
            assert!(read_code_nav_line_preview(&context.file_path, 1, 100).is_err());
            let mut content = String::new();
            opened.read_to_string(&mut content).unwrap();
            assert_eq!(content, "fn allowed_symbol() {}\n");
            assert_eq!(
                fs::read_to_string(original.join(relative_source)).unwrap(),
                content
            );
            if target_exists {
                assert_eq!(
                    fs::read_to_string(target_source).unwrap(),
                    "fn private_target_symbol() {}\n"
                );
            } else {
                assert!(!target.exists());
            }
        }
    }
}

#[test]
fn code_nav_reads_accept_initial_ancestor_links_after_context_validation() {
    for linked in ["project", "project/src", "project/src/nested"] {
        let fixture = Fixture::new();
        let root = fixture.0.join("project");
        let source = root.join("src/nested/main.rs");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, "fn allowed_symbol() {}\n").unwrap();
        let original = fixture.0.join(linked);
        let resolved = if linked == "project" {
            fixture.0.join("resolved")
        } else {
            root.join("resolved")
        };
        fs::rename(&original, &resolved).unwrap();
        symlink(&resolved, &original).unwrap();
        let context =
            build_project_context(root.to_str().unwrap(), source.to_str().unwrap()).unwrap();
        assert_eq!(
            read_code_nav_file_to_string(&context.file_path).unwrap(),
            "fn allowed_symbol() {}\n"
        );
        assert_eq!(
            read_code_nav_line_preview(&context.file_path, 1, 100).unwrap(),
            "fn allowed_symbol() {}"
        );
    }
}

#[test]
fn code_nav_reads_reject_file_symlink_replacement_after_context_validation() {
    for target_name in ["outside.rs", "project/other.rs", "missing.rs"] {
        let fixture = Fixture::new();
        let root = fixture.0.join("project");
        let source = root.join("main.rs");
        let target = fixture.0.join(target_name);
        fs::write(&source, "fn allowed_symbol() {}\n").unwrap();
        if target_name != "missing.rs" {
            fs::write(&target, "fn private_target_symbol() {}\n").unwrap();
        }
        let request = DocumentSymbolsRequest {
            project_root: root.to_str().unwrap().to_string(),
            file_path: source.to_str().unwrap().to_string(),
        };
        let context = build_project_context(&request.project_root, &request.file_path).unwrap();
        let before = fallback_document_symbols(&context, &request, "test").unwrap();
        assert_eq!(before.symbols[0].name, "allowed_symbol");
        assert_eq!(
            read_code_nav_line_preview(&context.file_path, 1, 100).unwrap(),
            "fn allowed_symbol() {}"
        );

        fs::rename(&source, root.join("original.rs")).unwrap();
        symlink(&target, &source).unwrap();
        let symbols = fallback_document_symbols(&context, &request, "test");
        assert!(
            symbols.is_err(),
            "replacement {target_name} leaked symbols: {symbols:?}"
        );
        assert!(read_code_nav_line_preview(&context.file_path, 1, 100).is_err());
        assert!(read_code_nav_file_to_string(&context.file_path).is_err());
        assert_eq!(
            fs::read_to_string(root.join("original.rs")).unwrap(),
            "fn allowed_symbol() {}\n"
        );
        if target_name != "missing.rs" {
            assert_eq!(
                fs::read_to_string(&target).unwrap(),
                "fn private_target_symbol() {}\n"
            );
        } else {
            assert!(!target.exists());
        }
    }
}

#[test]
fn code_nav_reads_accept_initial_in_project_symlinks_after_canonicalization() {
    let fixture = Fixture::new();
    let root = fixture.0.join("project");
    let source = root.join("main.rs");
    let alias = root.join("alias.rs");
    fs::write(&source, "fn allowed_symbol() {}\n").unwrap();
    symlink(&source, &alias).unwrap();
    let context = build_project_context(root.to_str().unwrap(), alias.to_str().unwrap()).unwrap();
    assert_eq!(context.file_path, source);
    assert_eq!(
        read_code_nav_file_to_string(&context.file_path).unwrap(),
        "fn allowed_symbol() {}\n"
    );
    assert_eq!(
        read_code_nav_line_preview(&context.file_path, 1, 100).unwrap(),
        "fn allowed_symbol() {}"
    );
}

#[test]
fn code_nav_open_handle_does_not_follow_later_file_replacement() {
    let fixture = Fixture::new();
    let source = fixture.0.join("project/main.rs");
    let target = fixture.0.join("outside.rs");
    fs::write(&source, "original contents").unwrap();
    fs::write(&target, "private target contents").unwrap();
    let mut file = open_code_nav_file(&source).unwrap();
    fs::rename(&source, fixture.0.join("project/original.rs")).unwrap();
    symlink(&target, &source).unwrap();
    let mut content = String::new();
    file.read_to_string(&mut content).unwrap();
    assert_eq!(content, "original contents");
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "private target contents"
    );
}
