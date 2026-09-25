// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::*;
use crate::services::code_nav::languages::rust::RustCodeNavProvider;
use crate::services::code_nav::manager::CodeNavManager;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("code-nav-index-paths-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("project/src")).unwrap();
        Self(fs::canonicalize(root).unwrap())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let root = self.0.join("project");
        let key = project_symbol_index_cache_key(&root, "rust");
        PROJECT_SYMBOL_INDEX_CACHE.remove(&key);
        PROJECT_SYMBOL_INDEX_DIRTY_PATHS.remove(&key);
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn indexed(path: &Path) -> ProjectIndexedSymbol {
    ProjectIndexedSymbol {
        path: path.to_string_lossy().into_owned(),
        // The persisted relative path is also untrusted. It must not authorize
        // an unrelated absolute path, even when it names a legitimate source.
        relative_path: "src/valid.rs".into(),
        symbol: IndexedSymbol {
            name: "greet".into(),
            kind: "function".into(),
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 5,
        },
    }
}

#[tokio::test]
async fn persisted_symbol_paths_cannot_read_outside_project_via_manager() {
    let fixture = Fixture::new();
    let root = fixture.0.join("project");
    let source = root.join("src/main.rs");
    let valid = root.join("src/valid.rs");
    let outside = fixture.0.join("private.txt");
    fs::write(&source, "fn main() { greet(); }\n").unwrap();
    fs::write(&valid, "fn greet() {}\n").unwrap();
    fs::write(&outside, "private_fixture_value\n").unwrap();

    // A project writer can supply a cache with a matching source snapshot but
    // fabricated index paths. No source mutation or timing race is necessary.
    let snapshot = project_symbol_index_snapshot(&root, &["rs"], &[]).unwrap();
    let poisoned = ProjectSymbolIndex {
        symbols_by_name: HashMap::from([(
            "greet".into(),
            vec![indexed(&valid), indexed(&outside)],
        )]),
    };
    write_cache_json(
        root.to_str().unwrap(),
        &symbol_index_cache_path("rust"),
        &persisted_project_symbol_index_entry(snapshot.clone(), &poisoned),
    )
    .unwrap();
    let request = NavPositionRequest {
        project_root: root.to_str().unwrap().into(),
        file_path: source.to_str().unwrap().into(),
        line: 1,
        column: 14,
    };
    let manager = CodeNavManager::new(vec![Arc::new(RustCodeNavProvider)]);
    for phase in ["persisted cache", "memory cache"] {
        let response = manager.definition(&request).await.unwrap();
        // Confirm the test reached the cache path instead of quietly rebuilding
        // from sources; both entries remain in the untrusted cached index.
        let key = project_symbol_index_cache_key(&root, "rust");
        let cached = PROJECT_SYMBOL_INDEX_CACHE.get(&key).unwrap();
        assert_eq!(cached.snapshot, snapshot);
        assert_eq!(cached.index.symbols_by_name["greet"].len(), 2);
        assert!(
            response.locations.iter().all(|location| {
                Path::new(&location.path).starts_with(&root)
                    && !location.preview.contains("private_fixture_value")
            }),
            "{phase} returned unauthorized locations: {:?}",
            response.locations
        );
        assert_eq!(response.locations.len(), 1, "{phase}");
        assert_eq!(response.locations[0].path, valid.to_str().unwrap());
        assert_eq!(response.locations[0].preview, "fn greet() {}");
    }
    assert_eq!(
        fs::read_to_string(outside).unwrap(),
        "private_fixture_value\n"
    );
}

#[test]
fn indexed_preview_rejects_untrusted_paths_before_reading() {
    let fixture = Fixture::new();
    let root = fixture.0.join("project");
    let outside = fixture.0.join("private.txt");
    let sibling = fixture.0.join("project-other/private.txt");
    fs::write(&outside, "private_fixture_value\n").unwrap();
    fs::create_dir_all(sibling.parent().unwrap()).unwrap();
    fs::write(&sibling, "private_fixture_value\n").unwrap();

    for path in [
        outside,
        sibling,
        root.join("../private.txt"),
        root.join("src/../../private.txt"),
        PathBuf::from("src/valid.rs"),
        PathBuf::new(),
    ] {
        for line in [0, 1] {
            let mut entry = indexed(&path);
            entry.symbol.line = line;
            let error = nav_location_from_indexed_symbol(&root, &entry, 1.0)
                .expect_err("untrusted index must not authorize a path");
            assert_eq!(error, "code-nav indexed path is outside project root");
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn index_cache_read_rejects_leaf_symlink_via_manager() {
    use crate::services::project_local_cache::project_cache_file_path;
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new();
    let root = fixture.0.join("project");
    let source = root.join("src/main.rs");
    let valid = root.join("src/valid.rs");
    fs::write(&source, "fn main() { greet(); }\n").unwrap();
    fs::write(&valid, "fn greet() {}\n").unwrap();
    let snapshot = project_symbol_index_snapshot(&root, &["rs"], &[]).unwrap();
    let external_index = ProjectSymbolIndex {
        symbols_by_name: HashMap::from([("greet".into(), vec![indexed(&valid)])]),
    };
    let bytes = serde_json::to_vec(&persisted_project_symbol_index_entry(
        snapshot,
        &external_index,
    ))
    .unwrap();
    let target = fixture.0.join("outside.json");
    fs::write(&target, &bytes).unwrap();
    let cache =
        project_cache_file_path(root.to_str().unwrap(), &symbol_index_cache_path("rust")).unwrap();
    fs::create_dir_all(cache.parent().unwrap()).unwrap();
    symlink(&target, &cache).unwrap();

    let manager = CodeNavManager::new(vec![Arc::new(RustCodeNavProvider)]);
    let response = manager
        .definition(&NavPositionRequest {
            project_root: root.to_str().unwrap().into(),
            file_path: source.to_str().unwrap().into(),
            line: 1,
            column: 14,
        })
        .await
        .unwrap();
    // Matching external JSON must not be admitted to the in-memory index.
    // The existing provider search fallback should still find real source.
    assert!(
        !PROJECT_SYMBOL_INDEX_CACHE.contains_key(&project_symbol_index_cache_key(&root, "rust")),
        "manager loaded the external index through a cache symlink"
    );
    assert!(response.locations.iter().any(|location| {
        location.path == valid.to_str().unwrap() && location.preview == "fn greet() {}"
    }));
    assert_eq!(fs::read_link(&cache).unwrap(), target);
    assert_eq!(fs::read(&target).unwrap(), bytes);
    assert_eq!(fs::read(&source).unwrap(), b"fn main() { greet(); }\n");
    assert_eq!(fs::read(&valid).unwrap(), b"fn greet() {}\n");
}

#[cfg(unix)]
#[test]
fn indexed_preview_preserves_native_paths_and_rejects_symlinks() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new();
    let root = fixture.0.join("project");
    let outside = fixture.0.join("project\\private/secret.rs");
    fs::create_dir_all(outside.parent().unwrap()).unwrap();
    fs::write(&outside, "private_fixture_value\n").unwrap();
    assert!(nav_location_from_indexed_symbol(&root, &indexed(&outside), 1.0).is_err());

    for name in ["src/valid.rs", "src/你好.rs", "src/valid\\name.rs"] {
        let path = root.join(name);
        fs::write(&path, "fn greet() {}\n").unwrap();
        let location = nav_location_from_indexed_symbol(&root, &indexed(&path), 1.0).unwrap();
        assert_eq!(location.path, path.to_str().unwrap());
        assert_eq!(location.preview, "fn greet() {}");
    }

    let leaf = root.join("src/link.rs");
    symlink(&outside, &leaf).unwrap();
    assert!(nav_location_from_indexed_symbol(&root, &indexed(&leaf), 1.0).is_err());
    let directory = root.join("redirected");
    symlink(outside.parent().unwrap(), &directory).unwrap();
    assert!(
        nav_location_from_indexed_symbol(&root, &indexed(&directory.join("secret.rs")), 1.0)
            .is_err()
    );
    assert_eq!(
        fs::read_to_string(outside).unwrap(),
        "private_fixture_value\n"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn index_cache_write_cannot_overwrite_symlink_target_via_manager() {
    use crate::services::project_local_cache::project_cache_file_path;
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new();
    let root = fixture.0.join("project");
    let source = root.join("src/main.rs");
    let valid = root.join("src/valid.rs");
    fs::write(&source, "fn main() { greet(); }\n").unwrap();
    fs::write(&valid, "fn greet() {}\n").unwrap();
    let request = NavPositionRequest {
        project_root: root.to_str().unwrap().into(),
        file_path: source.to_str().unwrap().into(),
        line: 1,
        column: 14,
    };
    let manager = CodeNavManager::new(vec![Arc::new(RustCodeNavProvider)]);
    assert!(!manager
        .definition(&request)
        .await
        .unwrap()
        .locations
        .is_empty());
    let cache =
        project_cache_file_path(root.to_str().unwrap(), &symbol_index_cache_path("rust")).unwrap();
    assert!(cache.is_file());
    let target = fixture.0.join("outside.json");
    let sentinel = b"private fixture contents\n";
    fs::write(&target, sentinel).unwrap();
    fs::remove_file(&cache).unwrap();
    symlink(&target, &cache).unwrap();
    // The real dirty-index branch writes the cache without reading it first.
    // A project writer can replace the leaf between two ordinary requests.
    invalidate_project_symbol_indexes_for_path(&valid);
    let response = manager.definition(&request).await.unwrap();
    assert!(response
        .locations
        .iter()
        .any(|location| location.path == valid.to_str().unwrap()));
    assert_eq!(fs::read_link(&cache).unwrap(), target);
    assert_eq!(
        fs::read(&target).unwrap(),
        sentinel,
        "index persistence overwrote a file outside the project"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn index_cache_write_cannot_overwrite_hard_link_target_via_manager() {
    use crate::services::project_local_cache::project_cache_file_path;
    use std::os::unix::fs::MetadataExt;

    let fixture = Fixture::new();
    let root = fixture.0.join("project");
    let source = root.join("src/main.rs");
    let valid = root.join("src/valid.rs");
    fs::write(&source, "fn main() { greet(); }\n").unwrap();
    fs::write(&valid, "fn greet() {}\n").unwrap();
    let request = NavPositionRequest {
        project_root: root.to_str().unwrap().into(),
        file_path: source.to_str().unwrap().into(),
        line: 1,
        column: 14,
    };
    let manager = CodeNavManager::new(vec![Arc::new(RustCodeNavProvider)]);
    assert!(!manager
        .definition(&request)
        .await
        .unwrap()
        .locations
        .is_empty());
    let cache =
        project_cache_file_path(root.to_str().unwrap(), &symbol_index_cache_path("rust")).unwrap();
    let target = fixture.0.join("outside.json");
    let sentinel = b"private fixture contents\n";
    fs::write(&target, sentinel).unwrap();
    fs::remove_file(&cache).unwrap();
    fs::hard_link(&target, &cache).unwrap();
    let before = fs::metadata(&target).unwrap();
    assert_eq!(before.nlink(), 2);
    // Exercise actual persistence after a project writer replaces the cache.
    assert!(invalidate_project_symbol_indexes_for_path(&valid) > 0);
    let response = manager.definition(&request).await.unwrap();
    assert!(response.locations.iter().any(|location| {
        location.path == valid.to_str().unwrap() && location.preview == "fn greet() {}"
    }));
    for path in [&target, &cache] {
        assert_eq!(
            fs::read(path).unwrap(),
            sentinel,
            "index persistence overwrote a hard-linked file"
        );
        let after = fs::metadata(path).unwrap();
        assert_eq!(
            (after.dev(), after.ino(), after.nlink()),
            (before.dev(), before.ino(), before.nlink())
        );
    }
    assert_eq!(fs::read(&source).unwrap(), b"fn main() { greet(); }\n");
    assert_eq!(fs::read(&valid).unwrap(), b"fn greet() {}\n");
}
