// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{anyhow, bail, Context, Result};

const HELPER_EXECUTABLE_NAME: &str = "chatos_computer_use_helper";
const HELPER_PATH_ENV: &str = "CHATOS_COMPUTER_USE_HELPER_PATH";
const HELPER_REQUIRE_SIGNED_ENV: &str = "CHATOS_COMPUTER_USE_HELPER_REQUIRE_SIGNED";
const HELPER_ALLOW_UNSIGNED_LOCAL_DEV_ENV: &str = "CHATOS_COMPUTER_USE_ALLOW_UNSIGNED_LOCAL_DEV";
const MACOS_CODESIGN_PATH: &str = "/usr/bin/codesign";
const MAX_CODESIGN_OUTPUT_BYTES: usize = 64 * 1024;

pub(super) fn helper_path() -> Result<PathBuf> {
    if let Some(configured) = std::env::var_os(HELPER_PATH_ENV) {
        let path = PathBuf::from(configured);
        if !path.is_absolute() {
            bail!("{HELPER_PATH_ENV} must be an absolute path");
        }
        return Ok(path);
    }
    let current_executable =
        std::env::current_exe().context("resolve Local Connector Core path")?;
    let parent = current_executable
        .parent()
        .ok_or_else(|| anyhow!("Local Connector Core executable directory is unavailable"))?;
    Ok(parent.join(HELPER_EXECUTABLE_NAME))
}

pub(super) fn validate_helper_binary(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("Computer Use helper is missing: {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("Computer Use helper must be a regular non-symlink file");
    }
    use std::os::unix::fs::PermissionsExt;
    if metadata.permissions().mode() & 0o111 == 0 {
        bail!("Computer Use helper is not executable");
    }
    Ok(())
}

pub(super) fn validate_helper_signature(path: &Path) -> Result<()> {
    if !helper_signature_required() {
        return Ok(());
    }
    if !Path::new(MACOS_CODESIGN_PATH).is_file() {
        bail!("macOS codesign verification runtime is missing: {MACOS_CODESIGN_PATH}");
    }
    verify_codesign(path)?;
    let current_executable =
        std::env::current_exe().context("resolve Local Connector Core path")?;
    verify_codesign(current_executable.as_path())?;
    let helper_team = codesign_team_identifier(path)?;
    let core_team = codesign_team_identifier(current_executable.as_path())?;
    if helper_team != core_team {
        bail!("Computer Use helper signing team does not match Local Connector Core");
    }
    Ok(())
}

pub(super) fn helper_signature_required() -> bool {
    if env_flag(HELPER_ALLOW_UNSIGNED_LOCAL_DEV_ENV) {
        return false;
    }
    if env_flag(HELPER_REQUIRE_SIGNED_ENV) {
        return true;
    }
    !cfg!(debug_assertions)
}

fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "TRUE" | "yes" | "YES"))
}

pub(super) fn verify_codesign(path: &Path) -> Result<()> {
    let output = Command::new(MACOS_CODESIGN_PATH)
        .args(["--verify", "--strict", "--verbose=2"])
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("verify code signature for {}", path.display()))?;
    ensure_codesign_output_bounded(&output.stdout, &output.stderr)?;
    if output.status.success() {
        Ok(())
    } else {
        bail!("Computer Use helper code signature verification failed")
    }
}

pub(super) fn codesign_team_identifier(path: &Path) -> Result<String> {
    let output = Command::new(MACOS_CODESIGN_PATH)
        .args(["-d", "--verbose=4"])
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("read code signature identity for {}", path.display()))?;
    ensure_codesign_output_bounded(&output.stdout, &output.stderr)?;
    if !output.status.success() {
        bail!("Computer Use helper code signature identity is unavailable");
    }
    let details = String::from_utf8_lossy(output.stderr.as_slice());
    let team = details
        .lines()
        .find_map(|line| line.trim().strip_prefix("TeamIdentifier="))
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "not set")
        .ok_or_else(|| anyhow!("Computer Use helper requires a Developer ID team signature"))?;
    if team.len() > 256
        || !team
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        bail!("Computer Use helper signing team identifier is invalid");
    }
    Ok(team.to_string())
}

fn ensure_codesign_output_bounded(stdout: &[u8], stderr: &[u8]) -> Result<()> {
    if stdout.len() > MAX_CODESIGN_OUTPUT_BYTES || stderr.len() > MAX_CODESIGN_OUTPUT_BYTES {
        bail!("macOS codesign output exceeded the safety limit");
    }
    Ok(())
}
