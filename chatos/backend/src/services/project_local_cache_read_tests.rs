// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("cache-read-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("project/.chatos/cache/code_nav")).unwrap();
        Self(fs::canonicalize(root).unwrap())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn cache_read_rejects_leaf_symlinks_without_loading_target_json() {
    use std::os::unix::fs::{symlink, MetadataExt};

    for name in ["outside.json", "project/inside.json"] {
        let fixture = Fixture::new();
        let project = fixture.0.join("project");
        let target = fixture.0.join(name);
        let sentinel = br#"{"private_fixture_value":"must not be loaded"}"#;
        fs::write(&target, sentinel).unwrap();
        let before = fs::metadata(&target).unwrap();
        let relative = "code_nav/index.json";
        let cache = project_cache_file_path(project.to_str().unwrap(), relative).unwrap();
        symlink(&target, &cache).unwrap();

        let result = read_cache_json::<serde_json::Value>(project.to_str().unwrap(), relative);
        assert!(result.is_err(), "cache reader followed link to {name}");
        assert!(!result.unwrap_err().contains("private_fixture_value"));
        assert_eq!(fs::read_link(&cache).unwrap(), target);
        assert_eq!(fs::read(&target).unwrap(), sentinel);
        let after = fs::metadata(&target).unwrap();
        assert_eq!(
            (after.dev(), after.ino(), after.mode(), after.nlink()),
            (before.dev(), before.ino(), before.mode(), before.nlink())
        );

        fs::remove_file(&target).unwrap();
        assert!(
            read_cache_json::<serde_json::Value>(project.to_str().unwrap(), relative)
                .unwrap()
                .is_none()
        );
        assert!(!target.exists());
        assert_eq!(fs::read_link(&cache).unwrap(), target);
    }
}

#[test]
fn cache_read_preserves_regular_json_missing_and_invalid_behavior() {
    let fixture = Fixture::new();
    let project = fixture.0.join("project");
    let root = project.to_str().unwrap();
    let relative = "code_nav/index.json";
    let cache = project_cache_file_path(root, relative).unwrap();
    assert_eq!(
        read_cache_json::<serde_json::Value>(root, relative).unwrap(),
        None
    );
    assert_eq!(
        read_cache_json::<serde_json::Value>(root, "code_nav").unwrap(),
        None
    );

    let value = serde_json::json!({"symbols": ["中文", "greet"], "revision": 1});
    let bytes = serde_json::to_vec(&value).unwrap();
    fs::write(&cache, &bytes).unwrap();
    assert_eq!(
        read_cache_json::<serde_json::Value>(root, relative).unwrap(),
        Some(value)
    );
    assert_eq!(fs::read(&cache).unwrap(), bytes);

    fs::write(&cache, b"invalid json").unwrap();
    assert!(read_cache_json::<serde_json::Value>(root, relative).is_err());
    assert_eq!(fs::read(&cache).unwrap(), b"invalid json");
}

#[cfg(unix)]
#[test]
fn cache_read_rejects_hard_links_without_loading_target_json() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    for name in ["outside.json", "project/inside.json"] {
        let fixture = Fixture::new();
        let project = fixture.0.join("project");
        let target = fixture.0.join(name);
        let sentinel = br#"{"private_fixture_value":"must not be loaded"}"#;
        fs::write(&target, sentinel).unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        let relative = "code_nav/index.json";
        let cache = project_cache_file_path(project.to_str().unwrap(), relative).unwrap();
        fs::hard_link(&target, &cache).unwrap();
        let before = fs::metadata(&target).unwrap();
        assert_eq!(before.nlink(), 2);
        assert!(!fs::symlink_metadata(&cache).unwrap().is_symlink());

        let result = read_cache_json::<serde_json::Value>(project.to_str().unwrap(), relative);
        assert!(result.is_err(), "cache reader accepted hard link to {name}");
        assert!(!result.unwrap_err().contains("private_fixture_value"));
        for path in [&target, &cache] {
            assert_eq!(fs::read(path).unwrap(), sentinel);
            let after = fs::metadata(path).unwrap();
            assert_eq!(
                (after.dev(), after.ino(), after.mode(), after.nlink()),
                (before.dev(), before.ino(), before.mode(), before.nlink())
            );
        }

        // Removing the other name restores an ordinary single-link cache.
        fs::remove_file(&target).unwrap();
        assert_eq!(
            read_cache_json::<serde_json::Value>(project.to_str().unwrap(), relative).unwrap(),
            Some(serde_json::from_slice(sentinel).unwrap())
        );
        assert_eq!(fs::read(&cache).unwrap(), sentinel);
        assert_eq!(fs::metadata(&cache).unwrap().nlink(), 1);
    }
}

#[cfg(unix)]
#[test]
fn cache_read_rejects_ancestor_symlinks_without_loading_target_json() {
    use std::os::unix::fs::{symlink, MetadataExt};

    for ancestor in [
        "project",
        "project/.chatos",
        "project/.chatos/cache",
        "project/.chatos/cache/code_nav",
    ] {
        let fixture = Fixture::new();
        let project = fixture.0.join("project");
        let relative = "code_nav/index.json";
        let cache = project_cache_file_path(project.to_str().unwrap(), relative).unwrap();
        let sentinel = br#"{"private_fixture_value":"must not be loaded"}"#;
        fs::write(&cache, sentinel).unwrap();
        let redirected = fixture.0.join(ancestor);
        let moved = fixture.0.join("moved");
        let target = moved.join(cache.strip_prefix(&redirected).unwrap());
        fs::rename(&redirected, &moved).unwrap();
        symlink(&moved, &redirected).unwrap();
        let before = fs::metadata(&target).unwrap();
        assert!(cache.is_file());
        assert!(!fs::symlink_metadata(&cache).unwrap().is_symlink());

        let result = read_cache_json::<serde_json::Value>(project.to_str().unwrap(), relative);
        assert!(result.is_err(), "cache reader followed ancestor {ancestor}");
        assert!(!result.unwrap_err().contains("private_fixture_value"));
        assert_eq!(fs::read_link(&redirected).unwrap(), moved);
        assert_eq!(fs::read(&target).unwrap(), sentinel);
        let after = fs::metadata(&target).unwrap();
        assert_eq!(
            (after.dev(), after.ino(), after.mode(), after.nlink()),
            (before.dev(), before.ino(), before.mode(), before.nlink())
        );

        // Restoring the real directory permits ordinary JSON reads again.
        fs::remove_file(&redirected).unwrap();
        fs::rename(&moved, &redirected).unwrap();
        assert_eq!(
            read_cache_json::<serde_json::Value>(project.to_str().unwrap(), relative).unwrap(),
            Some(serde_json::from_slice(sentinel).unwrap())
        );
    }
}
