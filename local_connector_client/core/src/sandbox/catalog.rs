// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

pub(crate) fn local_sandbox_runtime_specs() -> Vec<Value> {
    json!([
        runtime_spec(
            "java",
            "JDK",
            "OpenJDK development tools",
            "21",
            &[
                ("8", "JDK 8", "Temurin JDK 8 LTS", false),
                ("11", "JDK 11", "Temurin JDK 11 LTS", false),
                ("17", "JDK 17", "Temurin JDK 17 LTS", false),
                ("21", "JDK 21", "Temurin JDK 21 LTS", true),
                ("25", "JDK 25", "Temurin JDK 25 LTS", false),
            ],
        ),
        runtime_spec(
            "node",
            "Node.js",
            "Node.js, npm, pnpm and yarn",
            "24",
            &[
                ("20", "Node.js 20", "Node.js 20 legacy line", false),
                ("22", "Node.js 22", "Node.js 22 LTS", false),
                ("24", "Node.js 24", "Node.js 24 LTS", true),
                ("26", "Node.js 26", "Node.js 26 current line", false),
            ],
        ),
        runtime_spec(
            "python",
            "Python",
            "Python interpreter, pip and venv tooling",
            "3.14",
            &[
                (
                    "3.10",
                    "Python 3.10",
                    "Python 3.10 security support line",
                    false,
                ),
                (
                    "3.11",
                    "Python 3.11",
                    "Python 3.11 security support line",
                    false,
                ),
                (
                    "3.12",
                    "Python 3.12",
                    "Python 3.12 security support line",
                    false,
                ),
                (
                    "3.13",
                    "Python 3.13",
                    "Python 3.13 security support line",
                    false,
                ),
                (
                    "3.14",
                    "Python 3.14",
                    "Python 3.14 active support line",
                    true,
                ),
            ],
        ),
        runtime_spec(
            "rust",
            "Rust",
            "Rust toolchain",
            "stable",
            &[
                (
                    "1.85.1",
                    "Rust 1.85.1",
                    "Pinned Rust 1.85.1 toolchain",
                    false,
                ),
                (
                    "1.88.0",
                    "Rust 1.88.0",
                    "Pinned Rust 1.88.0 toolchain",
                    false,
                ),
                (
                    "1.92.0",
                    "Rust 1.92.0",
                    "Pinned Rust 1.92.0 toolchain",
                    false,
                ),
                (
                    "1.96.1",
                    "Rust 1.96.1",
                    "Pinned Rust 1.96.1 toolchain",
                    false,
                ),
                ("stable", "Stable", "Rust stable channel", true),
                ("beta", "Beta", "Rust beta channel", false),
                ("nightly", "Nightly", "Rust nightly channel", false),
            ],
        ),
        runtime_spec(
            "go",
            "Go",
            "Go toolchain",
            "1.26",
            &[
                ("1.22", "Go 1.22", "Go 1.22 toolchain", false),
                ("1.23", "Go 1.23", "Go 1.23 toolchain", false),
                ("1.24", "Go 1.24", "Go 1.24 toolchain", false),
                ("1.25", "Go 1.25", "Go 1.25 toolchain", false),
                ("1.26", "Go 1.26", "Go 1.26 toolchain", true),
            ],
        ),
        runtime_spec(
            "dotnet",
            ".NET",
            ".NET SDK for C# and F# projects",
            "10.0",
            &[
                ("8.0", ".NET 8", ".NET 8 LTS SDK", false),
                ("9.0", ".NET 9", ".NET 9 STS SDK", false),
                ("10.0", ".NET 10", ".NET 10 LTS SDK", true),
            ],
        ),
        runtime_spec(
            "php",
            "PHP",
            "PHP CLI runtime and Composer",
            "8.4",
            &[
                ("8.2", "PHP 8.2", "PHP 8.2 security support line", false),
                ("8.3", "PHP 8.3", "PHP 8.3 security support line", false),
                ("8.4", "PHP 8.4", "PHP 8.4 active support line", true),
                ("8.5", "PHP 8.5", "PHP 8.5 active support line", false),
            ],
        ),
        runtime_spec(
            "ruby",
            "Ruby",
            "Ruby runtime, RubyGems and Bundler",
            "3.4.10",
            &[
                ("3.2.11", "Ruby 3.2.11", "Ruby 3.2 maintenance line", false),
                ("3.3.11", "Ruby 3.3.11", "Ruby 3.3 maintenance line", false),
                ("3.4.10", "Ruby 3.4.10", "Ruby 3.4 stable line", true),
                ("4.0.5", "Ruby 4.0.5", "Ruby 4.0 current line", false),
            ],
        ),
        runtime_spec(
            "gcc",
            "C/C++ (GCC)",
            "GNU C and C++ compiler toolchain",
            "14",
            &[
                ("13", "GCC 13", "GNU C/C++ compiler 13", false),
                ("14", "GCC 14", "GNU C/C++ compiler 14", true),
            ],
        ),
        runtime_spec(
            "clang",
            "C/C++ (Clang)",
            "LLVM, Clang, LLD and Clangd toolchain",
            "20",
            &[
                ("18", "Clang 18", "LLVM/Clang 18 toolchain", false),
                ("19", "Clang 19", "LLVM/Clang 19 toolchain", false),
                ("20", "Clang 20", "LLVM/Clang 20 toolchain", true),
            ],
        ),
    ])
    .as_array()
    .cloned()
    .unwrap_or_default()
}

fn runtime_spec(
    id: &str,
    label: &str,
    description: &str,
    default_version: &str,
    versions: &[(&str, &str, &str, bool)],
) -> Value {
    json!({
        "id": id,
        "label": label,
        "description": description,
        "default_version": default_version,
        "versions": versions.iter().map(|(id, label, description, default)| json!({
            "id": id,
            "label": label,
            "description": description,
            "default": default,
        })).collect::<Vec<_>>()
    })
}
