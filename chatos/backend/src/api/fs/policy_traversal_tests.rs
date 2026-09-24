// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use std::fs;

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn fixture() -> (Fixture, FsPathPolicy) {
    let base = std::env::temp_dir().join(format!("chatos-fs-traversal-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&base).unwrap();
    let fixture = Fixture(fs::canonicalize(base).unwrap());
    let mut roots = Vec::new();
    for (name, kind) in [
        ("workspaces", FsAllowedRootKind::Workspace),
        ("public", FsAllowedRootKind::Public),
    ] {
        let path = fixture.0.join(name);
        fs::create_dir_all(path.join("nested")).unwrap();
        fs::write(path.join("note.txt"), "unchanged").unwrap();
        roots.push(FsAllowedRoot {
            path,
            kind,
            can_write: kind.can_write(),
            #[cfg(unix)]
            prepared_directory: None,
        });
    }
    (fixture, FsPathPolicy { roots })
}

#[test]
fn virtual_paths_reject_parent_components_after_separator_mapping() {
    let (_fixture, policy) = fixture();
    // The targets are inside allowed roots: canonical containment alone cannot
    // enforce the separate rule that user input must never contain traversal.
    for raw in [
        r"nested\..\note.txt",
        r"nested/..\note.txt",
        r"/public/nested\..\note.txt",
        r"\public\nested\..\note.txt",
        r"..\public\note.txt",
        "nested/../note.txt",
    ] {
        assert!(
            matches!(
                policy.authorize_existing_file(raw, "missing", "not file"),
                Err(FsPolicyError::Forbidden(ref message)) if message == PATH_TRAVERSAL_BLOCKED
            ),
            "parent components must be rejected before canonicalization: {raw:?}"
        );
    }
    for raw in [r"nested\..", r"/public/nested\..", r"..\public"] {
        assert!(matches!(
            policy.authorize_existing_dir(raw, "missing", "not dir"),
            Err(FsPolicyError::Forbidden(ref message)) if message == PATH_TRAVERSAL_BLOCKED
        ));
    }
    for root in &policy.roots {
        assert_eq!(
            fs::read_to_string(root.path.join("note.txt")).unwrap(),
            "unchanged"
        );
    }
}

#[test]
fn virtual_paths_keep_normal_separator_and_dot_filename_support() {
    let (_fixture, policy) = fixture();
    for raw in ["note.txt", "./note.txt", r".\note.txt", r"/public\note.txt"] {
        let authorized = policy
            .authorize_existing_file(raw, "missing", "not file")
            .unwrap();
        policy.require_write(&authorized).unwrap();
    }
    for root in &policy.roots {
        fs::write(root.path.join("..notes"), "valid filename").unwrap();
    }
    for raw in ["..notes", r"/public\..notes"] {
        policy
            .authorize_existing_file(raw, "missing", "not file")
            .unwrap();
    }

    #[cfg(unix)]
    {
        // Existing absolute Unix paths retain their native filename semantics.
        let path = policy.roots[0].path.join(r"literal\..\note.txt");
        fs::write(&path, "literal backslashes").unwrap();
        let authorized = policy
            .authorize_existing_file(path.to_str().unwrap(), "missing", "not file")
            .unwrap();
        assert_eq!(authorized.path, path);
        policy.require_write(&authorized).unwrap();
    }
}
