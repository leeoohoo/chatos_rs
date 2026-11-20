use super::filesystem::{bounded_file, is_link_or_reparse_point, resolve_non_linked_file};
use super::security::secure_random_token;
use super::{
    plugin_catalog_error, plugin_runtime_error, valid_health_path, ResolvedPluginApplication,
    ResolvedRuntimeContext,
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File},
    io::Read,
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener},
    path::{Path, PathBuf},
};

const MAXIMUM_PACKAGE_JSON_BYTES: u64 = 1_024 * 1_024;
const DEFAULT_HEALTH_PATH: &str = "/api/health";
const DEFAULT_LAUNCH_TIMEOUT_MS: u64 = 15_000;

pub(super) struct PreparedRuntimeProcess {
    pub(super) executable: PathBuf,
    pub(super) arguments: Vec<OsString>,
    pub(super) environment: BTreeMap<String, String>,
    pub(super) installation_root: PathBuf,
    pub(super) _data: PathBuf,
    pub(super) _cache: PathBuf,
    pub(super) base_url: String,
    pub(super) proxy_port: u16,
    pub(super) child_port: u16,
    pub(super) health_path: String,
    pub(super) authentication_token: String,
    pub(super) launch_timeout_ms: u64,
    #[cfg(windows)]
    pub(super) network_policy: &'static str,
}

#[derive(Deserialize)]
struct PackageManifest {
    name: String,
    bin: Value,
}

pub(super) fn prepare_runtime_process(
    resolved: &ResolvedPluginApplication,
    context: &ResolvedRuntimeContext,
) -> Result<PreparedRuntimeProcess, String> {
    let runtime = resolved
        .contribution
        .runtime
        .as_ref()
        .ok_or_else(plugin_runtime_error)?;
    let (executable, prefix_arguments) =
        resolve_package_executable(&resolved.installation_root, &runtime.bin)?;
    let child_port = available_loopback_port()?;
    let mut proxy_port = available_loopback_port()?;
    while proxy_port == child_port {
        proxy_port = available_loopback_port()?;
    }
    let base_url = format!("http://127.0.0.1:{proxy_port}/");
    let health_path = runtime
        .health_path
        .clone()
        .unwrap_or_else(|| DEFAULT_HEALTH_PATH.into());
    if !valid_health_path(&health_path) {
        return Err(plugin_catalog_error());
    }
    let authentication_token = secure_random_token()?;
    let mut environment = safe_process_environment();
    environment.extend(context.environment.clone());
    let sandbox_label = if cfg!(windows) {
        "windows-appcontainer"
    } else {
        "windows-job-lifecycle"
    };
    environment.extend([
        (
            "CHATOS_PLUGIN_ROOT".into(),
            resolved.installation_root.to_string_lossy().into_owned(),
        ),
        (
            "CHATOS_PLUGIN_DATA_DIR".into(),
            context.data_directory.to_string_lossy().into_owned(),
        ),
        (
            "CHATOS_PLUGIN_CACHE_DIR".into(),
            context.cache_directory.to_string_lossy().into_owned(),
        ),
        ("CHATOS_PLUGIN_APP_HOST".into(), "127.0.0.1".into()),
        ("CHATOS_PLUGIN_APP_PORT".into(), child_port.to_string()),
        ("CHATOS_PLUGIN_APP_HEALTH_PATH".into(), health_path.clone()),
        (
            "CHATOS_PLUGIN_APP_TOKEN".into(),
            authentication_token.clone(),
        ),
        (
            "CHATOS_PLUGIN_ID".into(),
            resolved.record.spec.plugin_id.clone(),
        ),
        (
            "CHATOS_PLUGIN_COMPONENT_KEY".into(),
            resolved.contribution.component_key.clone(),
        ),
        (
            "CHATOS_PLUGIN_RELEASE_ID".into(),
            resolved.record.spec.release_id.clone(),
        ),
        (
            "CHATOS_PLUGIN_VERSION".into(),
            resolved.manifest.version.clone(),
        ),
        (
            "CHATOS_PLUGIN_ARTIFACT_SHA256".into(),
            resolved.record.spec.release_digest.clone(),
        ),
        ("CHATOS_PLUGIN_FILE_WATCH_MODE".into(), "polling".into()),
        ("CHATOS_PLUGIN_SANDBOX".into(), sandbox_label.into()),
    ]);
    let arguments = prefix_arguments
        .into_iter()
        .chain(runtime.args.iter().map(OsString::from))
        .collect();
    #[cfg(windows)]
    let network_policy = if resolved.manifest.permissions.iter().any(|permission| {
        permission.permission == "network.client"
            && permission.required
            && (permission.components.is_empty()
                || permission
                    .components
                    .contains(&resolved.contribution.component_key))
    }) {
        "controlled"
    } else {
        "loopback"
    };
    Ok(PreparedRuntimeProcess {
        executable,
        arguments,
        environment,
        installation_root: resolved.installation_root.clone(),
        _data: context.data_directory.clone(),
        _cache: context.cache_directory.clone(),
        base_url,
        proxy_port,
        child_port,
        health_path,
        authentication_token,
        launch_timeout_ms: runtime
            .launch_timeout_ms
            .unwrap_or(DEFAULT_LAUNCH_TIMEOUT_MS),
        #[cfg(windows)]
        network_policy,
    })
}

pub(super) fn resolve_package_executable(
    installation_root: &Path,
    bin_name: &str,
) -> Result<(PathBuf, Vec<OsString>), String> {
    let package_path = resolve_non_linked_file(installation_root, "package.json")?;
    let package = serde_json::from_slice::<PackageManifest>(&bounded_file(
        &package_path,
        MAXIMUM_PACKAGE_JSON_BYTES,
    )?)
    .map_err(|_| "Plugin package.json 无效。".to_owned())?;
    let target = match &package.bin {
        Value::Object(bins) => bins.get(bin_name).and_then(Value::as_str),
        Value::String(target)
            if package
                .name
                .split('/')
                .next_back()
                .is_some_and(|name| name == bin_name) =>
        {
            Some(target.as_str())
        }
        _ => None,
    }
    .ok_or_else(|| "Plugin 应用入口未在 package.json.bin 中声明。".to_owned())?;
    let target = resolve_non_linked_file(installation_root, target)?;
    if is_node_script(&target)? {
        let node = find_system_executable("node")
            .ok_or_else(|| "Plugin 应用需要本机 Node.js 运行时。".to_owned())?;
        return Ok((node, vec![target.into_os_string()]));
    }
    #[cfg(windows)]
    if !target
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
    {
        return Err("Plugin 应用入口不是可执行文件。".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if target
            .metadata()
            .map(|metadata| metadata.permissions().mode() & 0o111 == 0)
            .unwrap_or(true)
        {
            return Err("Plugin 应用入口不是可执行文件。".into());
        }
    }
    Ok((target, Vec::new()))
}

fn is_node_script(path: &Path) -> Result<bool, String> {
    let extension_is_node = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "js" | "mjs" | "cjs"
            )
        });
    if extension_is_node {
        return Ok(true);
    }
    let mut prefix = [0_u8; 256];
    let count = File::open(path)
        .and_then(|mut file| file.read(&mut prefix))
        .map_err(|_| plugin_runtime_error())?;
    let header = String::from_utf8_lossy(&prefix[..count]);
    Ok(header.starts_with("#!")
        && header
            .lines()
            .next()
            .is_some_and(|line| line.contains("node")))
}

fn find_system_executable(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let extensions: &[&str] = if cfg!(windows) { &[".exe", ""] } else { &[""] };
    for directory in std::env::split_paths(&path) {
        for extension in extensions {
            let candidate = directory.join(format!("{name}{extension}"));
            let Ok(metadata) = fs::symlink_metadata(&candidate) else {
                continue;
            };
            if !metadata.is_file() || is_link_or_reparse_point(&metadata) {
                continue;
            }
            let Ok(canonical) = candidate.canonicalize() else {
                continue;
            };
            if canonical
                .metadata()
                .ok()
                .is_some_and(|metadata| metadata.is_file())
            {
                return Some(canonical);
            }
        }
    }
    None
}

fn safe_process_environment() -> BTreeMap<String, String> {
    const ALLOWED: &[&str] = &[
        "APPDATA",
        "ComSpec",
        "HOME",
        "LANG",
        "LC_ALL",
        "LOCALAPPDATA",
        "PATH",
        "PATHEXT",
        "PROGRAMDATA",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "SHELL",
        "SystemRoot",
        "TEMP",
        "TMP",
        "TMPDIR",
        "USER",
        "USERPROFILE",
        "WINDIR",
    ];
    ALLOWED
        .iter()
        .filter_map(|name| {
            std::env::var(name)
                .ok()
                .map(|value| ((*name).into(), value))
        })
        .collect()
}

pub(super) fn available_loopback_port() -> Result<u16, String> {
    let listener = TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
        .map_err(|_| "无法分配 Plugin 应用端口。".to_owned())?;
    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(|_| "无法读取 Plugin 应用端口。".to_owned())
}
