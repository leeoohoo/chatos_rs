// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
#[cfg(any(target_os = "linux", test))]
use std::ffi::OsString;
use std::path::{Path, PathBuf};
#[cfg(not(windows))]
use std::process::Stdio;

#[cfg(windows)]
mod windows;
#[cfg(any(windows, test))]
mod windows_command_line;
#[cfg(any(windows, test))]
mod windows_workspace;

const WRAPPER_MODE: &str = "--internal-plugin-stdio-wrapper";

pub(crate) fn is_internal_plugin_stdio_wrapper() -> bool {
    std::env::args().nth(1).as_deref() == Some(WRAPPER_MODE)
}

pub(crate) async fn run_internal_plugin_stdio_wrapper() -> Result<i32, String> {
    let spec = PluginStdioSandboxSpec::from_args(std::env::args().skip(2).collect())?;
    #[cfg(windows)]
    {
        return windows::run(&spec).await;
    }
    #[cfg(not(windows))]
    let mut command = sandboxed_plugin_command(&spec)?;
    #[cfg(not(windows))]
    let status = command.status().await.map_err(|err| err.to_string())?;
    #[cfg(not(windows))]
    Ok(status.code().unwrap_or(1))
}

#[derive(Debug, Clone)]
struct PluginStdioSandboxSpec {
    #[cfg_attr(not(windows), allow(dead_code))]
    sandbox_id: String,
    plugin_root: PathBuf,
    #[cfg_attr(windows, allow(dead_code))]
    state_root: PathBuf,
    #[cfg_attr(windows, allow(dead_code))]
    cache_root: PathBuf,
    #[cfg_attr(windows, allow(dead_code))]
    temp_root: PathBuf,
    #[cfg_attr(not(windows), allow(dead_code))]
    package_index: PathBuf,
    workspace_root: Option<PathBuf>,
    cwd: PathBuf,
    environment_names: Vec<String>,
    command: PathBuf,
    args: Vec<String>,
}

impl PluginStdioSandboxSpec {
    fn from_args(args: Vec<String>) -> Result<Self, String> {
        let separator = args
            .iter()
            .position(|arg| arg == "--")
            .ok_or_else(|| "Plugin stdio wrapper is missing command separator".to_string())?;
        let options = &args[..separator];
        let command_parts = &args[separator + 1..];
        let command = command_parts
            .first()
            .ok_or_else(|| "Plugin stdio wrapper is missing command".to_string())?;
        let mut values = BTreeMap::<String, String>::new();
        let mut environment_names = BTreeSet::new();
        let mut index = 0;
        while index < options.len() {
            let name = options[index].as_str();
            let value = options
                .get(index + 1)
                .ok_or_else(|| format!("Plugin stdio wrapper option has no value: {name}"))?;
            if name == "--env" {
                validate_environment_name(value)?;
                if !environment_names.insert(value.clone()) {
                    return Err(format!(
                        "Plugin stdio wrapper contains duplicate environment name: {value}"
                    ));
                }
            } else if matches!(
                name,
                "--sandbox-id"
                    | "--plugin-root"
                    | "--state-root"
                    | "--cache-root"
                    | "--temp-root"
                    | "--package-index"
                    | "--workspace-root"
                    | "--cwd"
            ) {
                if values.insert(name.to_string(), value.clone()).is_some() {
                    return Err(format!(
                        "Plugin stdio wrapper contains duplicate option: {name}"
                    ));
                }
            } else {
                return Err(format!(
                    "Plugin stdio wrapper option is unsupported: {name}"
                ));
            }
            index += 2;
        }
        let sandbox_id = required_option(&values, "--sandbox-id")?.to_string();
        if uuid::Uuid::parse_str(sandbox_id.as_str()).is_err() {
            return Err("Plugin stdio sandbox ID is invalid".to_string());
        }
        let plugin_root = canonical_directory(required_option(&values, "--plugin-root")?)?;
        let state_root = canonical_directory(required_option(&values, "--state-root")?)?;
        let cache_root = canonical_directory(required_option(&values, "--cache-root")?)?;
        let temp_root = canonical_directory(required_option(&values, "--temp-root")?)?;
        let package_index = canonical_regular_file(
            Path::new(required_option(&values, "--package-index")?),
            false,
        )?;
        let workspace_root = values
            .get("--workspace-root")
            .map(String::as_str)
            .map(canonical_directory)
            .transpose()?;
        let cwd = canonical_directory(required_option(&values, "--cwd")?)?;
        let command = canonical_file(Path::new(command))?;
        if !cwd.starts_with(plugin_root.as_path()) || !command.starts_with(plugin_root.as_path()) {
            return Err("Plugin stdio command and cwd must remain inside Plugin root".to_string());
        }
        for writable in [&state_root, &cache_root, &temp_root] {
            if writable.starts_with(plugin_root.as_path())
                || plugin_root.starts_with(writable.as_path())
            {
                return Err(
                    "Plugin stdio writable runtime roots must be outside Plugin root".to_string(),
                );
            }
        }
        if package_index.starts_with(plugin_root.as_path()) {
            return Err(
                "Plugin stdio signed package index must remain outside Plugin root".to_string(),
            );
        }
        if let Some(workspace_root) = workspace_root.as_ref() {
            for protected in [&plugin_root, &state_root, &cache_root, &temp_root] {
                if workspace_root.starts_with(protected.as_path())
                    || protected.starts_with(workspace_root.as_path())
                {
                    return Err(
                        "Plugin Hook workspace root must not overlap Plugin or runtime roots"
                            .to_string(),
                    );
                }
            }
        }
        Ok(Self {
            sandbox_id,
            plugin_root,
            state_root,
            cache_root,
            temp_root,
            package_index,
            workspace_root,
            cwd,
            environment_names: environment_names.into_iter().collect(),
            command,
            args: command_parts[1..].to_vec(),
        })
    }
}

fn required_option<'a>(
    values: &'a BTreeMap<String, String>,
    name: &str,
) -> Result<&'a str, String> {
    values
        .get(name)
        .map(String::as_str)
        .ok_or_else(|| format!("Plugin stdio wrapper is missing option: {name}"))
}

fn canonical_directory(value: &str) -> Result<PathBuf, String> {
    let path = Path::new(value);
    let metadata = std::fs::symlink_metadata(path).map_err(|err| {
        format!(
            "read Plugin stdio directory {} failed: {err}",
            path.display()
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "Plugin stdio directory is not a non-symlink directory: {}",
            path.display()
        ));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(format!(
                "Plugin stdio directory is a reparse point: {}",
                path.display()
            ));
        }
    }
    path.canonicalize().map_err(|err| {
        format!(
            "canonicalize Plugin stdio directory {} failed: {err}",
            path.display()
        )
    })
}

fn canonical_file(path: &Path) -> Result<PathBuf, String> {
    canonical_regular_file(path, true)
}

fn canonical_regular_file(path: &Path, _require_executable: bool) -> Result<PathBuf, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|err| format!("read Plugin stdio command {} failed: {err}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "Plugin stdio command is not a non-symlink file: {}",
            path.display()
        ));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(format!(
                "Plugin stdio file is a reparse point: {}",
                path.display()
            ));
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if _require_executable && metadata.permissions().mode() & 0o111 == 0 {
            return Err(format!(
                "Plugin stdio command is not executable: {}",
                path.display()
            ));
        }
    }
    path.canonicalize().map_err(|err| {
        format!(
            "canonicalize Plugin stdio command {} failed: {err}",
            path.display()
        )
    })
}

fn validate_environment_name(value: &str) -> Result<(), String> {
    let valid = !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    if !valid {
        return Err("Plugin stdio wrapper environment name is invalid".to_string());
    }
    let normalized = value.to_ascii_uppercase();
    if matches!(
        normalized.as_str(),
        "PATH"
            | "HOME"
            | "SHELL"
            | "TMPDIR"
            | "TMP"
            | "TEMP"
            | "COMSPEC"
            | "PATHEXT"
            | "SYSTEMROOT"
            | "WINDIR"
            | "USERPROFILE"
            | "APPDATA"
            | "LOCALAPPDATA"
            | "CHATOS_PLUGIN_ROOT"
            | "CHATOS_WORKSPACE"
            | "NODE_OPTIONS"
            | "PYTHONHOME"
            | "PYTHONPATH"
            | "RUBYOPT"
            | "PERL5OPT"
            | "BASH_ENV"
            | "ENV"
            | "PROMPT_COMMAND"
    ) || normalized.starts_with("LD_")
        || normalized.starts_with("DYLD_")
        || normalized.starts_with("XDG_")
    {
        return Err("Plugin stdio wrapper environment name is controlled by the Host".to_string());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn sandboxed_plugin_command(
    spec: &PluginStdioSandboxSpec,
) -> Result<tokio::process::Command, String> {
    let mut profile = String::from(include_str!(
        "../../../local_connector_client/core/src/sandbox/process/seatbelt_base_policy.sbpl"
    ));
    profile.push_str(include_str!("restricted_read_only_platform_defaults.sbpl"));
    let mut parameters = Vec::new();
    append_path_rule(
        &mut profile,
        &mut parameters,
        "PLUGIN_ROOT",
        spec.plugin_root.as_path(),
        false,
    );
    for (name, path) in [
        ("STATE_ROOT", &spec.state_root),
        ("CACHE_ROOT", &spec.cache_root),
        ("TEMP_ROOT", &spec.temp_root),
    ] {
        append_path_rule(&mut profile, &mut parameters, name, path.as_path(), true);
    }
    if let Some(workspace_root) = spec.workspace_root.as_ref() {
        append_path_rule(
            &mut profile,
            &mut parameters,
            "WORKSPACE_ROOT",
            workspace_root.as_path(),
            true,
        );
        append_denied_write_path(
            &mut profile,
            &mut parameters,
            "WORKSPACE_GIT_ROOT",
            workspace_root.join(".git"),
        );
    }
    let mut command = tokio::process::Command::new("/usr/bin/sandbox-exec");
    command.arg("-p").arg(profile);
    for (name, value) in parameters {
        command.arg(format!("-D{name}={}", value.to_string_lossy()));
    }
    command
        .arg("--")
        .arg(spec.command.as_path())
        .args(&spec.args)
        .current_dir(spec.cwd.as_path())
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    apply_safe_environment(&mut command, spec)?;
    Ok(command)
}

#[cfg(target_os = "macos")]
fn append_denied_write_path(
    profile: &mut String,
    parameters: &mut Vec<(String, PathBuf)>,
    name: &str,
    path: PathBuf,
) {
    parameters.push((name.to_string(), path));
    profile.push_str(
        format!(
            "\n(deny file-write* (require-any (literal (param \"{name}\")) (subpath (param \"{name}\"))))\n"
        )
        .as_str(),
    );
}

#[cfg(target_os = "linux")]
fn sandboxed_plugin_command(
    spec: &PluginStdioSandboxSpec,
) -> Result<tokio::process::Command, String> {
    let bwrap = trusted_linux_bwrap_executable(spec)?;
    linux_sandboxed_plugin_command_with_launcher(spec, bwrap.as_path())
}

#[cfg(any(target_os = "linux", test))]
fn linux_sandboxed_plugin_command_with_launcher(
    spec: &PluginStdioSandboxSpec,
    bwrap: &Path,
) -> Result<tokio::process::Command, String> {
    let mut command = tokio::process::Command::new(bwrap);
    command
        .args(linux_sandbox_arguments(spec)?)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    apply_linux_safe_environment(&mut command, spec)?;
    Ok(command)
}

#[cfg(any(target_os = "linux", test))]
#[cfg_attr(test, allow(dead_code))]
fn trusted_linux_bwrap_executable(spec: &PluginStdioSandboxSpec) -> Result<PathBuf, String> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let mut candidates = vec![PathBuf::from("/usr/bin/bwrap"), PathBuf::from("/bin/bwrap")];
    if let Some(search_path) = std::env::var_os("PATH") {
        candidates
            .extend(std::env::split_paths(&search_path).map(|directory| directory.join("bwrap")));
    }
    let protected_roots = [
        spec.plugin_root.as_path(),
        spec.state_root.as_path(),
        spec.cache_root.as_path(),
        spec.temp_root.as_path(),
    ];
    let mut visited = BTreeSet::new();
    for candidate in candidates {
        let Ok(candidate) = candidate.canonicalize() else {
            continue;
        };
        if !visited.insert(candidate.clone())
            || protected_roots
                .iter()
                .any(|root| candidate.starts_with(root))
            || spec
                .workspace_root
                .as_ref()
                .is_some_and(|root| candidate.starts_with(root))
        {
            continue;
        }
        let Ok(metadata) = candidate.metadata() else {
            continue;
        };
        if metadata.is_file()
            && metadata.uid() == 0
            && metadata.permissions().mode() & 0o111 != 0
            && metadata.permissions().mode() & 0o022 == 0
        {
            return Ok(candidate);
        }
    }
    Err(
        "Linux Bubblewrap launcher is unavailable or is not a trusted system executable"
            .to_string(),
    )
}

#[cfg(any(target_os = "linux", test))]
fn linux_sandbox_arguments(spec: &PluginStdioSandboxSpec) -> Result<Vec<OsString>, String> {
    let mut args = [
        "--new-session",
        "--die-with-parent",
        "--unshare-user",
        "--unshare-ipc",
        "--unshare-pid",
        "--unshare-uts",
        "--unshare-cgroup-try",
        "--unshare-net",
        "--cap-drop",
        "ALL",
        "--tmpfs",
        "/",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
    ]
    .into_iter()
    .map(OsString::from)
    .collect::<Vec<_>>();
    for path in ["/bin", "/sbin", "/usr", "/etc", "/lib", "/lib64"]
        .into_iter()
        .map(Path::new)
        .filter(|path| path.exists())
    {
        append_linux_mount(&mut args, "--ro-bind", path);
    }
    append_linux_mount(&mut args, "--ro-bind", spec.plugin_root.as_path());
    for path in [&spec.state_root, &spec.cache_root, &spec.temp_root] {
        append_linux_mount(&mut args, "--bind", path.as_path());
    }
    if let Some(workspace_root) = spec.workspace_root.as_ref() {
        append_linux_mount(&mut args, "--bind", workspace_root.as_path());
        let git_root = workspace_root.join(".git");
        match std::fs::symlink_metadata(git_root.as_path()) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "Plugin Hook workspace .git path must not be a symlink: {}",
                    git_root.display()
                ));
            }
            Ok(metadata) if metadata.is_dir() || metadata.is_file() => {
                append_linux_mount(&mut args, "--ro-bind", git_root.as_path());
            }
            Ok(_) => {
                return Err(format!(
                    "Plugin Hook workspace .git path is not a regular file or directory: {}",
                    git_root.display()
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                args.extend([
                    OsString::from("--perms"),
                    OsString::from("555"),
                    OsString::from("--tmpfs"),
                    git_root.as_os_str().to_os_string(),
                    OsString::from("--remount-ro"),
                    git_root.as_os_str().to_os_string(),
                ]);
            }
            Err(error) => {
                return Err(format!(
                    "read Plugin Hook workspace .git path {} failed: {error}",
                    git_root.display()
                ));
            }
        }
    }
    args.push(OsString::from("--chdir"));
    args.push(spec.cwd.as_os_str().to_os_string());
    args.push(OsString::from("--"));
    args.push(spec.command.as_os_str().to_os_string());
    args.extend(spec.args.iter().map(OsString::from));
    Ok(args)
}

#[cfg(any(target_os = "linux", test))]
fn append_linux_mount(args: &mut Vec<OsString>, option: &str, path: &Path) {
    args.push(OsString::from(option));
    args.push(path.as_os_str().to_os_string());
    args.push(path.as_os_str().to_os_string());
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn sandboxed_plugin_command(
    _spec: &PluginStdioSandboxSpec,
) -> Result<tokio::process::Command, String> {
    Err("Plugin stdio OS sandbox is currently supported only on macOS and Linux".to_string())
}

#[cfg(target_os = "macos")]
fn append_path_rule(
    profile: &mut String,
    parameters: &mut Vec<(String, PathBuf)>,
    name: &str,
    path: &Path,
    writable: bool,
) {
    parameters.push((name.to_string(), path.to_path_buf()));
    let operation = if writable {
        "allow file-read* file-write*"
    } else {
        "allow file-read*"
    };
    profile.push_str(
        format!(
            "\n({operation} (require-any (literal (param \"{name}\")) (subpath (param \"{name}\"))))\n"
        )
        .as_str(),
    );
    if !writable {
        profile.push_str(
            format!(
                "(deny file-write* (require-any (literal (param \"{name}\")) (subpath (param \"{name}\"))))\n"
            )
            .as_str(),
        );
    }
    profile.push_str(
        format!(
            "(allow file-read-metadata file-test-existence (path-ancestors (param \"{name}\")))\n"
        )
        .as_str(),
    );
}

#[cfg(target_os = "macos")]
fn apply_safe_environment(
    command: &mut tokio::process::Command,
    spec: &PluginStdioSandboxSpec,
) -> Result<(), String> {
    apply_safe_environment_common(command, spec)?;
    for name in ["PATH", "SHELL"] {
        if let Some(value) = std::env::var_os(name).filter(|value| !value.is_empty()) {
            command.env(name, value);
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn apply_safe_environment(
    command: &mut tokio::process::Command,
    spec: &PluginStdioSandboxSpec,
) -> Result<(), String> {
    apply_linux_safe_environment(command, spec)
}

#[cfg(any(target_os = "linux", test))]
fn apply_linux_safe_environment(
    command: &mut tokio::process::Command,
    spec: &PluginStdioSandboxSpec,
) -> Result<(), String> {
    apply_safe_environment_common(command, spec)?;
    command
        .env(
            "PATH",
            "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        )
        .env("SHELL", "/bin/sh");
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux", test))]
fn apply_safe_environment_common(
    command: &mut tokio::process::Command,
    spec: &PluginStdioSandboxSpec,
) -> Result<(), String> {
    command.env_clear();
    for name in ["LANG", "LC_ALL", "LC_CTYPE", "LOGNAME", "TERM", "USER"] {
        if let Some(value) = std::env::var_os(name).filter(|value| !value.is_empty()) {
            command.env(name, value);
        }
    }
    command
        .env("HOME", spec.state_root.as_os_str())
        .env("TMPDIR", spec.temp_root.as_os_str())
        .env("XDG_CACHE_HOME", spec.cache_root.as_os_str())
        .env("XDG_CONFIG_HOME", spec.state_root.join("config"))
        .env("XDG_STATE_HOME", spec.state_root.join("state"));
    if let Some(workspace_root) = spec.workspace_root.as_ref() {
        command.env("CHATOS_WORKSPACE", workspace_root.as_os_str());
    }
    for name in &spec.environment_names {
        let value = std::env::var_os(name)
            .ok_or_else(|| format!("Plugin stdio environment value is unavailable: {name}"))?;
        command.env(name, value);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        linux_sandbox_arguments, linux_sandboxed_plugin_command_with_launcher,
        PluginStdioSandboxSpec,
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
        let source = include_str!("plugin_stdio_wrapper/windows.rs");
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
}
