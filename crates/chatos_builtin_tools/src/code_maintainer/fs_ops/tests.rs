// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
