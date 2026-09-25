// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("cache-write-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("project/.chatos/cache/code_nav")).unwrap();
        Self(fs::canonicalize(root).unwrap())
    }

    fn project(&self) -> PathBuf {
        self.0.join("project")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn cache_write_rejects_leaf_symlinks_without_touching_targets() {
    use std::os::unix::fs::{symlink, PermissionsExt};

    for (name, target_exists) in [
        ("outside.json", true),
        ("missing.json", false),
        ("project/inside.json", true),
    ] {
        let fixture = Fixture::new();
        let project = fixture.project();
        let target = fixture.0.join(name);
        if target_exists {
            fs::write(&target, b"private fixture contents\n").unwrap();
            fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let cache =
            project_cache_file_path(project.to_str().unwrap(), "code_nav/index.json").unwrap();
        symlink(&target, &cache).unwrap();

        let result = write_cache_json(
            project.to_str().unwrap(),
            "code_nav/index.json",
            &serde_json::json!({"replacement": true}),
        );
        assert!(result.is_err(), "cache write followed {name}");
        assert_eq!(fs::read_link(&cache).unwrap(), target);
        if target_exists {
            assert_eq!(fs::read(&target).unwrap(), b"private fixture contents\n");
            assert_eq!(
                fs::metadata(&target).unwrap().permissions().mode() & 0o777,
                0o600
            );
        } else {
            assert!(!target.exists(), "dangling target must not be created");
        }
    }
}

#[test]
fn cache_write_creates_and_replaces_regular_json_files() {
    let fixture = Fixture::new();
    let project = fixture.project();
    let relative = "new/nested/index.json";
    let long = serde_json::json!({"symbols": ["long first value", "another value"]});
    let short = serde_json::json!({"symbols": []});
    for value in [long, short] {
        write_cache_json(project.to_str().unwrap(), relative, &value).unwrap();
        assert_eq!(
            read_cache_json::<serde_json::Value>(project.to_str().unwrap(), relative).unwrap(),
            Some(value.clone())
        );
        assert_eq!(
            fs::read(project_cache_file_path(project.to_str().unwrap(), relative).unwrap())
                .unwrap(),
            serde_json::to_vec_pretty(&value).unwrap()
        );
    }
}
