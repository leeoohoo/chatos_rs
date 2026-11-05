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
pub const STATE_LOCATOR_DIRECTORY_NAME: &str = "active-bridges";

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
    let registry_dir = locator_dir.join(STATE_LOCATOR_DIRECTORY_NAME);
    tokio::fs::create_dir_all(&registry_dir)
        .await
        .map_err(|error| format!("could not create Browser Bridge registry: {error}"))?;
    let locator_name = format!("bridge-{}.json", Uuid::new_v4().simple());
    let locator_file = registry_dir.join(&locator_name);
    let temporary = registry_dir.join(format!(".{locator_name}.tmp"));
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
    tokio::fs::rename(&temporary, &locator_file)
        .await
        .map_err(|error| format!("could not install Browser Bridge locator: {error}"))?;
    Ok(locator_file)
}

pub async fn resolve_active_bridge_state(locator_dir: &Path) -> Result<PathBuf, String> {
    resolve_active_bridge_states(locator_dir)
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| "Browser Bridge is not running".to_string())
}

pub async fn resolve_active_bridge_states(locator_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let registry_dir = locator_dir.join(STATE_LOCATOR_DIRECTORY_NAME);
    let mut entries = match tokio::fs::read_dir(&registry_dir).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Vec::new());
        }
        Err(error) => return Err(format!("could not read Browser Bridge registry: {error}")),
    };
    let mut state_files = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| format!("could not read Browser Bridge registry: {error}"))?
    {
        let locator_file = entry.path();
        if locator_file.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Ok(state_file) = read_bridge_locator(&locator_file).await else {
            let _ = tokio::fs::remove_file(&locator_file).await;
            continue;
        };
        if tokio::fs::try_exists(&state_file).await.unwrap_or(false) {
            state_files.push(state_file);
        } else {
            let _ = tokio::fs::remove_file(&locator_file).await;
        }
    }
    state_files.sort();
    Ok(state_files)
}

async fn read_bridge_locator(locator_file: &Path) -> Result<PathBuf, String> {
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
    async fn active_bridge_registry_points_to_every_running_instance() {
        let directory = tempdir().unwrap();
        let locator_dir = directory.path().join("locator");
        let first_state = directory
            .path()
            .join("data/users/user-1/plugin-1/browser-bridge/bridge-state.json");
        let second_state = directory
            .path()
            .join("data/users/user-1/plugin-1/browser-bridge-2/bridge-state.json");
        tokio::fs::create_dir_all(first_state.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::create_dir_all(second_state.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&first_state, b"{}").await.unwrap();
        tokio::fs::write(&second_state, b"{}").await.unwrap();
        let first_locator = install_active_bridge_locator(&locator_dir, &first_state)
            .await
            .unwrap();
        let second_locator = install_active_bridge_locator(&locator_dir, &second_state)
            .await
            .unwrap();
        assert_ne!(first_locator, second_locator);
        assert_eq!(
            resolve_active_bridge_states(&locator_dir).await.unwrap(),
            vec![first_state.clone(), second_state.clone()]
        );
        remove_active_bridge_locator_if_owned(&first_locator, &first_state).await;
        assert_eq!(
            resolve_active_bridge_states(&locator_dir).await.unwrap(),
            vec![second_state]
        );
    }
}
