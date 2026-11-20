// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::*;

fn read_file_exit_code(profile: &AppContainerProfile, file: &Path) -> Result<u32, String> {
    let system_root = std::env::var_os("SystemRoot")
        .ok_or_else(|| "Windows SystemRoot is unavailable.".to_owned())?;
    let reader = Path::new(&system_root).join("System32").join("findstr.exe");
    let command_line = format!(
        "\"{}\" /r /c:\"lease-readable\" \"{}\"",
        reader.display(),
        file.display()
    );
    profile.run(&reader, &command_line, Duration::from_secs(10))
}

#[test]
fn directory_read_lease_grants_only_the_workspace_and_restores_denial() {
    let workspace = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let outside_file = outside.path().join("outside.txt");
    std::fs::write(&outside_file, b"lease-readable").unwrap();
    let profile = AppContainerProfile::create("ChatOS.AppContainer.Probe").unwrap();
    let manager = WorkspaceLeaseManager::open(outside.path()).unwrap();
    let lease = manager
        .grant(workspace.path(), profile.sid, WorkspaceAccess::ReadOnly)
        .unwrap();
    let journal_path = lease.journal_path.clone();
    assert!(manager
        .grant(workspace.path(), profile.sid, WorkspaceAccess::ReadWrite)
        .is_err());
    let workspace_file = workspace.path().join("workspace.txt");
    std::fs::write(&workspace_file, b"lease-readable").unwrap();

    assert_eq!(read_file_exit_code(&profile, &workspace_file).unwrap(), 0);
    assert_ne!(read_file_exit_code(&profile, &outside_file).unwrap(), 0);

    lease.restore().unwrap();
    assert!(!journal_path.exists());
    assert_ne!(read_file_exit_code(&profile, &workspace_file).unwrap(), 0);

    let recovery_workspace = tempfile::tempdir().unwrap();
    let lease = manager
        .grant(
            recovery_workspace.path(),
            profile.sid,
            WorkspaceAccess::ReadOnly,
        )
        .unwrap();
    let journal_path = lease.journal_path.clone();
    let recovery_file = recovery_workspace.path().join("recovery.txt");
    std::fs::write(&recovery_file, b"lease-readable").unwrap();
    assert_eq!(read_file_exit_code(&profile, &recovery_file).unwrap(), 0);
    lease.abandon_for_recovery();
    assert!(journal_path.exists());
    drop(manager);
    let manager = WorkspaceLeaseManager::open(outside.path()).unwrap();
    assert!(!journal_path.exists());
    assert_eq!(manager.journal_count(), 0);
    assert_ne!(read_file_exit_code(&profile, &recovery_file).unwrap(), 0);

    let nonempty_workspace = tempfile::tempdir().unwrap();
    let nested = nonempty_workspace.path().join("nested");
    std::fs::create_dir(&nested).unwrap();
    let existing_file = nested.join("existing.txt");
    std::fs::write(&existing_file, b"lease-readable").unwrap();
    let lease = manager
        .grant(
            nonempty_workspace.path(),
            profile.sid,
            WorkspaceAccess::ReadOnly,
        )
        .unwrap();
    assert_eq!(read_file_exit_code(&profile, &existing_file).unwrap(), 0);
    let renamed_file = nested.join("renamed.txt");
    std::fs::rename(&existing_file, &renamed_file).unwrap();
    lease.restore().unwrap();
    assert_ne!(read_file_exit_code(&profile, &renamed_file).unwrap(), 0);
}

#[test]
fn workspace_parent_traversal_is_journaled_and_restored() {
    let app_data = tempfile::tempdir().unwrap();
    let parent_root = tempfile::tempdir().unwrap();
    let workspace_path = parent_root.path().join("workspace");
    std::fs::create_dir(&workspace_path).unwrap();
    let workspace = workspace_path;
    let parent = workspace.parent().unwrap().to_path_buf();
    let original_parent_dacl = read_dacl_sddl(&parent).unwrap();
    let manager = WorkspaceLeaseManager::open(app_data.path()).unwrap();
    let profile = AppContainerProfile::create("ChatOS.AppContainer.Probe").unwrap();
    let lease = manager
        .grant(&workspace, profile.sid, WorkspaceAccess::ReadOnly)
        .unwrap();
    assert_ne!(read_dacl_sddl(&parent).unwrap(), original_parent_dacl);
    let journal_path = lease.journal_path.clone();
    lease.abandon_for_recovery();
    assert!(journal_path.exists());
    drop(manager);
    let recovered = WorkspaceLeaseManager::open(app_data.path()).unwrap();
    assert_eq!(recovered.journal_count(), 0);
    assert_eq!(read_dacl_sddl(&parent).unwrap(), original_parent_dacl);
}

#[test]
fn legacy_root_journal_is_recovered_without_leaving_inherited_access() {
    let app_data = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let manager = WorkspaceLeaseManager::open(app_data.path()).unwrap();
    let profile = AppContainerProfile::create("ChatOS.AppContainer.Probe").unwrap();
    let original_dacl_sddl = read_dacl_sddl(workspace.path()).unwrap();
    grant_sid_access(
        workspace.path(),
        profile.sid,
        WorkspaceAccess::ReadOnly,
        true,
    )
    .unwrap();
    let inherited_file = workspace.path().join("legacy.txt");
    std::fs::write(&inherited_file, b"lease-readable").unwrap();
    assert_eq!(read_file_exit_code(&profile, &inherited_file).unwrap(), 0);
    let journal_path = manager.journal_root.join("legacy.json");
    write_lease_journal(
        &journal_path,
        &DirectoryLeaseJournal {
            version: 1,
            access: WorkspaceAccess::ReadOnly,
            workspace_path_utf16: workspace.path().as_os_str().encode_wide().collect(),
            original_dacl_sddl,
            sid_sddl: String::new(),
            entries: Vec::new(),
            parent_entries: Vec::new(),
        },
    )
    .unwrap();
    drop(manager);

    let recovered = WorkspaceLeaseManager::open(app_data.path()).unwrap();
    assert_eq!(recovered.journal_count(), 0);
    assert_ne!(read_file_exit_code(&profile, &inherited_file).unwrap(), 0);
}

#[test]
fn workspace_tree_skips_reparse_points_without_granting_link_targets() {
    let app_data = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("outside.txt");
    std::fs::write(&target, b"outside").unwrap();
    let link = workspace.path().join("link.txt");
    if std::os::windows::fs::symlink_file(&target, &link).is_err() {
        return;
    }
    let inside = workspace.path().join("inside.txt");
    std::fs::write(&inside, b"inside").unwrap();
    let manager = WorkspaceLeaseManager::open(app_data.path()).unwrap();
    let profile = AppContainerProfile::create("ChatOS.AppContainer.Probe").unwrap();
    let lease = manager
        .grant(workspace.path(), profile.sid, WorkspaceAccess::ReadOnly)
        .expect("a reparse child must not poison the workspace lease");
    assert_eq!(read_file_exit_code(&profile, &inside).unwrap(), 0);
    assert_ne!(read_file_exit_code(&profile, &target).unwrap(), 0);
    lease.restore().unwrap();
    assert_eq!(manager.journal_count(), 0);
}

#[test]
fn workspace_tree_skips_generated_and_dependency_directories() {
    let workspace = tempfile::tempdir().unwrap();
    let source = workspace.path().join("src");
    let target = workspace.path().join("target");
    let dependency = workspace.path().join("node_modules");
    std::fs::create_dir(&source).unwrap();
    std::fs::create_dir(&target).unwrap();
    std::fs::create_dir(&dependency).unwrap();
    std::fs::write(source.join("main.rs"), b"fn main() {}").unwrap();
    std::fs::write(target.join("generated.bin"), b"generated").unwrap();
    std::fs::write(dependency.join("package.js"), b"dependency").unwrap();

    let tree = lease_tree_handles(workspace.path()).unwrap();
    let paths = tree
        .iter()
        .map(|entry| entry.path.strip_prefix(workspace.path()).unwrap())
        .collect::<Vec<_>>();
    assert!(paths.iter().any(|path| *path == Path::new("src")));
    assert!(paths.iter().any(|path| *path == Path::new("src/main.rs")));
    assert!(!paths.iter().any(|path| path.starts_with("target")));
    assert!(!paths.iter().any(|path| path.starts_with("node_modules")));
}

#[test]
fn workspace_tree_recovery_rejects_replaced_entry_identity() {
    let app_data = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let existing_file = workspace.path().join("existing.txt");
    std::fs::write(&existing_file, b"original").unwrap();
    let manager = WorkspaceLeaseManager::open(app_data.path()).unwrap();
    let profile = AppContainerProfile::create("ChatOS.AppContainer.Probe").unwrap();
    let lease = manager
        .grant(workspace.path(), profile.sid, WorkspaceAccess::ReadOnly)
        .unwrap();
    let journal_path = lease.journal_path.clone();
    lease.abandon_for_recovery();
    std::fs::remove_file(&existing_file).unwrap();
    std::fs::write(&existing_file, b"replacement").unwrap();
    drop(manager);

    let failure = WorkspaceLeaseManager::open(app_data.path())
        .err()
        .expect("recovery must reject a replaced entry");
    assert!(failure.contains("changed identity"), "{failure}");
    assert!(journal_path.exists());
    assert_eq!(std::fs::read(&existing_file).unwrap(), b"replacement");
}

#[test]
fn workspace_lease_startup_recovery_discards_only_unpublished_drafts() {
    let app_data = tempfile::tempdir().unwrap();
    let journal_root = app_data
        .path()
        .join("local-agent")
        .join("appcontainer-workspace-leases");
    std::fs::create_dir_all(&journal_root).unwrap();
    let token = "00000000-0000-0000-0000-000000000000";
    let legacy_draft = journal_root.join(format!("{token}.tmp"));
    let owned_draft = journal_root.join(format!("{WORKSPACE_JOURNAL_TEMPORARY_PREFIX}{token}.tmp"));
    std::fs::write(&legacy_draft, b"legacy-not-published").unwrap();
    std::fs::write(&owned_draft, b"not-published").unwrap();

    let manager = WorkspaceLeaseManager::open(app_data.path()).unwrap();
    assert!(!legacy_draft.exists());
    assert!(!owned_draft.exists());
    assert_eq!(manager.journal_count(), 0);

    std::fs::write(
        journal_root.join(format!(
            "{WORKSPACE_JOURNAL_TEMPORARY_PREFIX}not-a-uuid.tmp"
        )),
        b"untrusted",
    )
    .unwrap();
    assert!(WorkspaceLeaseManager::open(app_data.path()).is_err());
}

#[test]
fn profile_lease_startup_recovery_discards_only_owned_or_legacy_drafts() {
    let app_data = tempfile::tempdir().unwrap();
    let journal_root = app_data
        .path()
        .join("local-agent")
        .join("appcontainer-profile-leases");
    std::fs::create_dir_all(&journal_root).unwrap();
    let token = "00000000-0000-0000-0000-000000000000";
    let legacy_draft = journal_root.join(format!("{token}.tmp"));
    let owned_draft = journal_root.join(format!("{PROFILE_JOURNAL_TEMPORARY_PREFIX}{token}.tmp"));
    std::fs::write(&legacy_draft, b"legacy-not-published").unwrap();
    std::fs::write(&owned_draft, b"not-published").unwrap();

    let manager = WorkspaceLeaseManager::open(app_data.path()).unwrap();
    assert!(!legacy_draft.exists());
    assert!(!owned_draft.exists());
    assert_eq!(manager.profile_journal_count(), 0);

    std::fs::write(
        journal_root.join(format!("{PROFILE_JOURNAL_TEMPORARY_PREFIX}not-a-uuid.tmp")),
        b"untrusted",
    )
    .unwrap();
    assert!(WorkspaceLeaseManager::open(app_data.path()).is_err());
}

#[test]
fn probe_cleanup_removes_only_canonical_stale_owned_trees() {
    let directory = tempfile::tempdir().unwrap();
    let token = "00000000-0000-0000-0000-000000000000";
    let owned = directory
        .path()
        .join(format!("{PROBE_TEMPORARY_PREFIX}{token}"));
    let malformed = directory
        .path()
        .join(format!("{PROBE_TEMPORARY_PREFIX}not-a-uuid"));
    let unrelated = directory.path().join(format!("other-probe-{token}"));
    std::fs::create_dir(&owned).unwrap();
    std::fs::write(owned.join("probe.txt"), b"owned").unwrap();
    std::fs::create_dir(&malformed).unwrap();
    std::fs::create_dir(&unrelated).unwrap();

    assert_eq!(
        cleanup_stale_probe_directories_in(
            directory.path(),
            SystemTime::now() + Duration::from_secs(1),
        ),
        1
    );
    assert!(!owned.exists());
    assert!(malformed.exists());
    assert!(unrelated.exists());
}
