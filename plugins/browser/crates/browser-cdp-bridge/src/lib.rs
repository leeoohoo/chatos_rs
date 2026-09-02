mod install;
mod native;
mod server;
mod wire;

use serde::{Deserialize, Serialize};
use std::{
    env,
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub use install::{
    NATIVE_HOST_NAME, NativeHostInstallation, install_native_host, uninstall_native_host,
};
pub use native::run_native_host;
pub use server::{BridgeReady, BridgeServer, BridgeServerConfig};

pub const STATE_FILE_NAME: &str = "bridge-state.json";
pub const STATE_LOCATOR_FILE_NAME: &str = "active-bridge.json";

#[derive(Debug, Serialize, Deserialize)]
struct BridgeStateLocator {
    state_file: PathBuf,
}

pub async fn install_active_bridge_locator(
    locator_dir: &Path,
    state_file: &Path,
) -> Result<PathBuf, String> {
    if !state_file.is_absolute() {
        return Err("Browser Bridge state file must be absolute".to_string());
    }
    tokio::fs::create_dir_all(locator_dir)
        .await
        .map_err(|error| format!("could not create Browser Bridge locator directory: {error}"))?;
    let locator_file = locator_dir.join(STATE_LOCATOR_FILE_NAME);
    let temporary = locator_dir.join(format!(
        ".{STATE_LOCATOR_FILE_NAME}.{}.tmp",
        Uuid::new_v4().simple()
    ));
    let mut bytes = serde_json::to_vec_pretty(&BridgeStateLocator {
        state_file: state_file.to_path_buf(),
    })
    .map_err(|error| format!("could not serialize Browser Bridge locator: {error}"))?;
    bytes.push(b'\n');
    tokio::fs::write(&temporary, bytes)
        .await
        .map_err(|error| format!("could not write Browser Bridge locator: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o600))
            .await
            .map_err(|error| format!("could not secure Browser Bridge locator: {error}"))?;
    }
    if tokio::fs::try_exists(&locator_file).await.unwrap_or(false) {
        tokio::fs::remove_file(&locator_file)
            .await
            .map_err(|error| format!("could not replace Browser Bridge locator: {error}"))?;
    }
    tokio::fs::rename(&temporary, &locator_file)
        .await
        .map_err(|error| format!("could not install Browser Bridge locator: {error}"))?;
    Ok(locator_file)
}

pub async fn resolve_active_bridge_state(locator_dir: &Path) -> Result<PathBuf, String> {
    let locator_file = locator_dir.join(STATE_LOCATOR_FILE_NAME);
    let bytes = tokio::fs::read(&locator_file)
        .await
        .map_err(|_| "Browser Bridge is not running".to_string())?;
    if bytes.len() > 64 * 1024 {
        return Err("Browser Bridge locator is invalid".to_string());
    }
    let locator: BridgeStateLocator = serde_json::from_slice(&bytes)
        .map_err(|_| "Browser Bridge locator is invalid".to_string())?;
    if !locator.state_file.is_absolute() {
        return Err("Browser Bridge locator state path is invalid".to_string());
    }
    Ok(locator.state_file)
}

pub async fn remove_active_bridge_locator_if_owned(locator_file: &Path, state_file: &Path) {
    let Ok(bytes) = tokio::fs::read(locator_file).await else {
        return;
    };
    let Ok(locator) = serde_json::from_slice::<BridgeStateLocator>(&bytes) else {
        return;
    };
    if locator.state_file == state_file {
        let _ = tokio::fs::remove_file(locator_file).await;
    }
}

pub fn default_data_dir() -> PathBuf {
    env::var_os("CHATOS_BROWSER_BRIDGE_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(platform_data_dir)
}

#[cfg(target_os = "macos")]
pub fn platform_data_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
        .join("Library/Application Support/Chatos/browser-bridge")
}

#[cfg(target_os = "linux")]
pub fn platform_data_dir() -> PathBuf {
    env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
        .join("chatos/browser-bridge")
}

#[cfg(target_os = "windows")]
pub fn platform_data_dir() -> PathBuf {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
        .join("Chatos/browser-bridge")
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[tokio::test]
    async fn active_bridge_locator_points_to_user_scoped_state() {
        let directory = tempdir().unwrap();
        let locator_dir = directory.path().join("locator");
        let user_state = directory
            .path()
            .join("data/users/user-1/plugin-1/browser-bridge/bridge-state.json");
        tokio::fs::create_dir_all(user_state.parent().unwrap())
            .await
            .unwrap();
        let locator = install_active_bridge_locator(&locator_dir, &user_state)
            .await
            .unwrap();
        assert_eq!(
            resolve_active_bridge_state(&locator_dir).await.unwrap(),
            user_state
        );
        remove_active_bridge_locator_if_owned(&locator, &user_state).await;
        assert!(!tokio::fs::try_exists(locator).await.unwrap());
    }

    #[tokio::test]
    async fn stale_bridge_shutdown_does_not_remove_a_new_owner_locator() {
        let directory = tempdir().unwrap();
        let locator_dir = directory.path().join("locator");
        let first = directory.path().join("users/first/bridge-state.json");
        let second = directory.path().join("users/second/bridge-state.json");
        let locator = install_active_bridge_locator(&locator_dir, &first)
            .await
            .unwrap();
        install_active_bridge_locator(&locator_dir, &second)
            .await
            .unwrap();
        remove_active_bridge_locator_if_owned(&locator, &first).await;
        assert_eq!(
            resolve_active_bridge_state(&locator_dir).await.unwrap(),
            second
        );
    }
}
