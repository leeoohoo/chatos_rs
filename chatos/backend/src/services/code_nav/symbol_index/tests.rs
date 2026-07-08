// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use super::{
    invalidate_project_symbol_indexes_for_path, nav_location_from_indexed_symbol,
    project_symbol_index, IndexedSymbol,
};

fn make_temp_symbol_index_project() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "code_nav_symbol_index_test_{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(root.join("src")).expect("create source dir");
    root
}

fn analyze_fixture_file(path: &Path) -> Result<Vec<IndexedSymbol>, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let mut symbols = Vec::new();
    for (index, line) in content.lines().enumerate() {
        let Some(name) = line.strip_prefix("symbol ") else {
            continue;
        };
        let name = name.trim().to_string();
        let column = line.find(&name).unwrap_or(0) + 1;
        symbols.push(IndexedSymbol {
            end_column: column + name.chars().count().saturating_sub(1),
            name,
            kind: "function".to_string(),
            line: index + 1,
            column,
            end_line: index + 1,
        });
    }
    Ok(symbols)
}

#[test]
fn project_symbol_index_collects_symbols_and_builds_locations() {
    let root = make_temp_symbol_index_project();
    let path = root.join("src/main.demo");
    fs::write(&path, "symbol greet\ncall greet\n").expect("write fixture");

    let index = project_symbol_index(
        root.as_path(),
        "test-symbol-index",
        &["demo"],
        &["ignored"],
        analyze_fixture_file,
    )
    .expect("build project symbol index");

    let symbols = index
        .symbols_by_name
        .get("greet")
        .expect("greet should be indexed");
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].relative_path, "src/main.demo");

    let location =
        nav_location_from_indexed_symbol(root.as_path(), &symbols[0], 7.0).expect("location");
    assert_eq!(location.line, 1);
    assert_eq!(location.column, 8);
    assert_eq!(location.preview, "symbol greet");

    fs::remove_dir_all(root).ok();
}

#[test]
fn project_symbol_index_reuses_cache_while_source_snapshot_is_unchanged() {
    let root = make_temp_symbol_index_project();
    let path = root.join("src/main.demo");
    fs::write(&path, "symbol greet\n").expect("write fixture");
    let analyze_calls = Arc::new(AtomicUsize::new(0));
    let provider_id = format!("test-symbol-index-cache-{}", uuid::Uuid::new_v4());

    let first_counter = Arc::clone(&analyze_calls);
    project_symbol_index(
        root.as_path(),
        provider_id.as_str(),
        &["demo"],
        &["ignored"],
        move |path| {
            first_counter.fetch_add(1, Ordering::SeqCst);
            analyze_fixture_file(path)
        },
    )
    .expect("build project symbol index");

    let second_counter = Arc::clone(&analyze_calls);
    project_symbol_index(
        root.as_path(),
        provider_id.as_str(),
        &["demo"],
        &["ignored"],
        move |path| {
            second_counter.fetch_add(1, Ordering::SeqCst);
            analyze_fixture_file(path)
        },
    )
    .expect("reuse project symbol index");

    assert_eq!(analyze_calls.load(Ordering::SeqCst), 1);
    fs::remove_dir_all(root).ok();
}

#[test]
fn project_symbol_index_rebuilds_when_source_snapshot_changes() {
    let root = make_temp_symbol_index_project();
    let path = root.join("src/main.demo");
    fs::write(&path, "symbol greet\n").expect("write fixture");
    let provider_id = format!("test-symbol-index-refresh-{}", uuid::Uuid::new_v4());

    project_symbol_index(
        root.as_path(),
        provider_id.as_str(),
        &["demo"],
        &["ignored"],
        analyze_fixture_file,
    )
    .expect("build project symbol index");

    fs::write(&path, "symbol greet\nsymbol farewell\n").expect("update fixture");
    let index = project_symbol_index(
        root.as_path(),
        provider_id.as_str(),
        &["demo"],
        &["ignored"],
        analyze_fixture_file,
    )
    .expect("refresh project symbol index");

    assert!(index.symbols_by_name.contains_key("farewell"));
    fs::remove_dir_all(root).ok();
}

#[test]
fn project_symbol_index_can_be_invalidated_by_changed_path() {
    let root = make_temp_symbol_index_project();
    let path = root.join("src/main.demo");
    fs::write(&path, "symbol greet\n").expect("write fixture");
    let analyze_calls = Arc::new(AtomicUsize::new(0));
    let provider_id = format!("test-symbol-index-invalidate-{}", uuid::Uuid::new_v4());

    let first_counter = Arc::clone(&analyze_calls);
    project_symbol_index(
        root.as_path(),
        provider_id.as_str(),
        &["demo"],
        &["ignored"],
        move |path| {
            first_counter.fetch_add(1, Ordering::SeqCst);
            analyze_fixture_file(path)
        },
    )
    .expect("build project symbol index");

    let removed = invalidate_project_symbol_indexes_for_path(path.as_path());
    assert_eq!(removed, 1);

    let second_counter = Arc::clone(&analyze_calls);
    project_symbol_index(
        root.as_path(),
        provider_id.as_str(),
        &["demo"],
        &["ignored"],
        move |path| {
            second_counter.fetch_add(1, Ordering::SeqCst);
            analyze_fixture_file(path)
        },
    )
    .expect("rebuild project symbol index after invalidation");

    assert_eq!(analyze_calls.load(Ordering::SeqCst), 2);
    fs::remove_dir_all(root).ok();
}
