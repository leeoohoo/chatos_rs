// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::ffi::OsStr;
use std::path::{Component, Path};

const IGNORED_DIRECTORY_NAMES: &[&str] = &[
    ".build",
    ".bundle",
    ".cache",
    ".dart_tool",
    ".gradle",
    ".hg",
    ".idea",
    ".mypy_cache",
    ".next",
    ".nuxt",
    ".parcel-cache",
    ".pnpm-store",
    ".pytest_cache",
    ".ruff_cache",
    ".stack-work",
    ".svelte-kit",
    ".svn",
    ".swiftpm",
    ".tox",
    ".turbo",
    ".venv",
    ".vite",
    ".vs",
    ".vscode",
    "__macosx",
    "__pycache__",
    "_build",
    "bower_components",
    "build",
    "cmakefiles",
    "coverage",
    "deps",
    "deriveddata",
    "deriveddatacache",
    "dist",
    "dist-newstyle",
    "ebin",
    "intermediate",
    "library",
    "node_modules",
    "obj",
    "out",
    "pods",
    "saved",
    "target",
    "testresults",
    "vendor",
    "venv",
];

const IGNORED_FILE_NAMES: &[&str] = &[
    ".coverage",
    ".ds_store",
    ".packages",
    "desktop.ini",
    "thumbs.db",
];

const IGNORED_FILE_EXTENSIONS: &[&str] = &[
    "a",
    "aab",
    "apk",
    "app",
    "beam",
    "class",
    "d",
    "dex",
    "dll",
    "dylib",
    "ear",
    "exe",
    "gcda",
    "gcno",
    "gch",
    "hi",
    "ipa",
    "jar",
    "lib",
    "o",
    "obj",
    "pch",
    "pdb",
    "profraw",
    "pyc",
    "pyd",
    "pyo",
    "rlib",
    "rmeta",
    "so",
    "swiftdoc",
    "swiftmodule",
    "war",
    "wasm",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ArchivePathAction {
    Include,
    Ignore,
}

pub(super) fn classify_archive_path(path: &Path) -> ArchivePathAction {
    let components = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().to_ascii_lowercase()),
            _ => None,
        })
        .collect::<Vec<_>>();

    if components.iter().any(|component| component == ".git") {
        return ArchivePathAction::Ignore;
    }
    if components.iter().any(|component| {
        IGNORED_DIRECTORY_NAMES.contains(&component.as_str())
            || component.starts_with("bazel-")
            || component.starts_with("cmake-build-")
            || component.ends_with(".app")
            || component.ends_with(".dsym")
    }) {
        return ArchivePathAction::Ignore;
    }

    let Some(file_name) = components.last() else {
        return ArchivePathAction::Include;
    };
    if IGNORED_FILE_NAMES.contains(&file_name.as_str())
        || file_name.ends_with('~')
        || file_name.ends_with(".log")
        || file_name.ends_with(".swo")
        || file_name.ends_with(".swp")
        || file_name.ends_with(".tmp")
        || file_name.ends_with(".tsbuildinfo")
    {
        return ArchivePathAction::Ignore;
    }

    let extension = Path::new(file_name)
        .extension()
        .and_then(OsStr::to_str)
        .unwrap_or_default();
    if IGNORED_FILE_EXTENSIONS.contains(&extension) {
        ArchivePathAction::Ignore
    } else {
        ArchivePathAction::Include
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_compiled_files_but_keeps_source_manifests() {
        assert_eq!(
            classify_archive_path(Path::new("service/bin/app.dll")),
            ArchivePathAction::Ignore
        );
        assert_eq!(
            classify_archive_path(Path::new("service/package-lock.json")),
            ArchivePathAction::Include
        );
        assert_eq!(
            classify_archive_path(Path::new("service/src/bin/main.rs")),
            ArchivePathAction::Include
        );
        assert_eq!(
            classify_archive_path(Path::new("service/go.mod")),
            ArchivePathAction::Include
        );
    }

    #[test]
    fn covers_common_language_build_directories() {
        for path in [
            "web/node_modules/pkg/index.js",
            "web/.next/server/app.js",
            "rust/target/release/app",
            "java/target/classes/App.class",
            "python/__pycache__/main.pyc",
            "dotnet/obj/project.assets.json",
            "cpp/cmake-build-release/app",
            "php/vendor/pkg/source.php",
            "flutter/.dart_tool/package_config.json",
            "ios/DerivedData/App/Build/product",
        ] {
            assert_eq!(
                classify_archive_path(Path::new(path)),
                ArchivePathAction::Ignore,
                "expected {path} to be ignored"
            );
        }
    }

    #[test]
    fn ignores_git_metadata_without_rejecting_the_project_archive() {
        assert_eq!(
            classify_archive_path(Path::new("repo/.git/config")),
            ArchivePathAction::Ignore
        );
        assert_eq!(
            classify_archive_path(Path::new("repo/.gitignore")),
            ArchivePathAction::Include
        );
    }
}
