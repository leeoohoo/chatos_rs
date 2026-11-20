// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use std::io::Read;

fn read_locked_owner(owner: &DatabaseOwner) -> Vec<u8> {
    let mut file = owner._file.try_clone().unwrap();
    file.rewind().unwrap();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    bytes
}

#[test]
fn contention_does_not_create_database_or_truncate_live_owner_description() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("client.sqlite");
    let first = DatabaseOwner::acquire(&database).unwrap();
    let path = lock_path(first.database());
    let before = read_locked_owner(&first);
    let description: serde_json::Value = serde_json::from_slice(&before).unwrap();
    assert_eq!(description["format_version"], 1);
    assert_eq!(description["pid"], std::process::id());
    assert_eq!(
        description["database"],
        directory
            .path()
            .canonicalize()
            .unwrap()
            .join("client.sqlite")
            .to_str()
            .unwrap()
    );
    assert!(Uuid::parse_str(description["owner_epoch"].as_str().unwrap()).is_ok());
    let error = DatabaseOwner::acquire(&database).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
    assert_eq!(read_locked_owner(&first), before);
    assert!(!database.exists(), "ownership precedes SQLite creation");
    drop(first);
    assert!(path.exists(), "never unlink the lock file to release it");
    let second = DatabaseOwner::acquire(&database).unwrap();
    let after: serde_json::Value = serde_json::from_slice(&read_locked_owner(&second)).unwrap();
    assert_ne!(after["owner_epoch"], description["owner_epoch"]);
    assert_eq!(second.database(), database);
}

#[test]
fn stale_or_invalid_metadata_never_prevents_kernel_lock_reacquisition() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("client.sqlite");
    for bytes in [
        b"not valid JSON".as_slice(),
        br#"{"pid":1,"owner_epoch":"stale"}"#,
    ] {
        fs::write(lock_path(&database), bytes).unwrap();
        let owner = DatabaseOwner::acquire(&database).unwrap();
        let description: serde_json::Value =
            serde_json::from_slice(&read_locked_owner(&owner)).unwrap();
        assert_eq!(description["pid"], std::process::id());
    }
}

#[test]
fn different_databases_have_independent_owners() {
    let directory = tempfile::tempdir().unwrap();
    let a = DatabaseOwner::acquire(&directory.path().join("a.sqlite")).unwrap();
    let b = DatabaseOwner::acquire(&directory.path().join("b.sqlite")).unwrap();
    assert_ne!(a.database(), b.database());
}

#[cfg(unix)]
#[test]
fn database_and_parent_symlinks_resolve_to_the_same_lock_and_private_inode() {
    use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};

    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("client.sqlite");
    fs::write(&database, b"placeholder; not opened by this test").unwrap();
    let first = DatabaseOwner::acquire(&database).unwrap();
    let alias = directory.path().join("alias.sqlite");
    symlink(&database, &alias).unwrap();
    assert_eq!(
        DatabaseOwner::acquire(&alias).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    let parent_alias = directory.path().join("parent-alias");
    symlink(directory.path(), &parent_alias).unwrap();
    assert_eq!(
        DatabaseOwner::acquire(&parent_alias.join("client.sqlite"))
            .unwrap_err()
            .kind(),
        io::ErrorKind::WouldBlock
    );
    let lock = lock_path(first.database());
    let metadata = fs::metadata(&lock).unwrap();
    assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    let inode = metadata.ino();
    drop(first);
    let _next = DatabaseOwner::acquire(&alias).unwrap();
    assert_eq!(
        _next.database(),
        alias,
        "preserve configured artifact-root location"
    );
    assert_eq!(fs::metadata(&lock).unwrap().ino(), inode);
}

#[cfg(unix)]
#[test]
fn parent_alias_for_a_new_database_cannot_bypass_ownership() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("client.sqlite");
    let _first = DatabaseOwner::acquire(&database).unwrap();
    let alias = directory.path().join("parent-alias");
    symlink(directory.path(), &alias).unwrap();
    assert_eq!(
        DatabaseOwner::acquire(&alias.join("client.sqlite"))
            .unwrap_err()
            .kind(),
        io::ErrorKind::WouldBlock
    );
}

#[cfg(unix)]
#[test]
fn unsafe_database_or_lock_paths_are_rejected_without_overwriting_targets() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("client.sqlite");
    let target = directory.path().join("keep.txt");
    fs::write(&target, b"keep this file").unwrap();
    symlink(&target, lock_path(&database)).unwrap();
    assert!(DatabaseOwner::acquire(&database).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"keep this file");
    fs::remove_file(lock_path(&database)).unwrap();
    fs::hard_link(&target, lock_path(&database)).unwrap();
    assert!(DatabaseOwner::acquire(&database).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"keep this file");
    fs::remove_file(lock_path(&database)).unwrap();
    fs::hard_link(&target, &database).unwrap();
    assert!(DatabaseOwner::acquire(&database).is_err());
    assert!(!lock_path(&database).exists());
    assert_eq!(fs::read(&target).unwrap(), b"keep this file");
    fs::remove_file(&database).unwrap();
    symlink(directory.path().join("missing.sqlite"), &database).unwrap();
    assert!(DatabaseOwner::acquire(&database).is_err());
    assert!(!directory.path().join("missing.sqlite").exists());
    assert!(DatabaseOwner::acquire(directory.path()).is_err());
}
