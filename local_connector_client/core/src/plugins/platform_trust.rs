// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::Result;

#[cfg(target_os = "macos")]
const ALLOW_ADHOC_MACOS_APPS_ENV: &str = "CHATOS_ALLOW_ADHOC_MACOS_PLUGIN_APPS";

pub(super) fn verify_platform_package_trust(
    package_root: &Path,
    package_files: &BTreeMap<String, String>,
) -> Result<()> {
    #[cfg(target_os = "macos")]
    verify_macos_app_bundles(package_root, package_files, |app_path| {
        assess_macos_app_bundle(app_path, local_development_adhoc_allowed())
    })?;

    #[cfg(not(target_os = "macos"))]
    let _ = (package_root, package_files);

    Ok(())
}

fn macos_app_bundle_paths(package_files: &BTreeMap<String, String>) -> BTreeSet<PathBuf> {
    let mut bundles = BTreeSet::new();
    for relative in package_files.keys() {
        let mut bundle = PathBuf::new();
        for segment in relative.split('/') {
            bundle.push(segment);
            if segment.to_ascii_lowercase().ends_with(".app") {
                bundles.insert(bundle.clone());
                break;
            }
        }
    }
    bundles
}

#[cfg(target_os = "macos")]
fn verify_macos_app_bundles<F>(
    package_root: &Path,
    package_files: &BTreeMap<String, String>,
    mut assess: F,
) -> Result<()>
where
    F: FnMut(&Path) -> Result<()>,
{
    for relative in macos_app_bundle_paths(package_files) {
        assess(package_root.join(relative).as_path())?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn assess_macos_app_bundle(app_path: &Path, allow_adhoc: bool) -> Result<()> {
    use std::process::Command;

    use anyhow::{bail, Context};

    let codesign = Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict", "--verbose=2"])
        .arg(app_path)
        .output()
        .with_context(|| {
            format!(
                "run macOS code-signing verification: {}",
                app_path.display()
            )
        })?;
    if !codesign.status.success() {
        bail!(
            "Plugin contains a macOS app with an invalid code signature: {} ({})",
            app_path.display(),
            command_diagnostic(&codesign)
        );
    }

    if allow_adhoc && has_adhoc_signature(app_path)? {
        return Ok(());
    }

    let gatekeeper = Command::new("/usr/sbin/spctl")
        .args(["--assess", "--type", "execute", "--verbose=4"])
        .arg(app_path)
        .output()
        .with_context(|| format!("run macOS Gatekeeper assessment: {}", app_path.display()))?;
    if !gatekeeper.status.success() {
        bail!(
            "Plugin contains a macOS app that Gatekeeper does not trust; the publisher must use a valid Developer ID certificate and Apple notarization: {} ({})",
            app_path.display(),
            command_diagnostic(&gatekeeper)
        );
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn has_adhoc_signature(app_path: &Path) -> Result<bool> {
    use std::process::Command;

    use anyhow::Context;

    let output = Command::new("/usr/bin/codesign")
        .args(["--display", "--verbose=4"])
        .arg(app_path)
        .output()
        .with_context(|| format!("inspect macOS app signature: {}", app_path.display()))?;
    if !output.status.success() {
        return Ok(false);
    }
    Ok(is_adhoc_codesign_output(
        String::from_utf8_lossy(output.stderr.as_slice()).as_ref(),
    ))
}

#[cfg(target_os = "macos")]
fn is_adhoc_codesign_output(output: &str) -> bool {
    output
        .lines()
        .any(|line| line.trim().eq_ignore_ascii_case("Signature=adhoc"))
}

#[cfg(target_os = "macos")]
fn local_development_adhoc_allowed() -> bool {
    std::env::var(ALLOW_ADHOC_MACOS_APPS_ENV).as_deref() == Ok("1")
}

#[cfg(target_os = "macos")]
fn command_diagnostic(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(output.stderr.as_slice());
    let stdout = String::from_utf8_lossy(output.stdout.as_slice());
    let message = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    if message.is_empty() {
        format!("exit status {}", output.status)
    } else {
        message.chars().take(1_000).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_each_macos_app_bundle_once() {
        let files = BTreeMap::from([
            (
                "dist/Open Computer Use.app/Contents/Info.plist".to_string(),
                "a".to_string(),
            ),
            (
                "dist/Open Computer Use.app/Contents/MacOS/OpenComputerUse".to_string(),
                "b".to_string(),
            ),
            (
                "vendor/Helper.app/Contents/MacOS/Helper".to_string(),
                "c".to_string(),
            ),
            ("bin/plugin".to_string(), "d".to_string()),
        ]);

        assert_eq!(
            macos_app_bundle_paths(&files),
            BTreeSet::from([
                PathBuf::from("dist/Open Computer Use.app"),
                PathBuf::from("vendor/Helper.app"),
            ])
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn assesses_discovered_bundles_before_installation() {
        let files = BTreeMap::from([(
            "dist/Example.app/Contents/MacOS/Example".to_string(),
            "digest".to_string(),
        )]);
        let mut assessed = Vec::new();

        verify_macos_app_bundles(Path::new("/plugin"), &files, |path| {
            assessed.push(path.to_path_buf());
            Ok(())
        })
        .expect("trusted app bundle");

        assert_eq!(assessed, vec![PathBuf::from("/plugin/dist/Example.app")]);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn only_recognizes_explicit_adhoc_codesign_output() {
        assert!(is_adhoc_codesign_output(
            "Identifier=example\nSignature=adhoc\nTeamIdentifier=not set"
        ));
        assert!(!is_adhoc_codesign_output(
            "Authority=Developer ID Application: Example\nTeamIdentifier=ABCDE12345"
        ));
    }
}
