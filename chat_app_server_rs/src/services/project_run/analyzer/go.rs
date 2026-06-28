use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::models::project_run::ProjectRunTarget;
use regex::Regex;

use super::scan_budget::{read_to_string_limited, ScanBudget, MAX_SOURCE_PROBE_BYTES};
use super::target_model::{build_target, push_target};

pub(super) fn detect_go_targets(
    dir: &Path,
    files: &HashSet<String>,
    out: &mut Vec<ProjectRunTarget>,
    budget: &mut ScanBudget,
) -> Result<(), String> {
    if !files.contains("go.mod") {
        return Ok(());
    }
    let cwd = dir.to_string_lossy().to_string();
    let manifest_path = Some(dir.join("go.mod").to_string_lossy().to_string());
    let go_entrypoints = detect_go_entrypoints_with_budget(dir, budget)?;
    if go_entrypoints.is_empty() {
        push_target(
            out,
            build_target(
                cwd.as_str(),
                "Go: run".to_string(),
                "go",
                "go run .".to_string(),
                0.86,
                Some(".".to_string()),
                manifest_path.clone(),
                vec!["go"],
            ),
        );
    } else {
        for entrypoint in go_entrypoints {
            push_target(
                out,
                build_target(
                    cwd.as_str(),
                    format!("Go: {}", entrypoint),
                    "go",
                    format!("go run {}", entrypoint),
                    0.9,
                    Some(entrypoint.clone()),
                    manifest_path.clone(),
                    vec!["go"],
                ),
            );
        }
    }
    push_target(
        out,
        build_target(
            cwd.as_str(),
            "Go: test".to_string(),
            "go",
            "go test ./...".to_string(),
            0.8,
            None,
            manifest_path,
            vec!["go"],
        ),
    );
    Ok(())
}

pub(in crate::services::project_run) fn detect_go_entrypoints(dir: &Path) -> Vec<String> {
    let mut budget = ScanBudget::for_project_run_analysis();
    detect_go_entrypoints_with_budget(dir, &mut budget).unwrap_or_default()
}

fn detect_go_entrypoints_with_budget(
    dir: &Path,
    budget: &mut ScanBudget,
) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let main_re = Regex::new(r"(?m)^\s*func\s+main\s*\(").ok();

    let cmd_dir = dir.join("cmd");
    if cmd_dir.is_dir() {
        for entry in fs::read_dir(&cmd_dir).into_iter().flatten().flatten() {
            budget.account_entry()?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let mut has_main = false;
            for item in walkdir::WalkDir::new(&path)
                .max_depth(2)
                .into_iter()
                .flatten()
            {
                budget.account_entry()?;
                if !item.file_type().is_file() {
                    continue;
                }
                if item.path().extension().and_then(|value| value.to_str()) != Some("go") {
                    continue;
                }
                if go_file_contains_main(item.path(), main_re.as_ref()) {
                    has_main = true;
                    break;
                }
            }
            if !has_main {
                continue;
            }
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            let rel = format!("./cmd/{name}");
            if seen.insert(rel.clone()) {
                out.push(rel);
            }
        }
    }

    if seen.is_empty() {
        let mut has_root_main = false;
        for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
            budget.account_entry()?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if path.extension().and_then(|value| value.to_str()) != Some("go") {
                continue;
            }
            if go_file_contains_main(&path, main_re.as_ref()) {
                has_root_main = true;
                break;
            }
        }
        if has_root_main {
            out.push(".".to_string());
        }
    }

    out.sort();
    Ok(out)
}

fn go_file_contains_main(path: &Path, main_re: Option<&Regex>) -> bool {
    read_to_string_limited(path, MAX_SOURCE_PROBE_BYTES)
        .zip(main_re)
        .is_some_and(|(content, re)| re.is_match(&content))
}
