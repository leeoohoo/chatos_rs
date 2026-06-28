use crate::models::project_run::ProjectRunTarget;

pub(super) const MAX_TARGETS: usize = 32;

pub(in crate::services::project_run) fn normalized_cwd(path: &str) -> String {
    let trimmed = path.trim().trim_end_matches(&['/', '\\'][..]);
    if trimmed.is_empty() {
        path.trim().to_string()
    } else {
        trimmed.to_string()
    }
}

pub(super) fn normalize_confidence(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

pub(super) fn target_id_from(cwd: &str, command: &str) -> String {
    use sha2::{Digest, Sha256};
    let raw = format!("{}\n{}", normalized_cwd(cwd), command.trim());
    let hash = Sha256::digest(raw.as_bytes());
    let hex = hex::encode(hash);
    format!("auto_{}", &hex[..12])
}

pub(in crate::services::project_run) fn is_same_cwd(left: &str, right: &str) -> bool {
    normalized_cwd(left) == normalized_cwd(right)
}

pub(super) fn push_target(out: &mut Vec<ProjectRunTarget>, target: ProjectRunTarget) {
    if out.len() >= MAX_TARGETS {
        return;
    }
    if out.iter().any(|item| {
        is_same_cwd(item.cwd.as_str(), target.cwd.as_str()) && item.command == target.command
    }) {
        return;
    }
    out.push(target);
}

pub(super) fn build_target(
    cwd: &str,
    label: String,
    kind: &str,
    command: String,
    confidence: f64,
    entrypoint: Option<String>,
    manifest_path: Option<String>,
    required_toolchains: Vec<&str>,
) -> ProjectRunTarget {
    ProjectRunTarget {
        id: target_id_from(cwd, command.as_str()),
        label,
        kind: kind.to_string(),
        language: Some(kind.to_string()),
        cwd: cwd.to_string(),
        command,
        source: "auto".to_string(),
        confidence,
        is_default: false,
        entrypoint,
        manifest_path,
        required_toolchains: required_toolchains
            .into_iter()
            .map(|value| value.to_string())
            .collect(),
    }
}
