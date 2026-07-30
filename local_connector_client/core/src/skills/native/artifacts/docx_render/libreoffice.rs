// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use url::Url;

use super::render_error;
use super::runtime_environment::{
    prepare_private_fontconfig, private_process_environment, trusted_font_paths,
};

pub(super) struct LibreOfficeDirectories {
    pub(super) output: PathBuf,
    pub(super) profile: PathBuf,
    pub(super) home: PathBuf,
    pub(super) temp: PathBuf,
}

pub(super) fn prepare_libreoffice_directories(
    work: &Path,
    render_kind: &str,
) -> Result<LibreOfficeDirectories> {
    let directories = LibreOfficeDirectories {
        output: work.join("output"),
        profile: work.join("libreoffice-profile"),
        home: work.join("home"),
        temp: work.join("tmp"),
    };
    for directory in [
        &directories.output,
        &directories.profile,
        &directories.home,
        &directories.temp,
    ] {
        fs::create_dir(directory).map_err(|error| {
            render_error(
                "private_directory_failed",
                format!(
                    "create private {render_kind} render directory {}: {error}",
                    directory.display()
                ),
            )
        })?;
    }
    Ok(directories)
}

pub(super) fn private_libreoffice_environment(
    work: &Path,
    directories: &LibreOfficeDirectories,
    font_directory: &Path,
) -> Result<BTreeMap<String, OsString>> {
    let mut environment =
        private_process_environment(directories.home.as_path(), directories.temp.as_path());
    environment.insert("SAL_USE_VCLPLUGIN".to_string(), OsString::from("svp"));
    environment.insert(
        "SAL_FONTPATH".to_string(),
        trusted_font_paths(font_directory)?,
    );
    let fontconfig = prepare_private_fontconfig(work, directories.home.as_path(), font_directory)?;
    let fontconfig_parent = fontconfig.parent().ok_or_else(|| {
        render_error(
            "private_directory_failed",
            "private fontconfig path does not have a parent directory",
        )
    })?;
    environment.insert(
        "FONTCONFIG_FILE".to_string(),
        fontconfig.as_os_str().to_os_string(),
    );
    environment.insert(
        "FONTCONFIG_PATH".to_string(),
        fontconfig_parent.as_os_str().to_os_string(),
    );
    Ok(environment)
}

pub(super) fn libreoffice_conversion_arguments(
    directories: &LibreOfficeDirectories,
    source: &Path,
    export_filter: &str,
    render_kind: &str,
    safe_mode: bool,
) -> Result<Vec<OsString>> {
    let profile_url = Url::from_directory_path(directories.profile.as_path()).map_err(|_| {
        render_error(
            "private_directory_failed",
            format!("encode private LibreOffice {render_kind} profile path"),
        )
    })?;
    let mut arguments = vec![OsString::from("--headless")];
    if safe_mode {
        arguments.push(OsString::from("--safe-mode"));
    }
    arguments.extend([
        OsString::from("--nologo"),
        OsString::from("--nodefault"),
        OsString::from("--nolockcheck"),
        OsString::from("--nofirststartwizard"),
        OsString::from(format!("-env:UserInstallation={profile_url}")),
        OsString::from("--convert-to"),
        OsString::from(export_filter),
        OsString::from("--outdir"),
        directories.output.as_os_str().to_os_string(),
        source.as_os_str().to_os_string(),
    ]);
    Ok(arguments)
}
