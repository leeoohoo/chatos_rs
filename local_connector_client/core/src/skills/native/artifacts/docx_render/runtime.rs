// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::Result;
use serde::Deserialize;

use super::super::format_helpers::sha256_file;
use super::{ensure_regular_non_symlink_file, render_error};

const DOCUMENT_RUNTIME_ENV: &str = "CHATOS_DOCUMENT_RUNTIME_DIR";
pub(super) const RUNTIME_MANIFEST_NAME: &str = "runtime.json";
const MAX_RUNTIME_MANIFEST_BYTES: u64 = 32 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DocumentRuntimeManifest {
    schema_version: u32,
    runtime_revision: String,
    platform: String,
    pub(super) soffice: RuntimeExecutableManifest,
    pub(super) pdftoppm: RuntimeExecutableManifest,
    pub(super) poppler_library_dir: Option<String>,
    pub(super) font_directory: String,
    fonts: Vec<RuntimeFontManifest>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeExecutableManifest {
    path: String,
    sha256: String,
    version: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeFontManifest {
    path: String,
    sha256: String,
}

#[derive(Debug)]
pub(super) struct DocumentRuntime {
    pub(super) revision: String,
    pub(super) soffice: PathBuf,
    pub(super) soffice_version: String,
    pub(super) pdftoppm: PathBuf,
    pub(super) pdftoppm_version: String,
    pub(super) poppler_library_dir: Option<PathBuf>,
    pub(super) font_directory: PathBuf,
}

pub(super) fn load_document_runtime(
    runtime_root_override: Option<&Path>,
) -> Result<DocumentRuntime> {
    let configured_root = if let Some(root) = runtime_root_override {
        root.to_path_buf()
    } else {
        std::env::var_os(DOCUMENT_RUNTIME_ENV)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| {
                render_error(
                    "runtime_unavailable",
                    "packaged document render runtime is not configured",
                )
            })?
    };
    let root_metadata = fs::symlink_metadata(configured_root.as_path()).map_err(|error| {
        render_error(
            "runtime_unavailable",
            format!("inspect packaged document render runtime: {error}"),
        )
    })?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(render_error(
            "runtime_manifest_invalid",
            "document render runtime root must be a regular non-symlink directory",
        ));
    }
    let root = configured_root.canonicalize().map_err(|error| {
        render_error(
            "runtime_unavailable",
            format!("resolve packaged document render runtime: {error}"),
        )
    })?;
    let manifest_path = root.join(RUNTIME_MANIFEST_NAME);
    ensure_regular_non_symlink_file(manifest_path.as_path(), "document runtime manifest")?;
    let manifest_metadata = fs::metadata(manifest_path.as_path()).map_err(|error| {
        render_error(
            "runtime_manifest_invalid",
            format!("inspect document runtime manifest: {error}"),
        )
    })?;
    if manifest_metadata.len() == 0 || manifest_metadata.len() > MAX_RUNTIME_MANIFEST_BYTES {
        return Err(render_error(
            "runtime_manifest_invalid",
            "document runtime manifest is empty or exceeds 32 KiB",
        ));
    }
    let manifest: DocumentRuntimeManifest = serde_json::from_slice(
        fs::read(manifest_path.as_path())
            .map_err(|error| {
                render_error(
                    "runtime_manifest_invalid",
                    format!("read document runtime manifest: {error}"),
                )
            })?
            .as_slice(),
    )
    .map_err(|error| {
        render_error(
            "runtime_manifest_invalid",
            format!("decode document runtime manifest: {error}"),
        )
    })?;
    validate_manifest_text(manifest.runtime_revision.as_str(), "runtime_revision", 128)?;
    validate_manifest_text(manifest.soffice.version.as_str(), "soffice.version", 256)?;
    validate_manifest_text(manifest.pdftoppm.version.as_str(), "pdftoppm.version", 256)?;
    if manifest.schema_version != 1 {
        return Err(render_error(
            "runtime_manifest_invalid",
            "document runtime manifest schema_version must be 1",
        ));
    }
    if manifest.platform != current_platform() {
        return Err(render_error(
            "runtime_manifest_invalid",
            format!(
                "document runtime platform {} does not match {}",
                manifest.platform,
                current_platform()
            ),
        ));
    }
    let soffice = resolve_runtime_file(
        root.as_path(),
        manifest.soffice.path.as_str(),
        manifest.soffice.sha256.as_str(),
        "soffice",
    )?;
    let pdftoppm = resolve_runtime_file(
        root.as_path(),
        manifest.pdftoppm.path.as_str(),
        manifest.pdftoppm.sha256.as_str(),
        "pdftoppm",
    )?;
    let poppler_library_dir = manifest
        .poppler_library_dir
        .as_deref()
        .map(|relative| resolve_runtime_directory(root.as_path(), relative, "poppler library"))
        .transpose()?;
    let font_directory = resolve_runtime_directory(
        root.as_path(),
        manifest.font_directory.as_str(),
        "document font",
    )?;
    if manifest.fonts.is_empty() || manifest.fonts.len() > 8 {
        return Err(render_error(
            "runtime_manifest_invalid",
            "document runtime manifest must declare between 1 and 8 fonts",
        ));
    }
    let mut total_font_bytes = 0_u64;
    for font in &manifest.fonts {
        let extension = Path::new(font.path.as_str())
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !matches!(extension.as_str(), "ttf" | "otf" | "ttc") {
            return Err(render_error(
                "runtime_manifest_invalid",
                "document runtime fonts must use .ttf, .otf, or .ttc",
            ));
        }
        let path = resolve_runtime_file(
            root.as_path(),
            font.path.as_str(),
            font.sha256.as_str(),
            "document font",
        )?;
        if !path.starts_with(font_directory.as_path()) {
            return Err(render_error(
                "runtime_manifest_invalid",
                "document runtime font path must remain inside font_directory",
            ));
        }
        total_font_bytes = total_font_bytes.saturating_add(fs::metadata(path)?.len());
        if total_font_bytes > 128 * 1024 * 1024 {
            return Err(render_error(
                "runtime_manifest_invalid",
                "document runtime fonts exceed the 128 MiB safety limit",
            ));
        }
    }
    Ok(DocumentRuntime {
        revision: manifest.runtime_revision,
        soffice,
        soffice_version: manifest.soffice.version,
        pdftoppm,
        pdftoppm_version: manifest.pdftoppm.version,
        poppler_library_dir,
        font_directory,
    })
}

fn resolve_runtime_file(root: &Path, relative: &str, sha256: &str, label: &str) -> Result<PathBuf> {
    validate_sha256(sha256, label)?;
    let path = resolve_runtime_path(root, relative, label)?;
    ensure_regular_non_symlink_file(path.as_path(), label)?;
    let actual = sha256_file(path.as_path()).map_err(|error| {
        render_error(
            "runtime_manifest_invalid",
            format!("hash packaged {label}: {error}"),
        )
    })?;
    if actual != sha256 {
        return Err(render_error(
            "runtime_manifest_invalid",
            format!("packaged {label} hash does not match runtime manifest"),
        ));
    }
    Ok(path)
}

fn resolve_runtime_directory(root: &Path, relative: &str, label: &str) -> Result<PathBuf> {
    let path = resolve_runtime_path(root, relative, label)?;
    let metadata = fs::symlink_metadata(path.as_path()).map_err(|error| {
        render_error(
            "runtime_manifest_invalid",
            format!("inspect packaged {label} directory: {error}"),
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(render_error(
            "runtime_manifest_invalid",
            format!("packaged {label} must be a regular non-symlink directory"),
        ));
    }
    Ok(path)
}

fn resolve_runtime_path(root: &Path, relative: &str, label: &str) -> Result<PathBuf> {
    let relative_path = Path::new(relative);
    if relative.is_empty()
        || relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(render_error(
            "runtime_manifest_invalid",
            format!("packaged {label} path must be a normalized relative path"),
        ));
    }
    let mut cursor = root.to_path_buf();
    for component in relative_path.components() {
        let Component::Normal(component) = component else {
            return Err(render_error(
                "runtime_manifest_invalid",
                format!("packaged {label} path must contain only normal components"),
            ));
        };
        cursor.push(component);
        let metadata = fs::symlink_metadata(cursor.as_path()).map_err(|error| {
            render_error(
                "runtime_manifest_invalid",
                format!("inspect packaged {label} path: {error}"),
            )
        })?;
        if metadata.file_type().is_symlink() {
            return Err(render_error(
                "runtime_manifest_invalid",
                format!("packaged {label} path must not traverse symlinks"),
            ));
        }
    }
    let canonical = cursor.canonicalize().map_err(|error| {
        render_error(
            "runtime_manifest_invalid",
            format!("resolve packaged {label} path: {error}"),
        )
    })?;
    if !canonical.starts_with(root) {
        return Err(render_error(
            "runtime_manifest_invalid",
            format!("packaged {label} path escapes the runtime root"),
        ));
    }
    Ok(canonical)
}

fn validate_manifest_text(value: &str, field: &str, max_chars: usize) -> Result<()> {
    if value.trim().is_empty()
        || value.chars().count() > max_chars
        || value.chars().any(|character| character.is_control())
    {
        return Err(render_error(
            "runtime_manifest_invalid",
            format!("document runtime manifest {field} is invalid"),
        ));
    }
    Ok(())
}

fn validate_sha256(value: &str, label: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(render_error(
            "runtime_manifest_invalid",
            format!("packaged {label} SHA-256 is invalid"),
        ));
    }
    Ok(())
}

pub(super) fn current_platform() -> &'static str {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "macos-arm64"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "macos-x64"
    }
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        "windows-arm64"
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "windows-x64"
    }
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "windows", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64")
    )))]
    {
        "unsupported"
    }
}
