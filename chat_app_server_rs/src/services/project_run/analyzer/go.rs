use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::models::project_run::ProjectRunTarget;
use regex::Regex;

use super::target_model::{build_target, push_target};

pub(super) fn detect_go_targets(
    dir: &Path,
    files: &HashSet<String>,
    out: &mut Vec<ProjectRunTarget>,
) {
    if !files.contains("go.mod") {
        return;
    }
    let cwd = dir.to_string_lossy().to_string();
    let manifest_path = Some(dir.join("go.mod").to_string_lossy().to_string());
    let go_entrypoints = detect_go_entrypoints(dir);
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
}

pub(in crate::services::project_run) fn detect_go_entrypoints(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let main_re = Regex::new(r"(?m)^\s*func\s+main\s*\(").ok();

    let cmd_dir = dir.join("cmd");
    if cmd_dir.is_dir() {
        for entry in fs::read_dir(&cmd_dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let has_main = walkdir::WalkDir::new(&path)
                .max_depth(2)
                .into_iter()
                .flatten()
                .filter(|item| item.file_type().is_file())
                .filter(|item| {
                    item.path().extension().and_then(|value| value.to_str()) == Some("go")
                })
                .any(|item| {
                    fs::read_to_string(item.path())
                        .ok()
                        .zip(main_re.as_ref())
                        .is_some_and(|(content, re)| re.is_match(&content))
                });
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
        let has_root_main = fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.path().is_file())
            .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("go"))
            .any(|entry| {
                fs::read_to_string(entry.path())
                    .ok()
                    .zip(main_re.as_ref())
                    .is_some_and(|(content, re)| re.is_match(&content))
            });
        if has_root_main {
            out.push(".".to_string());
        }
    }

    out.sort();
    out
}
