mod install;
mod native;
mod server;
mod wire;

use std::{env, path::PathBuf};

pub use install::{
    NATIVE_HOST_NAME, NativeHostInstallation, install_native_host, uninstall_native_host,
};
pub use native::run_native_host;
pub use server::{BridgeReady, BridgeServer, BridgeServerConfig};

pub const STATE_FILE_NAME: &str = "bridge-state.json";

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
