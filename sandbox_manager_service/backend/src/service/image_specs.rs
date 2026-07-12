// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::{SandboxImageFeatureRecord, SandboxImageRuntimeVersionRecord};

const DEFAULT_IMAGE_FEATURES: [&str; 4] = ["java@21", "node@24", "rust@stable", "go@1.26"];

#[derive(Debug, Clone, Copy)]
struct RuntimeVersionSpec {
    id: &'static str,
    label: &'static str,
    description: &'static str,
    default: bool,
}

#[derive(Debug, Clone, Copy)]
struct RuntimeSpec {
    id: &'static str,
    label: &'static str,
    description: &'static str,
    aliases: &'static [&'static str],
    versions: &'static [RuntimeVersionSpec],
}

#[derive(Debug, Clone, Copy)]
pub(super) struct RuntimeSelectionSpec {
    runtime: RuntimeSpec,
    version: RuntimeVersionSpec,
}

#[derive(Debug, Clone)]
pub(super) struct ParsedImageId {
    pub(super) selections: Vec<RuntimeSelectionSpec>,
    pub(super) custom_script_hash: Option<String>,
}

const JAVA_VERSIONS: [RuntimeVersionSpec; 5] = [
    RuntimeVersionSpec {
        id: "8",
        label: "JDK 8",
        description: "Temurin JDK 8 LTS",
        default: false,
    },
    RuntimeVersionSpec {
        id: "11",
        label: "JDK 11",
        description: "Temurin JDK 11 LTS",
        default: false,
    },
    RuntimeVersionSpec {
        id: "17",
        label: "JDK 17",
        description: "Temurin JDK 17 LTS",
        default: false,
    },
    RuntimeVersionSpec {
        id: "21",
        label: "JDK 21",
        description: "Temurin JDK 21 LTS",
        default: true,
    },
    RuntimeVersionSpec {
        id: "25",
        label: "JDK 25",
        description: "Temurin JDK 25 LTS",
        default: false,
    },
];

const NODE_VERSIONS: [RuntimeVersionSpec; 4] = [
    RuntimeVersionSpec {
        id: "20",
        label: "Node.js 20",
        description: "Node.js 20 legacy line",
        default: false,
    },
    RuntimeVersionSpec {
        id: "22",
        label: "Node.js 22",
        description: "Node.js 22 LTS",
        default: false,
    },
    RuntimeVersionSpec {
        id: "24",
        label: "Node.js 24",
        description: "Node.js 24 LTS",
        default: true,
    },
    RuntimeVersionSpec {
        id: "26",
        label: "Node.js 26",
        description: "Node.js 26 current line",
        default: false,
    },
];

const PYTHON_VERSIONS: [RuntimeVersionSpec; 5] = [
    RuntimeVersionSpec {
        id: "3.10",
        label: "Python 3.10",
        description: "Python 3.10 security support line",
        default: false,
    },
    RuntimeVersionSpec {
        id: "3.11",
        label: "Python 3.11",
        description: "Python 3.11 security support line",
        default: false,
    },
    RuntimeVersionSpec {
        id: "3.12",
        label: "Python 3.12",
        description: "Python 3.12 security support line",
        default: false,
    },
    RuntimeVersionSpec {
        id: "3.13",
        label: "Python 3.13",
        description: "Python 3.13 security support line",
        default: false,
    },
    RuntimeVersionSpec {
        id: "3.14",
        label: "Python 3.14",
        description: "Python 3.14 active support line",
        default: true,
    },
];

const RUST_VERSIONS: [RuntimeVersionSpec; 7] = [
    RuntimeVersionSpec {
        id: "1.85.1",
        label: "Rust 1.85.1",
        description: "Pinned Rust 1.85.1 toolchain",
        default: false,
    },
    RuntimeVersionSpec {
        id: "1.88.0",
        label: "Rust 1.88.0",
        description: "Pinned Rust 1.88.0 toolchain",
        default: false,
    },
    RuntimeVersionSpec {
        id: "1.92.0",
        label: "Rust 1.92.0",
        description: "Pinned Rust 1.92.0 toolchain",
        default: false,
    },
    RuntimeVersionSpec {
        id: "1.96.1",
        label: "Rust 1.96.1",
        description: "Pinned Rust 1.96.1 toolchain",
        default: false,
    },
    RuntimeVersionSpec {
        id: "stable",
        label: "Stable",
        description: "Rust stable channel",
        default: true,
    },
    RuntimeVersionSpec {
        id: "beta",
        label: "Beta",
        description: "Rust beta channel",
        default: false,
    },
    RuntimeVersionSpec {
        id: "nightly",
        label: "Nightly",
        description: "Rust nightly channel",
        default: false,
    },
];

const GO_VERSIONS: [RuntimeVersionSpec; 5] = [
    RuntimeVersionSpec {
        id: "1.22",
        label: "Go 1.22",
        description: "Go 1.22 toolchain",
        default: false,
    },
    RuntimeVersionSpec {
        id: "1.23",
        label: "Go 1.23",
        description: "Go 1.23 toolchain",
        default: false,
    },
    RuntimeVersionSpec {
        id: "1.24",
        label: "Go 1.24",
        description: "Go 1.24 toolchain",
        default: false,
    },
    RuntimeVersionSpec {
        id: "1.25",
        label: "Go 1.25",
        description: "Go 1.25 toolchain",
        default: false,
    },
    RuntimeVersionSpec {
        id: "1.26",
        label: "Go 1.26",
        description: "Go 1.26 toolchain",
        default: true,
    },
];

const DOTNET_VERSIONS: [RuntimeVersionSpec; 3] = [
    RuntimeVersionSpec {
        id: "8.0",
        label: ".NET 8",
        description: ".NET 8 LTS SDK",
        default: false,
    },
    RuntimeVersionSpec {
        id: "9.0",
        label: ".NET 9",
        description: ".NET 9 STS SDK",
        default: false,
    },
    RuntimeVersionSpec {
        id: "10.0",
        label: ".NET 10",
        description: ".NET 10 LTS SDK",
        default: true,
    },
];

const PHP_VERSIONS: [RuntimeVersionSpec; 4] = [
    RuntimeVersionSpec {
        id: "8.2",
        label: "PHP 8.2",
        description: "PHP 8.2 security support line",
        default: false,
    },
    RuntimeVersionSpec {
        id: "8.3",
        label: "PHP 8.3",
        description: "PHP 8.3 security support line",
        default: false,
    },
    RuntimeVersionSpec {
        id: "8.4",
        label: "PHP 8.4",
        description: "PHP 8.4 active support line",
        default: true,
    },
    RuntimeVersionSpec {
        id: "8.5",
        label: "PHP 8.5",
        description: "PHP 8.5 active support line",
        default: false,
    },
];

const RUBY_VERSIONS: [RuntimeVersionSpec; 4] = [
    RuntimeVersionSpec {
        id: "3.2.11",
        label: "Ruby 3.2.11",
        description: "Ruby 3.2 maintenance line",
        default: false,
    },
    RuntimeVersionSpec {
        id: "3.3.11",
        label: "Ruby 3.3.11",
        description: "Ruby 3.3 maintenance line",
        default: false,
    },
    RuntimeVersionSpec {
        id: "3.4.10",
        label: "Ruby 3.4.10",
        description: "Ruby 3.4 stable line",
        default: true,
    },
    RuntimeVersionSpec {
        id: "4.0.5",
        label: "Ruby 4.0.5",
        description: "Ruby 4.0 current line",
        default: false,
    },
];

const GCC_VERSIONS: [RuntimeVersionSpec; 2] = [
    RuntimeVersionSpec {
        id: "13",
        label: "GCC 13",
        description: "GNU C/C++ compiler 13",
        default: false,
    },
    RuntimeVersionSpec {
        id: "14",
        label: "GCC 14",
        description: "GNU C/C++ compiler 14",
        default: true,
    },
];

const CLANG_VERSIONS: [RuntimeVersionSpec; 3] = [
    RuntimeVersionSpec {
        id: "18",
        label: "Clang 18",
        description: "LLVM/Clang 18 toolchain",
        default: false,
    },
    RuntimeVersionSpec {
        id: "19",
        label: "Clang 19",
        description: "LLVM/Clang 19 toolchain",
        default: false,
    },
    RuntimeVersionSpec {
        id: "20",
        label: "Clang 20",
        description: "LLVM/Clang 20 toolchain",
        default: true,
    },
];

const IMAGE_RUNTIMES: [RuntimeSpec; 10] = [
    RuntimeSpec {
        id: "java",
        label: "JDK",
        description: "OpenJDK development tools",
        aliases: &["jdk", "openjdk"],
        versions: &JAVA_VERSIONS,
    },
    RuntimeSpec {
        id: "node",
        label: "Node.js",
        description: "Node.js, npm, pnpm and yarn",
        aliases: &["js", "nodejs", "javascript", "typescript"],
        versions: &NODE_VERSIONS,
    },
    RuntimeSpec {
        id: "python",
        label: "Python",
        description: "Python interpreter, pip and venv tooling",
        aliases: &["py", "python3"],
        versions: &PYTHON_VERSIONS,
    },
    RuntimeSpec {
        id: "rust",
        label: "Rust",
        description: "Rust toolchain",
        aliases: &["cargo"],
        versions: &RUST_VERSIONS,
    },
    RuntimeSpec {
        id: "go",
        label: "Go",
        description: "Go toolchain",
        aliases: &["golang"],
        versions: &GO_VERSIONS,
    },
    RuntimeSpec {
        id: "dotnet",
        label: ".NET",
        description: ".NET SDK for C# and F# projects",
        aliases: &["csharp", "cs", "fsharp"],
        versions: &DOTNET_VERSIONS,
    },
    RuntimeSpec {
        id: "php",
        label: "PHP",
        description: "PHP CLI runtime and Composer",
        aliases: &[],
        versions: &PHP_VERSIONS,
    },
    RuntimeSpec {
        id: "ruby",
        label: "Ruby",
        description: "Ruby runtime, RubyGems and Bundler",
        aliases: &["rails"],
        versions: &RUBY_VERSIONS,
    },
    RuntimeSpec {
        id: "gcc",
        label: "C/C++ (GCC)",
        description: "GNU C and C++ compiler toolchain",
        aliases: &["c", "cpp", "c++", "cplusplus", "g++"],
        versions: &GCC_VERSIONS,
    },
    RuntimeSpec {
        id: "clang",
        label: "C/C++ (Clang)",
        description: "LLVM, Clang, LLD and Clangd toolchain",
        aliases: &["llvm"],
        versions: &CLANG_VERSIONS,
    },
];

pub(super) fn default_image_features() -> Vec<String> {
    DEFAULT_IMAGE_FEATURES
        .iter()
        .map(|feature| (*feature).to_string())
        .collect()
}

pub(super) fn catalog_features() -> Vec<SandboxImageFeatureRecord> {
    IMAGE_RUNTIMES
        .into_iter()
        .map(|runtime| SandboxImageFeatureRecord {
            id: runtime.id.to_string(),
            label: runtime.label.to_string(),
            description: runtime.description.to_string(),
            default_version: default_version(runtime).id.to_string(),
            versions: runtime
                .versions
                .iter()
                .map(|version| SandboxImageRuntimeVersionRecord {
                    id: version.id.to_string(),
                    label: version.label.to_string(),
                    description: version.description.to_string(),
                    default: version.default,
                })
                .collect(),
        })
        .collect()
}

pub(super) fn canonical_features(features: &[String]) -> Result<Vec<RuntimeSelectionSpec>, String> {
    let mut selections = Vec::new();
    for feature in features {
        let Some(selection) = parse_runtime_selection(feature)? else {
            continue;
        };
        if selections
            .iter()
            .any(|existing: &RuntimeSelectionSpec| existing.runtime.id == selection.runtime.id)
        {
            return Err(format!(
                "runtime {} is selected more than once",
                selection.runtime.label
            ));
        }
        selections.push(selection);
    }

    selections.sort_by_key(|selection| runtime_index(selection.runtime.id));
    Ok(selections)
}

pub(super) fn parse_generated_image_id(image_id: &str) -> Option<ParsedImageId> {
    if image_id == "base" {
        return Some(ParsedImageId {
            selections: Vec::new(),
            custom_script_hash: None,
        });
    }
    let suffix = image_id.strip_prefix("dev-")?;
    if suffix.trim().is_empty() {
        return None;
    }

    let mut selections = Vec::new();
    let mut custom_script_hash = None;
    for segment in suffix.split('-') {
        if let Some(hash) = parse_custom_script_segment(segment) {
            if custom_script_hash.is_some() {
                return None;
            }
            custom_script_hash = Some(hash);
            continue;
        }
        let selection = parse_image_id_segment(segment)?;
        if selections
            .iter()
            .any(|existing: &RuntimeSelectionSpec| existing.runtime.id == selection.runtime.id)
        {
            return None;
        }
        selections.push(selection);
    }
    selections.sort_by_key(|selection| runtime_index(selection.runtime.id));
    Some(ParsedImageId {
        selections,
        custom_script_hash,
    })
}

pub(super) fn selection_feature_token(selection: &RuntimeSelectionSpec) -> String {
    format!("{}@{}", selection.runtime.id, selection.version.id)
}

pub(super) fn selection_label(selection: &RuntimeSelectionSpec) -> &'static str {
    selection.version.label
}

fn parse_runtime_selection(value: &str) -> Result<Option<RuntimeSelectionSpec>, String> {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty() {
        return Ok(None);
    }

    let (runtime_raw, version_raw) = split_runtime_version(value.as_str());
    let runtime = find_runtime(runtime_raw)
        .ok_or_else(|| format!("unknown sandbox image runtime: {runtime_raw}"))?;
    let version = match version_raw {
        Some(version) => find_runtime_version(runtime, version).ok_or_else(|| {
            format!(
                "unknown {} version: {}; supported versions are {}",
                runtime.label,
                version,
                runtime
                    .versions
                    .iter()
                    .map(|item| item.id)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?,
        None => default_version(runtime),
    };
    Ok(Some(RuntimeSelectionSpec { runtime, version }))
}

fn parse_custom_script_segment(segment: &str) -> Option<String> {
    let hash = segment.trim().strip_prefix("script")?;
    if hash.len() < 8 || hash.len() > 32 || !hash.chars().all(|item| item.is_ascii_hexdigit()) {
        return None;
    }
    Some(hash.to_ascii_lowercase())
}

fn parse_image_id_segment(segment: &str) -> Option<RuntimeSelectionSpec> {
    let segment = segment.trim().to_ascii_lowercase();
    for runtime in IMAGE_RUNTIMES {
        let mut names = std::iter::once(runtime.id)
            .chain(runtime.aliases.iter().copied())
            .collect::<Vec<_>>();
        names.sort_by_key(|name| std::cmp::Reverse(name.len()));
        for name in names {
            if let Some(version) = segment.strip_prefix(name) {
                let version = if version.is_empty() {
                    default_version(runtime)
                } else {
                    find_runtime_version(runtime, version)?
                };
                return Some(RuntimeSelectionSpec { runtime, version });
            }
        }
    }
    None
}

fn split_runtime_version(value: &str) -> (&str, Option<&str>) {
    if let Some((runtime, version)) = value.split_once('@').or_else(|| value.split_once(':')) {
        return (runtime.trim(), Some(version.trim()));
    }

    for runtime in IMAGE_RUNTIMES {
        for name in std::iter::once(runtime.id).chain(runtime.aliases.iter().copied()) {
            if let Some(version) = value.strip_prefix(name) {
                if !version.is_empty() {
                    return (runtime.id, Some(version.trim_matches(['-', '_', '@', ':'])));
                }
            }
        }
    }

    (value, None)
}

fn find_runtime(value: &str) -> Option<RuntimeSpec> {
    let value = value.trim();
    IMAGE_RUNTIMES
        .into_iter()
        .find(|runtime| runtime.id == value || runtime.aliases.contains(&value))
}

fn find_runtime_version(runtime: RuntimeSpec, value: &str) -> Option<RuntimeVersionSpec> {
    let value = value.trim().trim_start_matches('v');
    runtime
        .versions
        .iter()
        .find(|version| {
            version.id == value
                || format!("{}{}", runtime.id, version.id) == value
                || format!("{}{}", runtime.label.to_ascii_lowercase(), version.id) == value
        })
        .copied()
}

fn default_version(runtime: RuntimeSpec) -> RuntimeVersionSpec {
    runtime
        .versions
        .iter()
        .find(|version| version.default)
        .copied()
        .unwrap_or(runtime.versions[0])
}

fn runtime_index(runtime_id: &str) -> usize {
    IMAGE_RUNTIMES
        .iter()
        .position(|runtime| runtime.id == runtime_id)
        .unwrap_or(usize::MAX)
}
