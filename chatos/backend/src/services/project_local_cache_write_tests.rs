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
    fs::remove_dir_all(project.join(".chatos")).unwrap();
    let relative = "new/目录/index.json";
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

#[cfg(unix)]
#[test]
fn cache_write_rejects_hard_links_before_truncating() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    for name in ["outside.json", "project/inside.json"] {
        let fixture = Fixture::new();
        let project = fixture.project();
        let target = fixture.0.join(name);
        let sentinel = b"private fixture contents\n";
        fs::write(&target, sentinel).unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        let relative = "code_nav/index.json";
        let cache = project_cache_file_path(project.to_str().unwrap(), relative).unwrap();
        fs::hard_link(&target, &cache).unwrap();
        let before = fs::metadata(&target).unwrap();
        assert_eq!(before.nlink(), 2);
        assert!(!fs::symlink_metadata(&cache).unwrap().is_symlink());

        let result = write_cache_json(
            project.to_str().unwrap(),
            relative,
            &serde_json::json!({"replacement": true}),
        );
        assert!(result.is_err(), "cache writer accepted hard link to {name}");
        for path in [&target, &cache] {
            assert_eq!(fs::read(path).unwrap(), sentinel);
            let after = fs::metadata(path).unwrap();
            assert_eq!(
                (after.dev(), after.ino(), after.mode(), after.nlink()),
                (before.dev(), before.ino(), before.mode(), before.nlink())
            );
        }

        // A regular cache inode becomes writable again after its other name
        // is removed; the guard does not remember or ban previously linked files.
        fs::remove_file(&target).unwrap();
        let value = serde_json::json!({"ok": true});
        write_cache_json(project.to_str().unwrap(), relative, &value).unwrap();
        assert_eq!(
            fs::read(&cache).unwrap(),
            serde_json::to_vec_pretty(&value).unwrap()
        );
        assert_eq!(fs::metadata(&cache).unwrap().nlink(), 1);
    }
}

#[cfg(unix)]
#[test]
fn cache_write_rejects_ancestor_symlinks_before_creating_or_overwriting() {
    use std::os::unix::fs::{symlink, MetadataExt};

    for ancestor in ["", ".chatos", ".chatos/cache", ".chatos/cache/code_nav"] {
        for existing in [false, true] {
            let fixture = Fixture::new();
            let project = fixture.project();
            let redirected = if ancestor.is_empty() {
                project.clone()
            } else {
                project.join(ancestor)
            };
            let moved = fixture.0.join("moved");
            let relative = "code_nav/new/nested/index.json";
            let cache = project_cache_file_path(project.to_str().unwrap(), relative).unwrap();
            let value = serde_json::json!({"replacement": true});
            let sentinel = b"private fixture contents\n";
            if existing {
                fs::create_dir_all(cache.parent().unwrap()).unwrap();
                fs::write(&cache, sentinel).unwrap();
            }
            let target = moved.join(cache.strip_prefix(&redirected).unwrap());
            fs::rename(&redirected, &moved).unwrap();
            symlink(&moved, &redirected).unwrap();
            let before = fs::metadata(&moved).unwrap();
            let file_before = existing.then(|| fs::metadata(&target).unwrap());

            let result = write_cache_json(project.to_str().unwrap(), relative, &value);
            assert!(
                result.is_err(),
                "cache writer followed {ancestor:?}, existing={existing}"
            );
            assert_eq!(fs::read_link(&redirected).unwrap(), moved);
            let after = fs::metadata(&moved).unwrap();
            assert_eq!(
                (after.dev(), after.ino(), after.mode()),
                (before.dev(), before.ino(), before.mode())
            );
            if let Some(before) = file_before {
                assert_eq!(fs::read(&target).unwrap(), sentinel);
                let after = fs::metadata(&target).unwrap();
                assert_eq!(
                    (after.dev(), after.ino(), after.mode(), after.nlink()),
                    (before.dev(), before.ino(), before.mode(), before.nlink())
                );
            } else {
                let created = moved.join(
                    project
                        .join(".chatos/cache/code_nav/new")
                        .strip_prefix(&redirected)
                        .unwrap(),
                );
                assert!(
                    !created.exists(),
                    "rejected write created external directories"
                );
            }

            fs::remove_file(&redirected).unwrap();
            fs::rename(&moved, &redirected).unwrap();
            write_cache_json(project.to_str().unwrap(), relative, &value).unwrap();
            assert_eq!(
                fs::read(cache).unwrap(),
                serde_json::to_vec_pretty(&value).unwrap()
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn cache_write_rejects_dangling_and_non_directory_ancestors() {
    use std::os::unix::fs::symlink;

    for ancestor in [".chatos", ".chatos/cache", ".chatos/cache/code_nav"] {
        for dangling in [false, true] {
            let fixture = Fixture::new();
            let project = fixture.project();
            let blocked = project.join(ancestor);
            fs::remove_dir_all(&blocked).unwrap();
            let missing = fixture.0.join("missing");
            if dangling {
                symlink(&missing, &blocked).unwrap();
            } else {
                fs::write(&blocked, b"sentinel").unwrap();
            }
            assert!(write_cache_json(
                project.to_str().unwrap(),
                "code_nav/new/index.json",
                &serde_json::json!({"ok": true})
            )
            .is_err());
            assert!(!missing.exists());
            if dangling {
                assert_eq!(fs::read_link(&blocked).unwrap(), missing);
            } else {
                assert_eq!(fs::read(&blocked).unwrap(), b"sentinel");
            }
        }
    }
}
