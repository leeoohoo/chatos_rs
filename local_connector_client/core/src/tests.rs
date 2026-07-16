// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;
use uuid::Uuid;

use crate::relay::RelayRequest;
use crate::sandbox::workspace::{
    local_sandbox_baseline_workspace, prepare_local_sandbox_workspace,
};
use crate::terminal::guard::{
    normalize_path_for_guard, path_is_inside_root, sanitize_terminal_command_line,
    validate_local_terminal_command, validate_local_terminal_directory_change,
};
use crate::workspace::paths::resolve_request_workspace_path;

mod local_mcp;

fn temp_test_dir(name: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("chatos-local-connector-{name}-{}", Uuid::new_v4()));
    fs::create_dir_all(path.as_path()).expect("create temp test dir");
    path
}

fn test_relay_request(workspace_id: &str) -> RelayRequest {
    RelayRequest {
        _message_type: "sandbox_request".to_string(),
        request_id: "req-test".to_string(),
        owner_user_id: Some("user-test".to_string()),
        device_id: Some("device-test".to_string()),
        workspace_id: workspace_id.to_string(),
        method: Some("POST".to_string()),
        path: Some("/api/sandboxes/leases".to_string()),
        headers: BTreeMap::new(),
        body: json!({}),
    }
}

fn test_workspace(root: &Path) -> WorkspaceState {
    WorkspaceState {
        id: "workspace-test".to_string(),
        absolute_root: fs::canonicalize(root).expect("canonical root"),
        alias: "workspace".to_string(),
        fingerprint: "fingerprint-test".to_string(),
        project_config_trust: None,
    }
}

fn request_with_cwd(cwd: &str) -> RelayRequest {
    let mut request = test_relay_request("workspace-test");
    request
        .headers
        .insert("x-local-connector-cwd".to_string(), cwd.to_string());
    request
}

fn request_with_cwd_and_builtin_kinds(cwd: &str, kinds: &str) -> RelayRequest {
    let mut request = request_with_cwd(cwd);
    request.headers.insert(
        LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER.to_string(),
        kinds.to_string(),
    );
    request
}

fn test_state_with_workspace(workspace: WorkspaceState) -> LocalState {
    LocalState {
        workspaces: vec![workspace],
        ..LocalState::default()
    }
}

fn test_state_with_full_control_workspace(workspace: WorkspaceState) -> LocalState {
    let mut state = test_state_with_workspace(workspace);
    state.approval.default_mode = crate::approval::ApprovalMode::FullControl;
    state
}

#[test]
fn local_connector_path_at_current_cwd_resolves_to_project_root() {
    let root = temp_test_dir("path-local-uri");
    let project = root
        .join("learn")
        .join("applocations")
        .join("react-fs-explorer");
    fs::create_dir_all(project.as_path()).expect("create project");
    let workspace = test_workspace(root.as_path());
    let request = request_with_cwd("learn/applocations/react-fs-explorer");

    let resolved = resolve_request_workspace_path(
        &workspace,
        &request,
        "local://connector/device-test/workspace-test/learn/applocations/react-fs-explorer",
    )
    .expect("resolve local connector project path");

    assert_eq!(
        normalize_path_for_guard(resolved.as_path()),
        normalize_path_for_guard(
            fs::canonicalize(project.as_path())
                .expect("canonical project")
                .as_path()
        )
    );
    fs::remove_dir_all(root.as_path()).expect("cleanup");
}

#[test]
fn absolute_workspace_path_at_current_cwd_resolves_to_project_root() {
    let root = temp_test_dir("path-absolute-project");
    let project = root
        .join("learn")
        .join("applocations")
        .join("react-fs-explorer");
    fs::create_dir_all(project.as_path()).expect("create project");
    let workspace = test_workspace(root.as_path());
    let request = request_with_cwd("learn/applocations/react-fs-explorer");

    let resolved =
        resolve_request_workspace_path(&workspace, &request, project.to_string_lossy().as_ref())
            .expect("resolve absolute project path");

    assert_eq!(
        normalize_path_for_guard(resolved.as_path()),
        normalize_path_for_guard(
            fs::canonicalize(project.as_path())
                .expect("canonical project")
                .as_path()
        )
    );
    fs::remove_dir_all(root.as_path()).expect("cleanup");
}

#[test]
fn workspace_root_absolute_path_is_clamped_to_current_project_cwd() {
    let root = temp_test_dir("path-absolute-root");
    let project = root
        .join("learn")
        .join("applocations")
        .join("react-fs-explorer");
    fs::create_dir_all(project.as_path()).expect("create project");
    let workspace = test_workspace(root.as_path());
    let request = request_with_cwd("learn/applocations/react-fs-explorer");

    let resolved = resolve_request_workspace_path(
        &workspace,
        &request,
        workspace.absolute_root.to_string_lossy().as_ref(),
    )
    .expect("resolve absolute workspace root path");

    assert_eq!(
        normalize_path_for_guard(resolved.as_path()),
        normalize_path_for_guard(
            fs::canonicalize(project.as_path())
                .expect("canonical project")
                .as_path()
        )
    );
    fs::remove_dir_all(root.as_path()).expect("cleanup");
}

#[test]
fn workspace_absolute_path_outside_current_project_is_rejected() {
    let root = temp_test_dir("path-absolute-outside");
    let project = root
        .join("learn")
        .join("applocations")
        .join("react-fs-explorer");
    let sibling = root.join("learn").join("other-project");
    fs::create_dir_all(project.as_path()).expect("create project");
    fs::create_dir_all(sibling.as_path()).expect("create sibling");
    let workspace = test_workspace(root.as_path());
    let request = request_with_cwd("learn/applocations/react-fs-explorer");

    let err =
        resolve_request_workspace_path(&workspace, &request, sibling.to_string_lossy().as_ref())
            .expect_err("sibling project should be outside current project cwd");

    assert!(err.to_string().contains("outside current local project"));
    fs::remove_dir_all(root.as_path()).expect("cleanup");
}

#[test]
fn relative_workspace_path_matching_cwd_is_not_duplicated() {
    let root = temp_test_dir("path-relative-cwd");
    let project = root
        .join("learn")
        .join("applocations")
        .join("react-fs-explorer");
    let package_json = project.join("package.json");
    fs::create_dir_all(project.as_path()).expect("create project");
    fs::write(package_json.as_path(), "{}").expect("write package");
    let workspace = test_workspace(root.as_path());
    let request = request_with_cwd("learn/applocations/react-fs-explorer");

    let resolved = resolve_request_workspace_path(
        &workspace,
        &request,
        "learn/applocations/react-fs-explorer/package.json",
    )
    .expect("resolve workspace-relative file path");

    assert_eq!(
        normalize_path_for_guard(resolved.as_path()),
        normalize_path_for_guard(
            fs::canonicalize(package_json.as_path())
                .expect("canonical package")
                .as_path()
        )
    );
    fs::remove_dir_all(root.as_path()).expect("cleanup");
}

#[test]
fn prepare_local_sandbox_workspace_clears_existing_run_copy() {
    let root = temp_test_dir("workspace-copy");
    let workspace_root = root.join("project");
    fs::create_dir_all(workspace_root.as_path()).expect("create project root");
    fs::write(workspace_root.join("keep.txt"), "current").expect("write project file");
    fs::create_dir_all(workspace_root.join(".chatos").join("task-runner"))
        .expect("create internal dir");
    fs::write(
        workspace_root
            .join(".chatos")
            .join("task-runner")
            .join("skip.txt"),
        "internal",
    )
    .expect("write internal file");

    let run_workspace = workspace_root
        .join(".chatos")
        .join("task-runner")
        .join("runs")
        .join("run-test")
        .join("input")
        .join("workspace");
    let baseline_workspace =
        local_sandbox_baseline_workspace(run_workspace.as_path()).expect("baseline path");
    fs::create_dir_all(run_workspace.as_path()).expect("create run workspace");
    fs::create_dir_all(baseline_workspace.as_path()).expect("create baseline workspace");
    fs::write(run_workspace.join("stale.txt"), "old").expect("write stale run file");
    fs::write(baseline_workspace.join("stale.txt"), "old").expect("write stale baseline file");

    let workspace = WorkspaceState {
        id: "workspace-test".to_string(),
        absolute_root: fs::canonicalize(workspace_root.as_path()).expect("canonical root"),
        alias: "project".to_string(),
        fingerprint: "fingerprint-test".to_string(),
        project_config_trust: None,
    };
    let state = LocalState {
        workspaces: vec![workspace],
        ..LocalState::default()
    };
    prepare_local_sandbox_workspace(
        &test_relay_request("workspace-test"),
        &state,
        &json!({ "run_workspace": run_workspace.to_string_lossy() }),
    )
    .expect("prepare workspace");

    assert!(run_workspace.join("keep.txt").is_file());
    assert!(baseline_workspace.join("keep.txt").is_file());
    assert!(!run_workspace.join("stale.txt").exists());
    assert!(!baseline_workspace.join("stale.txt").exists());
    assert!(!run_workspace.join(".chatos").exists());
    assert!(!baseline_workspace.join(".chatos").exists());

    fs::remove_dir_all(root.as_path()).expect("cleanup temp test dir");
}

#[test]
fn local_terminal_directory_guard_allows_descendants_and_blocks_escape() {
    let root = temp_test_dir("terminal-guard");
    let project = root.join("project");
    let child = project.join("child");
    fs::create_dir_all(child.as_path()).expect("create child");
    let project = fs::canonicalize(project.as_path()).expect("canonical project");
    let mut current = project.clone();

    assert!(
        validate_local_terminal_directory_change("cd child", project.as_path(), &mut current,)
            .is_none()
    );
    assert!(path_is_inside_root(current.as_path(), project.as_path()));

    assert!(
        validate_local_terminal_directory_change("cd ..", project.as_path(), &mut current,)
            .is_none()
    );
    assert_eq!(
        normalize_path_for_guard(current.as_path()),
        normalize_path_for_guard(project.as_path())
    );

    let blocked =
        validate_local_terminal_directory_change("cd ..", project.as_path(), &mut current);
    assert_eq!(
        blocked.as_deref(),
        Some("Blocked: cannot leave terminal workspace.")
    );

    let blocked_root =
        validate_local_terminal_directory_change("cd /", project.as_path(), &mut current);
    assert_eq!(
        blocked_root.as_deref(),
        Some("Blocked: cannot leave terminal workspace.")
    );

    fs::remove_dir_all(root.as_path()).expect("cleanup");
}

#[test]
fn local_terminal_directory_guard_blocks_dynamic_and_pushd() {
    let root = temp_test_dir("terminal-guard-dynamic");
    let project = root.join("project");
    fs::create_dir_all(project.as_path()).expect("create project");
    let project = fs::canonicalize(project.as_path()).expect("canonical project");
    let mut current = project.clone();

    assert!(
        validate_local_terminal_directory_change("cd $HOME", project.as_path(), &mut current)
            .is_some()
    );
    assert!(
        validate_local_terminal_directory_change("pushd .", project.as_path(), &mut current)
            .is_some()
    );

    fs::remove_dir_all(root.as_path()).expect("cleanup");
}

#[test]
fn local_terminal_directory_guard_blocks_ansi_wrapped_escape() {
    let root = temp_test_dir("terminal-guard-ansi");
    let project = root.join("project");
    let outside = root.join("outside");
    fs::create_dir_all(project.as_path()).expect("create project");
    fs::create_dir_all(outside.as_path()).expect("create outside");
    let project = fs::canonicalize(project.as_path()).expect("canonical project");
    let outside = fs::canonicalize(outside.as_path()).expect("canonical outside");
    let mut current = project.clone();
    let command = format!("\x1b[200~cd {}\x1b[201~", outside.display());
    let sanitized = sanitize_terminal_command_line(command.as_str());
    assert_eq!(sanitized, format!("cd {}", outside.display()));
    assert!(validate_local_terminal_directory_change(
        sanitized.as_str(),
        project.as_path(),
        &mut current,
    )
    .is_some());

    fs::remove_dir_all(root.as_path()).expect("cleanup");
}

#[test]
fn local_terminal_command_guard_blocks_obvious_outside_path_arguments() {
    let root = temp_test_dir("terminal-guard-path-args");
    let project = root.join("project");
    let child = project.join("child");
    let outside = root.join("outside");
    fs::create_dir_all(child.as_path()).expect("create child");
    fs::create_dir_all(outside.as_path()).expect("create outside");
    fs::write(child.join("file.txt"), "ok").expect("write child file");
    fs::write(outside.join("secret.txt"), "nope").expect("write outside file");
    let project = fs::canonicalize(project.as_path()).expect("canonical project");
    let outside = fs::canonicalize(outside.as_path()).expect("canonical outside");
    let mut current = project.clone();

    assert!(
        validate_local_terminal_command("ls child", project.as_path(), &mut current,).is_none()
    );
    assert!(
        validate_local_terminal_command("cat child/file.txt", project.as_path(), &mut current,)
            .is_none()
    );
    assert!(validate_local_terminal_command("ls /", project.as_path(), &mut current).is_some());
    assert!(validate_local_terminal_command(
        format!("cat {}", outside.join("secret.txt").display()).as_str(),
        project.as_path(),
        &mut current,
    )
    .is_some());
    assert!(validate_local_terminal_command(
        "cat ../outside/secret.txt",
        project.as_path(),
        &mut current,
    )
    .is_some());

    fs::remove_dir_all(root.as_path()).expect("cleanup");
}
