// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::*;

fn assert_send<T: Send>() {}

#[test]
fn interactive_runtime_handle_can_move_to_a_blocking_worker() {
    assert_send::<SandboxedChild>();
}

#[test]
fn launches_a_zero_capability_appcontainer_with_file_and_network_denial() {
    let probe = probe_isolation().expect("AppContainer isolation probe should run");
    assert!(probe.filesystem_denied);
    assert!(probe.network_denied);
    assert!(probe.workspace_lease_verified);
    assert!(probe.workspace_recovery_verified);
    assert!(probe.workspace_permission_tiers_verified);
    assert!(probe.nonempty_workspace_tree_verified);
    assert!(probe.network_capability_policy_verified);
    assert!(probe.runtime_launcher_verified);
    assert_eq!(
        NetworkAccess::from_policy("disabled").unwrap(),
        NetworkAccess::Disabled
    );
    assert_eq!(
        NetworkAccess::from_policy("controlled").unwrap(),
        NetworkAccess::Controlled
    );
    assert_eq!(
        NetworkAccess::from_policy("loopback").unwrap(),
        NetworkAccess::Loopback
    );
    assert!(NetworkAccess::from_policy("host").is_err());
}

#[test]
fn read_write_lease_runs_real_git_against_an_existing_repository() {
    // Prefer the regular machine Git installation. The Codex-bundled Git
    // may appear first in `where.exe` on developer machines, but its
    // private parent ACL is intentionally not AppContainer-readable; in
    // that case CreateProcessW reports the misleading ERROR_FILE_NOT_FOUND
    // even though the current user can execute it. Fall back to PATH only
    // when the machine installation is absent.
    let git = std::env::var_os("ProgramFiles")
        .map(PathBuf::from)
        .map(|program_files| program_files.join("Git").join("cmd").join("git.exe"))
        .filter(|path| path.is_file())
        .or_else(|| {
            std::process::Command::new("where.exe")
                .arg("git.exe")
                .output()
                .ok()
                .filter(|located| located.status.success())
                .and_then(|located| {
                    String::from_utf8_lossy(&located.stdout)
                        .lines()
                        .find(|line| !line.trim().is_empty())
                        .map(|line| PathBuf::from(line.trim()))
                })
                .filter(|path| path.is_file())
        });
    let Some(git) = git else {
        return;
    };
    let app_data = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let initialized = std::process::Command::new(&git)
        .arg("init")
        .arg(workspace.path())
        .status()
        .unwrap();
    assert!(initialized.success());
    std::fs::write(workspace.path().join("fixture.txt"), b"fixture").unwrap();

    let manager = WorkspaceLeaseManager::open(app_data.path()).unwrap();
    let profile = AppContainerProfile::create("ChatOS.AppContainer.Probe").unwrap();
    let lease = manager
        .grant(workspace.path(), profile.sid, WorkspaceAccess::ReadWrite)
        .unwrap();
    let command_line = build_windows_command_line(
        git.as_os_str(),
        &[
            "-C".to_owned(),
            workspace.path().to_string_lossy().into_owned(),
            "add".to_owned(),
            "--".to_owned(),
            "fixture.txt".to_owned(),
        ],
    )
    .unwrap();
    let captured = profile
        .run_captured(
            &git,
            &command_line,
            workspace.path(),
            Duration::from_secs(30),
            NetworkAccess::Disabled,
            None,
            1_024 * 1_024,
        )
        .unwrap();
    assert_eq!(
        captured.exit_code,
        0,
        "{}",
        String::from_utf8_lossy(&captured.stderr)
    );
    assert!(captured.stdout.is_empty());
    assert!(captured.capability_policy_verified);
    lease.restore().unwrap();

    let staged = std::process::Command::new(&git)
        .arg("-C")
        .arg(workspace.path())
        .args(["diff", "--cached", "--name-only"])
        .output()
        .unwrap();
    assert!(staged.status.success());
    assert_eq!(
        String::from_utf8_lossy(&staged.stdout).trim(),
        "fixture.txt"
    );
}

#[test]
fn runtime_launcher_bounds_output_and_restores_its_workspace() {
    let system_root = std::env::var_os("SystemRoot").unwrap();
    let shell = Path::new(&system_root).join("System32").join("cmd.exe");
    let app_data = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let manager = WorkspaceLeaseManager::open(app_data.path()).unwrap();
    let failure = manager
        .run_workspace_command(
            workspace.path(),
            &shell,
            &[
                "/d".to_owned(),
                "/c".to_owned(),
                "for /L %i in (1,1,1000) do @echo 1234567890".to_owned(),
            ],
            WorkspaceAccess::ReadOnly,
            "disabled",
            &restricted_runtime_environment(),
            Duration::from_secs(10),
            1_024,
        )
        .unwrap_err();
    assert!(failure.contains("output exceeded"), "{failure}");
    assert_eq!(manager.journal_count(), 0);
}

#[test]
fn interactive_runtime_round_trips_stdio_and_restores_its_leases() {
    let system_root = std::env::var_os("SystemRoot").unwrap();
    let shell = Path::new(&system_root).join("System32").join("cmd.exe");
    let app_data = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let manager = WorkspaceLeaseManager::open(app_data.path()).unwrap();
    let mut child = manager
        .spawn_workspace_command(
            workspace.path(),
            &shell,
            &[
                "/d".to_owned(),
                "/q".to_owned(),
                "/v:on".to_owned(),
                "/c".to_owned(),
                "set /p CHATOS_LINE=& echo reply:!CHATOS_LINE!".to_owned(),
            ],
            WorkspaceAccess::ReadOnly,
            "disabled",
            &restricted_runtime_environment(),
        )
        .unwrap();
    let mut stdin = child.take_stdin().unwrap();
    let mut stdout = child.take_stdout().unwrap();
    let mut stderr = child.take_stderr().unwrap();
    stdin.write_all(b"mcp-roundtrip\r\n").unwrap();
    drop(stdin);

    assert_eq!(child.wait(Duration::from_secs(10)).unwrap(), 0);
    let mut stdout_bytes = Vec::new();
    stdout.read_to_end(&mut stdout_bytes).unwrap();
    let mut stderr_bytes = Vec::new();
    stderr.read_to_end(&mut stderr_bytes).unwrap();
    assert_eq!(
        String::from_utf8_lossy(&stdout_bytes).trim(),
        "reply:mcp-roundtrip"
    );
    assert!(
        stderr_bytes.is_empty(),
        "{}",
        String::from_utf8_lossy(&stderr_bytes)
    );
    assert_eq!(manager.journal_count(), 0);
    assert_eq!(manager.profile_journal_count(), 0);
}

#[test]
fn interactive_runtime_rejects_overlapping_workspace_leases_before_launch() {
    let system_root = std::env::var_os("SystemRoot").unwrap();
    let shell = Path::new(&system_root).join("System32").join("cmd.exe");
    let app_data = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let nested = workspace.path().join("nested");
    std::fs::create_dir(&nested).unwrap();
    let manager = WorkspaceLeaseManager::open(app_data.path()).unwrap();
    let failure = manager
        .spawn_workspace_command_with_leases(
            workspace.path(),
            &shell,
            &["/d".to_owned(), "/c".to_owned(), "exit 0".to_owned()],
            &[
                (workspace.path().to_owned(), WorkspaceAccess::ReadOnly),
                (nested, WorkspaceAccess::ReadWrite),
            ],
            "disabled",
            &restricted_runtime_environment(),
        )
        .err()
        .expect("overlapping workspace leases must fail");
    assert!(failure.contains("overlap"), "{failure}");
    assert_eq!(manager.journal_count(), 0);
    assert_eq!(manager.profile_journal_count(), 0);
}

#[test]
fn runtime_profile_journal_recovers_a_crash_abandoned_profile() {
    let app_data = tempfile::tempdir().unwrap();
    let manager = WorkspaceLeaseManager::open(app_data.path()).unwrap();
    let profile = AppContainerProfile::create_journaled(&manager.profile_journal_root).unwrap();
    let profile_name = String::from_utf16(profile.name.strip_suffix(&[0]).unwrap()).unwrap();
    assert_eq!(manager.profile_journal_count(), 1);
    profile.abandon_for_recovery();

    assert_eq!(manager.recover_profiles().unwrap(), 1);
    assert_eq!(manager.profile_journal_count(), 0);
    let recreated = AppContainerProfile::create_named(&profile_name, None, None).unwrap();
    drop(recreated);
}
