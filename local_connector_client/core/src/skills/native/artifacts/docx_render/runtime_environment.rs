// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

use super::render_error;

pub(super) fn private_process_environment(home: &Path, temp: &Path) -> BTreeMap<String, OsString> {
    let mut environment = BTreeMap::new();
    environment.insert("HOME".to_string(), home.as_os_str().to_os_string());
    environment.insert("TMPDIR".to_string(), temp.as_os_str().to_os_string());
    environment.insert("TMP".to_string(), temp.as_os_str().to_os_string());
    environment.insert("TEMP".to_string(), temp.as_os_str().to_os_string());
    environment.insert(
        "XDG_CACHE_HOME".to_string(),
        home.join(".cache").as_os_str().to_os_string(),
    );
    environment.insert(
        "XDG_CONFIG_HOME".to_string(),
        home.join(".config").as_os_str().to_os_string(),
    );
    environment.insert(
        "XDG_STATE_HOME".to_string(),
        home.join(".local/state").as_os_str().to_os_string(),
    );
    #[cfg(unix)]
    {
        environment.insert("PATH".to_string(), OsString::from("/usr/bin:/bin"));
        environment.insert("LANG".to_string(), OsString::from("C.UTF-8"));
        environment.insert("LC_ALL".to_string(), OsString::from("C.UTF-8"));
    }
    #[cfg(windows)]
    if let Some(system_root) = std::env::var_os("SystemRoot") {
        environment.insert("SystemRoot".to_string(), system_root);
    }
    environment
}

pub(super) fn add_poppler_library_environment(
    environment: &mut BTreeMap<String, OsString>,
    library_dir: Option<&Path>,
) {
    #[cfg(target_os = "macos")]
    if let Some(library_dir) = library_dir {
        environment.insert(
            "DYLD_FALLBACK_LIBRARY_PATH".to_string(),
            library_dir.as_os_str().to_os_string(),
        );
    }
    #[cfg(target_os = "linux")]
    if let Some(library_dir) = library_dir {
        environment.insert(
            "LD_LIBRARY_PATH".to_string(),
            library_dir.as_os_str().to_os_string(),
        );
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let _ = (environment, library_dir);
}

pub(super) fn trusted_font_paths(runtime_fonts: &Path) -> Result<OsString> {
    let mut paths = vec![runtime_fonts.to_path_buf()];
    #[cfg(target_os = "macos")]
    paths.extend([
        PathBuf::from("/System/Library/Fonts"),
        PathBuf::from("/System/Library/Fonts/Supplemental"),
        PathBuf::from("/Library/Fonts"),
    ]);
    #[cfg(target_os = "windows")]
    if let Some(system_root) = std::env::var_os("SystemRoot").filter(|value| !value.is_empty()) {
        paths.push(PathBuf::from(system_root).join("Fonts"));
    }
    std::env::join_paths(paths.iter()).map_err(|error| {
        render_error(
            "runtime_manifest_invalid",
            format!("encode trusted document font paths: {error}"),
        )
    })
}

pub(super) fn prepare_private_fontconfig(
    work: &Path,
    home: &Path,
    runtime_fonts: &Path,
) -> Result<PathBuf> {
    let config_dir = work.join("fontconfig");
    let cache_dir = home.join("fontconfig-cache");
    fs::create_dir(config_dir.as_path()).map_err(|error| {
        render_error(
            "private_directory_failed",
            format!("create private fontconfig directory: {error}"),
        )
    })?;
    fs::create_dir(cache_dir.as_path()).map_err(|error| {
        render_error(
            "private_directory_failed",
            format!("create private fontconfig cache: {error}"),
        )
    })?;
    let mut directories = vec![runtime_fonts.to_path_buf()];
    #[cfg(target_os = "macos")]
    directories.extend([
        PathBuf::from("/System/Library/Fonts"),
        PathBuf::from("/System/Library/Fonts/Supplemental"),
        PathBuf::from("/Library/Fonts"),
    ]);
    #[cfg(target_os = "windows")]
    if let Some(system_root) = std::env::var_os("SystemRoot").filter(|value| !value.is_empty()) {
        directories.push(PathBuf::from(system_root).join("Fonts"));
    }
    let directory_xml = directories
        .iter()
        .map(|directory| format!("<dir>{}</dir>", escape_fontconfig_xml(directory.as_path())))
        .collect::<String>();
    let config = format!(
        "<?xml version=\"1.0\"?><!DOCTYPE fontconfig SYSTEM \"urn:fontconfig:fonts.dtd\"><fontconfig>{directory_xml}<cachedir>{}</cachedir><config><rescan><int>0</int></rescan></config></fontconfig>",
        escape_fontconfig_xml(cache_dir.as_path())
    );
    let config_path = config_dir.join("fonts.conf");
    fs::write(config_path.as_path(), config).map_err(|error| {
        render_error(
            "private_directory_failed",
            format!("write private fontconfig configuration: {error}"),
        )
    })?;
    Ok(config_path)
}

fn escape_fontconfig_xml(path: &Path) -> String {
    path.to_string_lossy()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
