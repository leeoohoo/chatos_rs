// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;
use std::path::Path;

use crate::models::project_run::ProjectRunTarget;

use super::scan_budget::{read_to_string_limited, ScanBudget, MAX_MANIFEST_BYTES};
use super::target_model::{build_target, push_target};

pub(super) fn detect_rust_targets(
    dir: &Path,
    files: &HashSet<String>,
    out: &mut Vec<ProjectRunTarget>,
    budget: &mut ScanBudget,
) -> Result<(), String> {
    if !files.contains("cargo.toml") {
        return Ok(());
    }
    let cwd = dir.to_string_lossy().to_string();
    let manifest_path = Some(dir.join("Cargo.toml").to_string_lossy().to_string());
    let rust_bins = detect_rust_bins_with_budget(dir, budget)?;
    let has_default_main = dir.join("src").join("main.rs").is_file();
    if has_default_main {
        push_target(
            out,
            build_target(
                cwd.as_str(),
                "Rust: cargo run".to_string(),
                "rust",
                "cargo run".to_string(),
                0.86,
                Some("src/main.rs".to_string()),
                manifest_path.clone(),
                vec!["cargo"],
            ),
        );
    } else if rust_bins.is_empty() {
        push_target(
            out,
            build_target(
                cwd.as_str(),
                "Rust: cargo run".to_string(),
                "rust",
                "cargo run".to_string(),
                0.8,
                None,
                manifest_path.clone(),
                vec!["cargo"],
            ),
        );
    }
    for bin_name in rust_bins {
        let entrypoint = if bin_name.contains('/') {
            format!("src/bin/{}/main.rs", bin_name)
        } else {
            format!("src/bin/{}.rs", bin_name)
        };
        push_target(
            out,
            build_target(
                cwd.as_str(),
                format!("Rust: {}", bin_name),
                "rust",
                format!("cargo run --bin {}", bin_name),
                0.93,
                Some(entrypoint),
                manifest_path.clone(),
                vec!["cargo"],
            ),
        );
    }
    push_target(
        out,
        build_target(
            cwd.as_str(),
            "Rust: cargo test".to_string(),
            "rust",
            "cargo test".to_string(),
            0.8,
            None,
            manifest_path,
            vec!["cargo"],
        ),
    );
    Ok(())
}

pub(in crate::services::project_run) fn detect_rust_bins(dir: &Path) -> Vec<String> {
    let mut budget = ScanBudget::for_project_run_analysis();
    detect_rust_bins_with_budget(dir, &mut budget).unwrap_or_default()
}

fn detect_rust_bins_with_budget(
    dir: &Path,
    budget: &mut ScanBudget,
) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    let src_bin_dir = dir.join("src").join("bin");
    if src_bin_dir.is_dir() {
        for entry in walkdir::WalkDir::new(&src_bin_dir)
            .max_depth(2)
            .into_iter()
            .flatten()
        {
            budget.account_entry()?;
            if !entry.file_type().is_file() {
                continue;
            }
            if entry.path().extension().and_then(|value| value.to_str()) != Some("rs") {
                continue;
            }
            let relative = match entry.path().strip_prefix(&src_bin_dir) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if relative.file_name().and_then(|value| value.to_str()) == Some("main.rs") {
                let Some(parent) = relative.parent() else {
                    continue;
                };
                let name = parent.to_string_lossy().trim().replace('\\', "/");
                if !name.is_empty() && seen.insert(name.clone()) {
                    out.push(name);
                }
                continue;
            }
            let Some(stem) = entry.path().file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            let stem = stem.trim();
            if !stem.is_empty() && seen.insert(stem.to_string()) {
                out.push(stem.to_string());
            }
        }
    }

    let cargo_toml = dir.join("Cargo.toml");
    if cargo_toml.is_file() {
        if let Some(content) = read_to_string_limited(&cargo_toml, MAX_MANIFEST_BYTES) {
            for raw_line in content.lines() {
                let line = raw_line.trim();
                if line == "[[bin]]" {
                    continue;
                }
                if line.starts_with('[') {
                    continue;
                }
                if let Some(rest) = line.strip_prefix("name") {
                    let Some((_, value)) = rest.split_once('=') else {
                        continue;
                    };
                    let normalized = value.trim().trim_matches('"').trim_matches('\'').trim();
                    if normalized.is_empty() {
                        continue;
                    }
                    if seen.insert(normalized.to_string()) {
                        out.push(normalized.to_string());
                    }
                }
            }
        }
    }

    out.sort();
    Ok(out)
}
