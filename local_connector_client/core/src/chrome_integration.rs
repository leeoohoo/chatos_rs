// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tempfile::NamedTempFile;
use url::Url;
use uuid::Uuid;

use crate::chrome_bridge::{
    chrome_bridge_status, ChromeBridgeStatus, CHROME_EXTENSION_ID, CHROME_EXTENSION_ORIGIN,
    CHROME_EXTENSION_VERSION, CHROME_NATIVE_HOST_NAME, CHROME_NATIVE_PROTOCOL_VERSION,
};
use crate::config::{home_dir, optional_env};

const CHROME_NATIVE_HOST_FILE: &str = "com.chatos.chrome.json";
const CHROME_EXTENSION_MANIFEST_FILE: &str = "manifest.json";
const CHROME_RENDEZVOUS_FILE: &str = "chrome-native-host.json";
const CHROME_HOST_DESCRIPTION: &str = "ChatOS Chrome Native Messaging Host";
#[cfg(any(target_os = "windows", test))]
const CHROME_WINDOWS_REGISTRY_SUBKEY: &str =
    r"Software\Google\Chrome\NativeMessagingHosts\com.chatos.chrome";
const CHROME_EXTENSION_PUBLIC_KEY: &str = "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAwtyBLDERxm2J31roRxBzHGFmtn03x51KFG7KLXkLNzNVaEnk6Np4ZnQMiu7ADkVykLoDtBUZCcJ5/Ol7Ceo9eYGOdtKp1KPpW5tM16vj+y0NkwOi27Ofr9ak0P3MvHQnJjAFOHd/vOSF8El94VV6A6iWuhlGSbnvbj+oZ+w3RWQkqKiXr/Qkd77DvvJhQghcz0V5JhVqrMANxOW1kPDVPIZvPfrxh4+LX4jrzPSLzgQcsG6q6M4dkdIH7UeymQv12XVdP2UtSrLyTRC2MpzuohQmau334GnZAGfkfg9ODXbrVdlabFb4JnhZHVCEoMwNI0wNhbkTlxG1bhZlgQTQawIDAQAB";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChromeHostPlatform {
    Macos,
    Windows,
    Unsupported,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ChromeIntegrationStatus {
    pub(crate) platform_supported: bool,
    pub(crate) enabled: bool,
    pub(crate) native_host_available: bool,
    pub(crate) native_host_manifest_path: Option<String>,
    pub(crate) extension_available: bool,
    pub(crate) extension_directory: Option<String>,
    pub(crate) extension_id: String,
    pub(crate) bridge: ChromeBridgeStatus,
    pub(crate) setup_note: String,
    pub(crate) last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChromeNativeHostManifest {
    name: String,
    description: String,
    path: String,
    #[serde(rename = "type")]
    transport_type: String,
    allowed_origins: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ChromeNativeRendezvous {
    pub(crate) schema_version: u32,
    pub(crate) instance_id: String,
    pub(crate) api_base_url: String,
    pub(crate) auth_token: String,
    pub(crate) extension_origin: String,
    pub(crate) protocol_version: u32,
    pub(crate) core_pid: u32,
    pub(crate) written_at: String,
}

#[derive(Debug)]
pub(crate) struct ChromeRendezvousGuard {
    path: PathBuf,
    instance_id: String,
}

impl Drop for ChromeRendezvousGuard {
    fn drop(&mut self) {
        let current = fs::read_to_string(self.path.as_path())
            .ok()
            .and_then(|text| serde_json::from_str::<ChromeNativeRendezvous>(&text).ok());
        if current
            .as_ref()
            .is_some_and(|value| value.instance_id == self.instance_id)
        {
            let _ = fs::remove_file(self.path.as_path());
        }
    }
}

pub(crate) fn chrome_integration_status() -> ChromeIntegrationStatus {
    let platform = chrome_host_platform();
    let platform_supported = platform != ChromeHostPlatform::Unsupported;
    let host = chrome_native_host_path().and_then(|path| validate_host_binary(&path).map(|_| path));
    let extension = chrome_extension_directory()
        .and_then(|path| validate_extension_directory(&path).map(|_| path));
    let manifest_path = chrome_native_host_manifest_path();
    let registration = manifest_path
        .as_ref()
        .map(|path| registration_matches(path.as_path()))
        .transpose();
    let bridge = chrome_bridge_status().unwrap_or_else(|_| ChromeBridgeStatus {
        connected: false,
        extension_id: CHROME_EXTENSION_ID.to_string(),
        extension_version: None,
        extension_compatible: false,
        connected_at_ms: None,
        last_seen_at_ms: None,
        claimed_tab_count: 0,
        authorized_origin_count: 0,
        pending_command_count: 0,
    });
    let last_error = if !platform_supported {
        Some(
            "Chrome Native Messaging setup is currently implemented for macOS and Windows only"
                .to_string(),
        )
    } else if let Err(error) = &host {
        Some(error.to_string())
    } else if let Err(error) = &extension {
        Some(error.to_string())
    } else if let Err(error) = &registration {
        Some(error.to_string())
    } else {
        None
    };
    let enabled = registration
        .as_ref()
        .is_ok_and(|value| value == &Some(true));
    let setup_note = if bridge.connected && !bridge.extension_compatible {
        format!(
            "Chrome 扩展版本不兼容，请从 Local Connector 重新加载 {CHROME_EXTENSION_VERSION} 版本。"
        )
    } else if enabled {
        match platform {
            ChromeHostPlatform::Windows => "Native Host 已注册到当前 Windows 用户。请在 Chrome 扩展页加载 ChatOS 扩展，再从扩展弹窗逐站点授权并连接标签页。".to_string(),
            _ => "Native Host 已注册。请在 Chrome 扩展页加载 ChatOS 扩展，再从扩展弹窗逐站点授权并连接标签页。".to_string(),
        }
    } else {
        "启用后只注册用户级 Native Host；Chrome 扩展仍需由用户手动加载，站点访问也必须在扩展弹窗中逐站点确认。"
            .to_string()
    };
    ChromeIntegrationStatus {
        platform_supported,
        enabled,
        native_host_available: host.is_ok(),
        native_host_manifest_path: manifest_path.map(|path| path.to_string_lossy().to_string()),
        extension_available: extension.is_ok(),
        extension_directory: extension
            .ok()
            .map(|path| path.to_string_lossy().to_string()),
        extension_id: CHROME_EXTENSION_ID.to_string(),
        bridge,
        setup_note,
        last_error,
    }
}

pub(crate) fn enable_chrome_integration(
    risk_acknowledged: bool,
) -> Result<ChromeIntegrationStatus> {
    if !risk_acknowledged {
        bail!("enabling Chrome integration requires explicit risk acknowledgement");
    }
    let platform = chrome_host_platform();
    if platform == ChromeHostPlatform::Unsupported {
        bail!("Chrome Native Messaging setup is currently implemented for macOS and Windows only");
    }
    let host_path = chrome_native_host_path()?;
    validate_host_binary(host_path.as_path())?;
    let extension_dir = chrome_extension_directory()?;
    validate_extension_directory(extension_dir.as_path())?;
    let manifest_path = chrome_native_host_manifest_path()
        .ok_or_else(|| anyhow!("Chrome Native Messaging manifest directory is unavailable"))?;
    let manifest_existed = manifest_path.exists();
    if manifest_existed && !registration_owned_by_chatos(manifest_path.as_path())? {
        bail!("an unrelated Chrome Native Messaging manifest already uses the ChatOS host name");
    }
    let previous_manifest = if manifest_existed {
        Some(read_native_host_manifest(manifest_path.as_path())?)
    } else {
        None
    };
    if platform == ChromeHostPlatform::Windows {
        ensure_windows_registration_available(manifest_path.as_path())?;
    }
    let manifest = ChromeNativeHostManifest {
        name: CHROME_NATIVE_HOST_NAME.to_string(),
        description: CHROME_HOST_DESCRIPTION.to_string(),
        path: host_path
            .canonicalize()
            .context("canonicalize Chrome Native Host path")?
            .to_string_lossy()
            .to_string(),
        transport_type: "stdio".to_string(),
        allowed_origins: vec![CHROME_EXTENSION_ORIGIN.to_string()],
    };
    write_private_json(manifest_path.as_path(), &manifest)?;
    if platform == ChromeHostPlatform::Windows {
        if let Err(error) = register_windows_native_host(manifest_path.as_path()) {
            if let Some(previous_manifest) = previous_manifest.as_ref() {
                let _ = write_private_json(manifest_path.as_path(), previous_manifest);
            } else {
                let _ = fs::remove_file(manifest_path.as_path());
            }
            return Err(error);
        }
    }
    Ok(chrome_integration_status())
}

pub(crate) fn disable_chrome_integration() -> Result<ChromeIntegrationStatus> {
    let platform = chrome_host_platform();
    if let Some(path) = chrome_native_host_manifest_path() {
        if path.exists() {
            if !registration_owned_by_chatos(path.as_path())? {
                bail!("refusing to remove a Chrome Native Messaging manifest not owned by ChatOS");
            }
            if platform == ChromeHostPlatform::Windows {
                ensure_windows_registration_available(path.as_path())?;
                unregister_windows_native_host()?;
            }
            fs::remove_file(path.as_path()).with_context(|| {
                format!("remove Chrome Native Host manifest {}", path.display())
            })?;
        } else if platform == ChromeHostPlatform::Windows {
            ensure_windows_registration_available(path.as_path())?;
            unregister_windows_native_host()?;
        }
    }
    Ok(chrome_integration_status())
}

pub(crate) fn publish_chrome_native_rendezvous(
    state_path: &Path,
    auth_token: &str,
    api_port: u16,
) -> Result<ChromeRendezvousGuard> {
    if auth_token.len() < 32 || auth_token.len() > 512 || auth_token.chars().any(char::is_control) {
        bail!("Local Connector desktop authentication token is invalid for Chrome rendezvous");
    }
    let api_base_url = format!("http://127.0.0.1:{api_port}");
    validate_loopback_api_base(api_base_url.as_str())?;
    let path = chrome_rendezvous_path(state_path)?;
    let instance_id = Uuid::new_v4().to_string();
    let rendezvous = ChromeNativeRendezvous {
        schema_version: 1,
        instance_id: instance_id.clone(),
        api_base_url,
        auth_token: auth_token.to_string(),
        extension_origin: CHROME_EXTENSION_ORIGIN.to_string(),
        protocol_version: CHROME_NATIVE_PROTOCOL_VERSION,
        core_pid: std::process::id(),
        written_at: chrono::Utc::now().to_rfc3339(),
    };
    write_private_json(path.as_path(), &rendezvous)?;
    Ok(ChromeRendezvousGuard { path, instance_id })
}

pub(crate) fn load_chrome_native_rendezvous(path: &Path) -> Result<ChromeNativeRendezvous> {
    validate_private_file(path)?;
    let bytes =
        fs::read(path).with_context(|| format!("read Chrome rendezvous {}", path.display()))?;
    if bytes.len() > 16 * 1024 {
        bail!("Chrome rendezvous file exceeded the safety limit");
    }
    let rendezvous = serde_json::from_slice::<ChromeNativeRendezvous>(&bytes)
        .context("decode Chrome rendezvous file")?;
    if rendezvous.schema_version != 1
        || rendezvous.extension_origin != CHROME_EXTENSION_ORIGIN
        || rendezvous.protocol_version != CHROME_NATIVE_PROTOCOL_VERSION
    {
        bail!("Chrome rendezvous identity or protocol is invalid");
    }
    if rendezvous.auth_token.len() < 32
        || rendezvous.auth_token.len() > 512
        || rendezvous.auth_token.chars().any(char::is_control)
    {
        bail!("Chrome rendezvous authentication token is invalid");
    }
    validate_loopback_api_base(rendezvous.api_base_url.as_str())?;
    Ok(rendezvous)
}

pub(crate) fn default_chrome_rendezvous_path() -> Result<PathBuf> {
    if let Some(path) = optional_env("CHATOS_CHROME_NATIVE_RENDEZVOUS") {
        return Ok(PathBuf::from(path));
    }
    let state_path = crate::config::default_state_path();
    chrome_rendezvous_path(state_path.as_path())
}

fn chrome_rendezvous_path(state_path: &Path) -> Result<PathBuf> {
    state_path
        .parent()
        .map(|parent| parent.join(CHROME_RENDEZVOUS_FILE))
        .ok_or_else(|| anyhow!("Local Connector state directory is unavailable"))
}

fn chrome_host_platform() -> ChromeHostPlatform {
    #[cfg(target_os = "macos")]
    {
        ChromeHostPlatform::Macos
    }
    #[cfg(target_os = "windows")]
    {
        ChromeHostPlatform::Windows
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        ChromeHostPlatform::Unsupported
    }
}

fn chrome_native_host_file_name(platform: ChromeHostPlatform) -> &'static str {
    match platform {
        ChromeHostPlatform::Windows => "chatos_chrome_native_host.exe",
        ChromeHostPlatform::Macos | ChromeHostPlatform::Unsupported => "chatos_chrome_native_host",
    }
}

fn chrome_native_host_path() -> Result<PathBuf> {
    if let Some(path) = optional_env("CHATOS_CHROME_NATIVE_HOST_PATH") {
        return Ok(PathBuf::from(path));
    }
    let executable = std::env::current_exe().context("resolve Local Connector executable")?;
    let parent = executable
        .parent()
        .ok_or_else(|| anyhow!("Local Connector executable directory is unavailable"))?;
    Ok(parent.join(chrome_native_host_file_name(chrome_host_platform())))
}

fn chrome_extension_directory() -> Result<PathBuf> {
    if let Some(path) = optional_env("CHATOS_CHROME_EXTENSION_DIR") {
        return Ok(PathBuf::from(path));
    }
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    Ok(manifest_dir
        .parent()
        .ok_or_else(|| anyhow!("Local Connector source root is unavailable"))?
        .join("chrome_extension"))
}

fn chrome_native_host_manifest_path() -> Option<PathBuf> {
    home_dir().and_then(|home| {
        chrome_native_host_manifest_path_for(chrome_host_platform(), home.as_path())
    })
}

fn chrome_native_host_manifest_path_for(
    platform: ChromeHostPlatform,
    home: &Path,
) -> Option<PathBuf> {
    match platform {
        ChromeHostPlatform::Macos => Some(
            home.join("Library")
                .join("Application Support")
                .join("Google")
                .join("Chrome")
                .join("NativeMessagingHosts")
                .join(CHROME_NATIVE_HOST_FILE),
        ),
        ChromeHostPlatform::Windows => Some(
            home.join(".chatos")
                .join("local_connector")
                .join("chrome-native-messaging")
                .join(CHROME_NATIVE_HOST_FILE),
        ),
        ChromeHostPlatform::Unsupported => None,
    }
}

fn validate_host_binary(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("Chrome Native Host is missing: {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("Chrome Native Host must be a regular non-symlink file");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            bail!("Chrome Native Host is not executable");
        }
    }
    Ok(())
}

fn validate_extension_directory(path: &Path) -> Result<()> {
    let manifest_path = path.join(CHROME_EXTENSION_MANIFEST_FILE);
    let bytes = fs::read(manifest_path.as_path()).with_context(|| {
        format!(
            "Chrome extension manifest is missing: {}",
            manifest_path.display()
        )
    })?;
    if bytes.len() > 128 * 1024 {
        bail!("Chrome extension manifest exceeded the safety limit");
    }
    let manifest =
        serde_json::from_slice::<Value>(&bytes).context("decode Chrome extension manifest")?;
    if manifest.get("manifest_version").and_then(Value::as_u64) != Some(3)
        || manifest.get("version").and_then(Value::as_str) != Some(CHROME_EXTENSION_VERSION)
        || manifest.get("key").and_then(Value::as_str) != Some(CHROME_EXTENSION_PUBLIC_KEY)
    {
        bail!("Chrome extension manifest identity is invalid");
    }
    Ok(())
}

fn ensure_windows_registration_available(expected_manifest_path: &Path) -> Result<()> {
    if let Some(registered_path) = windows_registered_manifest_path()? {
        if !windows_registration_paths_match(expected_manifest_path, registered_path.as_path())? {
            bail!("an unrelated Windows Chrome Native Messaging registration already uses the ChatOS host name");
        }
    }
    Ok(())
}

fn windows_registration_paths_match(expected: &Path, registered: &Path) -> Result<bool> {
    let expected = normalized_windows_registration_path(expected)?;
    let registered = normalized_windows_registration_path(registered)?;
    Ok(expected == registered)
}

fn normalized_windows_registration_path(path: &Path) -> Result<String> {
    let value = path.to_string_lossy();
    if value.is_empty() || value.len() > 32_767 || value.chars().any(char::is_control) {
        bail!("Windows Chrome Native Messaging manifest path is invalid");
    }
    Ok(value.replace('/', "\\").to_lowercase())
}

#[cfg(target_os = "windows")]
fn windows_registered_manifest_path() -> Result<Option<PathBuf>> {
    use std::io::ErrorKind;
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let current_user = RegKey::predef(HKEY_CURRENT_USER);
    let key = match current_user.open_subkey(CHROME_WINDOWS_REGISTRY_SUBKEY) {
        Ok(key) => key,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).context("open Windows Chrome Native Messaging registration")
        }
    };
    let value = key
        .get_value::<String, _>("")
        .context("read Windows Chrome Native Messaging registration")?;
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        bail!("Windows Chrome Native Messaging registration path must be absolute");
    }
    normalized_windows_registration_path(path.as_path())?;
    Ok(Some(path))
}

#[cfg(not(target_os = "windows"))]
fn windows_registered_manifest_path() -> Result<Option<PathBuf>> {
    bail!("Windows Chrome Native Messaging registration is unavailable on this platform")
}

#[cfg(target_os = "windows")]
fn register_windows_native_host(manifest_path: &Path) -> Result<()> {
    use winreg::enums::{HKEY_CURRENT_USER, REG_CREATED_NEW_KEY};
    use winreg::RegKey;

    let manifest_path = manifest_path
        .canonicalize()
        .context("canonicalize Windows Chrome Native Messaging manifest path")?;
    let value = manifest_path.to_string_lossy().to_string();
    normalized_windows_registration_path(manifest_path.as_path())?;
    let current_user = RegKey::predef(HKEY_CURRENT_USER);
    let (key, disposition) = current_user
        .create_subkey(CHROME_WINDOWS_REGISTRY_SUBKEY)
        .context("create Windows Chrome Native Messaging registration")?;
    if let Err(error) = key.set_value("", &value) {
        drop(key);
        if disposition == REG_CREATED_NEW_KEY {
            let _ = current_user.delete_subkey(CHROME_WINDOWS_REGISTRY_SUBKEY);
        }
        return Err(error).context("write Windows Chrome Native Messaging registration");
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn register_windows_native_host(_manifest_path: &Path) -> Result<()> {
    bail!("Windows Chrome Native Messaging registration is unavailable on this platform")
}

#[cfg(target_os = "windows")]
fn unregister_windows_native_host() -> Result<()> {
    use std::io::ErrorKind;
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let current_user = RegKey::predef(HKEY_CURRENT_USER);
    match current_user.delete_subkey(CHROME_WINDOWS_REGISTRY_SUBKEY) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).context("remove Windows Chrome Native Messaging registration"),
    }
}

#[cfg(not(target_os = "windows"))]
fn unregister_windows_native_host() -> Result<()> {
    bail!("Windows Chrome Native Messaging registration is unavailable on this platform")
}

fn registration_matches(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    if !registration_owned_by_chatos(path)? {
        return Ok(false);
    }
    let manifest = read_native_host_manifest(path)?;
    let expected_path = chrome_native_host_path()?
        .canonicalize()
        .context("canonicalize expected Chrome Native Host path")?;
    let registered_path = PathBuf::from(manifest.path)
        .canonicalize()
        .context("canonicalize registered Chrome Native Host path")?;
    if expected_path != registered_path {
        return Ok(false);
    }
    if chrome_host_platform() == ChromeHostPlatform::Windows {
        let Some(registered_manifest_path) = windows_registered_manifest_path()? else {
            return Ok(false);
        };
        return windows_registration_paths_match(path, registered_manifest_path.as_path());
    }
    Ok(true)
}

fn registration_owned_by_chatos(path: &Path) -> Result<bool> {
    let manifest = read_native_host_manifest(path)?;
    Ok(manifest.name == CHROME_NATIVE_HOST_NAME
        && manifest.description == CHROME_HOST_DESCRIPTION
        && manifest.transport_type == "stdio"
        && manifest.allowed_origins == [CHROME_EXTENSION_ORIGIN])
}

fn read_native_host_manifest(path: &Path) -> Result<ChromeNativeHostManifest> {
    let bytes = fs::read(path)
        .with_context(|| format!("read Chrome Native Host manifest {}", path.display()))?;
    if bytes.len() > 64 * 1024 {
        bail!("Chrome Native Host manifest exceeded the safety limit");
    }
    serde_json::from_slice(&bytes).context("decode Chrome Native Host manifest")
}

fn write_private_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("private JSON parent directory is unavailable"))?;
    fs::create_dir_all(parent).with_context(|| format!("create directory {}", parent.display()))?;
    let mut temp = NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary file in {}", parent.display()))?;
    serde_json::to_writer_pretty(temp.as_file_mut(), value).context("encode private JSON")?;
    temp.as_file_mut().sync_all().context("sync private JSON")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o600))
            .context("restrict private JSON permissions")?;
    }
    temp.persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("publish private JSON {}", path.display()))?;
    Ok(())
}

fn validate_private_file(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("Chrome rendezvous is missing: {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("Chrome rendezvous must be a regular non-symlink file");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.uid() != unsafe { libc::geteuid() }
            || metadata.permissions().mode() & 0o077 != 0
        {
            bail!("Chrome rendezvous ownership or permissions are unsafe");
        }
    }
    Ok(())
}

fn validate_loopback_api_base(value: &str) -> Result<()> {
    let url = Url::parse(value).context("parse Chrome rendezvous API URL")?;
    if url.scheme() != "http"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
        || !url
            .host_str()
            .is_some_and(|host| host == "localhost" || host == "::1" || host.starts_with("127."))
        || url.port().is_none()
    {
        bail!("Chrome rendezvous API URL must be an explicit loopback HTTP origin");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_host_manifest_identity_is_exact() {
        let manifest = ChromeNativeHostManifest {
            name: CHROME_NATIVE_HOST_NAME.to_string(),
            description: CHROME_HOST_DESCRIPTION.to_string(),
            path: "/Applications/ChatOS/chatos_chrome_native_host".to_string(),
            transport_type: "stdio".to_string(),
            allowed_origins: vec![CHROME_EXTENSION_ORIGIN.to_string()],
        };
        assert_eq!(manifest.allowed_origins, [CHROME_EXTENSION_ORIGIN]);
        assert_eq!(manifest.transport_type, "stdio");
    }

    #[test]
    fn native_host_paths_are_platform_specific_and_user_scoped() {
        let home = Path::new("/Users/example");
        let macos = chrome_native_host_manifest_path_for(ChromeHostPlatform::Macos, home)
            .expect("macOS manifest path");
        let windows = chrome_native_host_manifest_path_for(ChromeHostPlatform::Windows, home)
            .expect("Windows manifest path");
        assert!(macos.ends_with(Path::new(
            "Library/Application Support/Google/Chrome/NativeMessagingHosts/com.chatos.chrome.json"
        )));
        assert!(windows.ends_with(Path::new(
            ".chatos/local_connector/chrome-native-messaging/com.chatos.chrome.json"
        )));
        assert_eq!(
            chrome_native_host_file_name(ChromeHostPlatform::Windows),
            "chatos_chrome_native_host.exe"
        );
        assert_eq!(
            chrome_native_host_manifest_path_for(ChromeHostPlatform::Unsupported, home),
            None
        );
    }

    #[test]
    fn windows_registry_binding_is_exact_but_case_and_separator_insensitive() {
        assert_eq!(
            CHROME_WINDOWS_REGISTRY_SUBKEY,
            r"Software\Google\Chrome\NativeMessagingHosts\com.chatos.chrome"
        );
        assert!(windows_registration_paths_match(
            Path::new(r"C:\Users\Example\ChatOS\com.chatos.chrome.json"),
            Path::new("c:/users/example/chatos/com.chatos.chrome.json"),
        )
        .expect("matching Windows registration paths"));
        assert!(!windows_registration_paths_match(
            Path::new(r"C:\Users\Example\ChatOS\com.chatos.chrome.json"),
            Path::new(r"C:\Other\com.chatos.chrome.json"),
        )
        .expect("different Windows registration paths"));
        assert!(normalized_windows_registration_path(Path::new("bad\npath")).is_err());
    }

    #[test]
    fn rendezvous_accepts_only_explicit_loopback_http_origins() {
        assert!(validate_loopback_api_base("http://127.0.0.1:39232/").is_ok());
        assert!(validate_loopback_api_base("https://127.0.0.1:39232/").is_err());
        assert!(validate_loopback_api_base("http://example.com:39232/").is_err());
        assert!(validate_loopback_api_base("http://user:secret@127.0.0.1:39232/").is_err());
    }
}
