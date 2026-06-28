use std::fs;
use std::path::Path;

use crate::models::project_run::ProjectRunTarget;

use super::target_model::{build_target, normalize_confidence, push_target};

pub(super) fn detect_node_targets(dir: &Path, out: &mut Vec<ProjectRunTarget>) {
    let package_json = dir.join("package.json");
    if !package_json.is_file() {
        return;
    }
    let cwd = dir.to_string_lossy().to_string();
    let manifest_path = Some(package_json.to_string_lossy().to_string());
    let package_manager = if dir.join("pnpm-lock.yaml").is_file() {
        "pnpm"
    } else if dir.join("yarn.lock").is_file() {
        "yarn"
    } else {
        "npm"
    };
    let required_toolchains = match package_manager {
        "pnpm" => vec!["node", "pnpm"],
        "yarn" => vec!["node", "yarn"],
        _ => vec!["node", "npm"],
    };
    let raw = fs::read_to_string(&package_json).unwrap_or_default();
    let parsed = serde_json::from_str::<serde_json::Value>(&raw).ok();
    let scripts = parsed
        .as_ref()
        .and_then(|value| value.get("scripts"))
        .and_then(|value| value.as_object());

    let preferred = ["dev", "start", "serve", "test", "build"];
    let mut added = false;
    if let Some(scripts_obj) = scripts {
        for key in preferred {
            if scripts_obj.contains_key(key) {
                push_target(
                    out,
                    build_target(
                        cwd.as_str(),
                        format!("Node: {package_manager} {}", key),
                        "node",
                        if package_manager == "npm" {
                            format!("npm run {}", key)
                        } else {
                            format!("{package_manager} {}", key)
                        },
                        normalize_confidence(if key == "dev" || key == "start" {
                            0.95
                        } else {
                            0.85
                        }),
                        Some(format!("package.json:scripts.{}", key)),
                        manifest_path.clone(),
                        required_toolchains.clone(),
                    ),
                );
                added = true;
            }
        }
    }
    if !added {
        push_target(
            out,
            build_target(
                cwd.as_str(),
                format!("Node: {package_manager} start"),
                "node",
                if package_manager == "npm" {
                    "npm start".to_string()
                } else {
                    format!("{package_manager} start")
                },
                0.7,
                Some("package.json:scripts.start".to_string()),
                manifest_path,
                required_toolchains,
            ),
        );
    }
}
