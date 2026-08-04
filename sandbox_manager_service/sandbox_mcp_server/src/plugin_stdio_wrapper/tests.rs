// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    linux_sandbox_arguments, linux_sandboxed_plugin_command_with_launcher, PluginStdioSandboxSpec,
};
use std::ffi::OsStr;
use std::path::Path;

#[test]
fn wrapper_rejects_unknown_or_duplicate_options_before_launch() {
    assert!(PluginStdioSandboxSpec::from_args(vec![
        "--unknown".to_string(),
        "value".to_string(),
        "--".to_string(),
        "/bin/true".to_string(),
    ])
    .is_err());
    assert!(PluginStdioSandboxSpec::from_args(vec![
        "--env".to_string(),
        "TOKEN".to_string(),
        "--env".to_string(),
        "TOKEN".to_string(),
        "--".to_string(),
        "/bin/true".to_string(),
    ])
    .is_err());
    assert!(super::validate_environment_name("CHATOS_WORKSPACE").is_err());
    assert!(super::validate_environment_name("CHATOS_PLUGIN_ROOT").is_err());
    assert!(super::validate_environment_name("APPDATA").is_err());
    assert!(super::validate_environment_name("LOCALAPPDATA").is_err());
}

#[test]
fn windows_appcontainer_source_contract_is_offline_read_only_and_tree_scoped() {
    let source = include_str!("windows.rs");
    for required in [
        "CreateAppContainerProfile",
        "CapabilityCount: 0",
        "PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES",
        "PROC_THREAD_ATTRIBUTE_HANDLE_LIST",
        "PROC_THREAD_ATTRIBUTE_ALL_APPLICATION_PACKAGES_POLICY",
        "PROCESS_CREATION_ALL_APPLICATION_PACKAGES_OPT_OUT",
        "signed Plugin package file failed SHA-256 verification",
        "grant_appcontainer_read_only",
        "Do not deny FILE_GENERIC_WRITE as a single mask",
        "JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE",
        "JOB_OBJECT_LIMIT_ACTIVE_PROCESS",
        "WindowsWorkspaceMirror",
        "workspace.commit()",
        "CHATOS_WORKSPACE",
    ] {
        assert!(
            source.contains(required),
            "Windows Plugin stdio isolation contract is missing {required}"
        );
    }
    assert!(!source.contains("internetClient"));
    assert!(!source.contains("CreateProcessAsUserW"));
}

#[test]
fn wrapper_accepts_one_disjoint_workspace_root_and_rejects_overlap() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let plugin_root = temp.path().join("plugin");
    let state_root = temp.path().join("runtime/state");
    let cache_root = temp.path().join("runtime/cache");
    let temp_root = temp.path().join("runtime/tmp");
    let package_index = temp.path().join("runtime/package-index.json");
    let workspace_root = temp.path().join("workspace");
    for directory in [
        &plugin_root,
        &state_root,
        &cache_root,
        &temp_root,
        &workspace_root,
    ] {
        std::fs::create_dir_all(directory).expect("create sandbox test directory");
    }
    let command = plugin_root.join("hook.sh");
    std::fs::write(command.as_path(), "#!/bin/sh\n").expect("write Hook command");
    std::fs::write(
        package_index.as_path(),
        r#"{"schema_version":1,"files":{}}"#,
    )
    .expect("write package index");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(command.as_path(), std::fs::Permissions::from_mode(0o700))
            .expect("make Hook command executable");
    }
    let args = |workspace: &Path| {
        vec![
            "--sandbox-id".to_string(),
            "00000000-0000-4000-8000-000000000001".to_string(),
            "--plugin-root".to_string(),
            plugin_root.to_string_lossy().into_owned(),
            "--state-root".to_string(),
            state_root.to_string_lossy().into_owned(),
            "--cache-root".to_string(),
            cache_root.to_string_lossy().into_owned(),
            "--temp-root".to_string(),
            temp_root.to_string_lossy().into_owned(),
            "--package-index".to_string(),
            package_index.to_string_lossy().into_owned(),
            "--workspace-root".to_string(),
            workspace.to_string_lossy().into_owned(),
            "--cwd".to_string(),
            plugin_root.to_string_lossy().into_owned(),
            "--".to_string(),
            command.to_string_lossy().into_owned(),
        ]
    };

    let parsed = PluginStdioSandboxSpec::from_args(args(workspace_root.as_path()))
        .expect("parse disjoint workspace root");
    assert_eq!(
        parsed.workspace_root,
        Some(
            workspace_root
                .canonicalize()
                .expect("canonical workspace root")
        )
    );
    assert!(PluginStdioSandboxSpec::from_args(args(plugin_root.as_path())).is_err());
}

#[test]
fn linux_workspace_contract_is_minimal_offline_and_protects_git() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let plugin_root = temp.path().join("plugin");
    let state_root = temp.path().join("runtime/state");
    let cache_root = temp.path().join("runtime/cache");
    let temp_root = temp.path().join("runtime/tmp");
    let package_index = temp.path().join("runtime/package-index.json");
    let workspace_root = temp.path().join("workspace");
    for directory in [
        &plugin_root,
        &state_root,
        &cache_root,
        &temp_root,
        &workspace_root,
    ] {
        std::fs::create_dir_all(directory).expect("create sandbox test directory");
    }
    let command = plugin_root.join("hook.sh");
    std::fs::write(command.as_path(), "#!/bin/sh\n").expect("write Hook command");
    std::fs::write(
        package_index.as_path(),
        r#"{"schema_version":1,"files":{}}"#,
    )
    .expect("write package index");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(command.as_path(), std::fs::Permissions::from_mode(0o700))
            .expect("make Hook command executable");
    }
    let spec = PluginStdioSandboxSpec::from_args(vec![
        "--sandbox-id".to_string(),
        "00000000-0000-4000-8000-000000000002".to_string(),
        "--plugin-root".to_string(),
        plugin_root.to_string_lossy().into_owned(),
        "--state-root".to_string(),
        state_root.to_string_lossy().into_owned(),
        "--cache-root".to_string(),
        cache_root.to_string_lossy().into_owned(),
        "--temp-root".to_string(),
        temp_root.to_string_lossy().into_owned(),
        "--package-index".to_string(),
        package_index.to_string_lossy().into_owned(),
        "--workspace-root".to_string(),
        workspace_root.to_string_lossy().into_owned(),
        "--cwd".to_string(),
        plugin_root.to_string_lossy().into_owned(),
        "--".to_string(),
        command.to_string_lossy().into_owned(),
    ])
    .expect("parse writable Hook sandbox");

    let render = |args: Vec<std::ffi::OsString>| {
        args.into_iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
    };
    let args = render(linux_sandbox_arguments(&spec).expect("Linux sandbox arguments"));
    let contains_mount = |values: &[String], option: &str, path: &Path| {
        let path = path.to_string_lossy();
        values
            .windows(3)
            .any(|window| window[0] == option && window[1] == path && window[2] == path)
    };
    assert!(args.iter().any(|value| value == "--unshare-net"));
    assert!(args.windows(2).any(|window| window == ["--tmpfs", "/"]));
    assert!(!args
        .windows(3)
        .any(|window| window == ["--ro-bind", "/", "/"]));
    assert!(contains_mount(
        args.as_slice(),
        "--ro-bind",
        spec.plugin_root.as_path()
    ));
    assert!(contains_mount(
        args.as_slice(),
        "--bind",
        spec.state_root.as_path()
    ));
    assert!(contains_mount(
        args.as_slice(),
        "--bind",
        spec.cache_root.as_path()
    ));
    assert!(contains_mount(
        args.as_slice(),
        "--bind",
        spec.temp_root.as_path()
    ));
    assert!(contains_mount(
        args.as_slice(),
        "--bind",
        spec.workspace_root
            .as_ref()
            .expect("canonical workspace root")
            .as_path()
    ));
    let git_root = spec
        .workspace_root
        .as_ref()
        .expect("canonical workspace root")
        .join(".git");
    let git_root_text = git_root.to_string_lossy();
    assert!(args
        .windows(2)
        .any(|window| { window[0] == "--tmpfs" && window[1] == git_root_text }));
    assert!(args
        .windows(2)
        .any(|window| { window[0] == "--remount-ro" && window[1] == git_root_text }));

    let command =
        linux_sandboxed_plugin_command_with_launcher(&spec, Path::new("/trusted/system/bwrap"))
            .expect("construct Linux Bubblewrap command");
    let command = command.as_std();
    assert_eq!(command.get_program(), OsStr::new("/trusted/system/bwrap"));
    let environment = command
        .get_envs()
        .map(|(name, value)| (name.to_os_string(), value.map(OsStr::to_os_string)))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        environment.get(OsStr::new("PATH")),
        Some(&Some(
            OsStr::new("/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin")
                .to_os_string()
        ))
    );
    assert_eq!(
        environment.get(OsStr::new("SHELL")),
        Some(&Some(OsStr::new("/bin/sh").to_os_string()))
    );
    assert_eq!(
        environment.get(OsStr::new("CHATOS_WORKSPACE")),
        Some(&Some(
            spec.workspace_root
                .as_ref()
                .expect("canonical workspace root")
                .as_os_str()
                .to_os_string()
        ))
    );

    std::fs::create_dir(git_root.as_path()).expect("create workspace .git");
    let args = render(linux_sandbox_arguments(&spec).expect("Linux sandbox arguments"));
    assert!(contains_mount(
        args.as_slice(),
        "--ro-bind",
        git_root.as_path()
    ));
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn macos_workspace_profile_allows_approved_write_but_protects_git_and_plugin_root() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let plugin_root = temp.path().join("plugin");
    let state_root = temp.path().join("runtime/state");
    let cache_root = temp.path().join("runtime/cache");
    let temp_root = temp.path().join("runtime/tmp");
    let package_index = temp.path().join("runtime/package-index.json");
    let workspace_root = temp.path().join("workspace");
    for directory in [
        &plugin_root,
        &state_root,
        &cache_root,
        &temp_root,
        &workspace_root,
        &workspace_root.join(".git"),
    ] {
        std::fs::create_dir_all(directory).expect("create sandbox test directory");
    }
    let command = plugin_root.join("hook.sh");
    std::fs::write(
        command.as_path(),
        r#"#!/bin/sh
printf allowed > "$CHATOS_WORKSPACE/allowed.txt" || exit 10
if printf blocked > "$CHATOS_WORKSPACE/.git/blocked.txt"; then exit 11; fi
if printf blocked > "$0"; then exit 12; fi
exit 0
"#,
    )
    .expect("write Hook command");
    std::fs::write(
        package_index.as_path(),
        r#"{"schema_version":1,"files":{}}"#,
    )
    .expect("write package index");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(command.as_path(), std::fs::Permissions::from_mode(0o700))
        .expect("make Hook command executable");
    let spec = PluginStdioSandboxSpec::from_args(vec![
        "--sandbox-id".to_string(),
        "00000000-0000-4000-8000-000000000003".to_string(),
        "--plugin-root".to_string(),
        plugin_root.to_string_lossy().into_owned(),
        "--state-root".to_string(),
        state_root.to_string_lossy().into_owned(),
        "--cache-root".to_string(),
        cache_root.to_string_lossy().into_owned(),
        "--temp-root".to_string(),
        temp_root.to_string_lossy().into_owned(),
        "--package-index".to_string(),
        package_index.to_string_lossy().into_owned(),
        "--workspace-root".to_string(),
        workspace_root.to_string_lossy().into_owned(),
        "--cwd".to_string(),
        plugin_root.to_string_lossy().into_owned(),
        "--".to_string(),
        command.to_string_lossy().into_owned(),
    ])
    .expect("parse writable Hook sandbox");

    let status = super::sandboxed_plugin_command(&spec)
        .expect("construct Seatbelt command")
        .status()
        .await
        .expect("run Seatbelt command");
    assert!(status.success(), "Hook sandbox exited with {status}");
    assert_eq!(
        std::fs::read_to_string(workspace_root.join("allowed.txt"))
            .expect("read approved workspace output"),
        "allowed"
    );
    assert!(!workspace_root.join(".git/blocked.txt").exists());
    assert!(std::fs::read_to_string(command.as_path())
        .expect("read signed Hook command")
        .starts_with("#!/bin/sh"));
}
