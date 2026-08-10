// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

mod mutation;

use self::mutation::validate_file_mutation_contract;

const NODE_DEPENDENCY_MARKERS: [&str; 9] = [
    "node_modules/.package-lock.json",
    "node_modules/.modules.yaml",
    "node_modules/.pnpm/lock.yaml",
    "node_modules/.yarn-state.yml",
    "node_modules/.yarn-integrity",
    "node_modules/.bin",
    ".pnp.cjs",
    ".pnp.loader.mjs",
    ".yarn/install-state.gz",
];

struct NodeDependencyContext {
    package_manifest: PathBuf,
    lockfile: Option<PathBuf>,
    package_manager: &'static str,
}

pub(super) fn validate_command_preflight(
    workspace_root: &Path,
    requested_path: &str,
    command: &str,
) -> Result<(), String> {
    validate_file_mutation_contract(workspace_root, command)?;
    if !contains_node_validation(command) {
        return Ok(());
    }
    if command_masks_failure(command) {
        return Err(structured_error(json!({
            "category": "validation",
            "error": "Validation commands must preserve their original exit status",
            "reason": "unconditional_failure_masking",
            "command": command,
            "recovery": {
                "required_next_tool": "execute_command",
                "guidance": "Run the validation command without `|| true` or an equivalent unconditional success mask."
            }
        })));
    }
    if !node_validation_runs_before_install(command) {
        return Ok(());
    }
    let workspace_root = std::fs::canonicalize(workspace_root).map_err(|err| {
        structured_error(json!({
            "category": "infrastructure",
            "error": "The current project workspace is unavailable during command preflight",
            "reason": "workspace_unavailable",
            "detail": err.to_string()
        }))
    })?;

    let Some(target_dir) = resolve_existing_target(workspace_root.as_path(), requested_path) else {
        return Ok(());
    };
    let Some(context) =
        discover_node_dependency_context(workspace_root.as_path(), target_dir.as_path())
    else {
        return Ok(());
    };
    if dependency_installation_exists(
        workspace_root.as_path(),
        target_dir.as_path(),
        context.package_manager,
    ) {
        return Ok(());
    }

    let package_manifest =
        workspace_relative(workspace_root.as_path(), context.package_manifest.as_path());
    let lockfile = context
        .lockfile
        .as_deref()
        .map(|path| workspace_relative(workspace_root.as_path(), path));
    let install_command = recommended_install_command(
        context.package_manager,
        context.lockfile.as_deref().is_some(),
    );
    Err(structured_error(json!({
        "category": "dependency_precheck",
        "error": "Node.js dependencies are not installed for this validation command",
        "command_executed": false,
        "path": requested_path,
        "package_manifest": package_manifest,
        "lockfile": lockfile,
        "package_manager": context.package_manager,
        "checked_installation_markers": NODE_DEPENDENCY_MARKERS,
        "recovery": {
            "required_next_tool": "execute_command",
            "recommended_command": install_command,
            "guidance": "Install dependencies successfully, then run the original validation command unchanged."
        }
    })))
}

fn resolve_existing_target(workspace_root: &Path, requested_path: &str) -> Option<PathBuf> {
    let root = std::fs::canonicalize(workspace_root).ok()?;
    let requested = Path::new(requested_path.trim());
    if requested.is_absolute() {
        return None;
    }
    let target = std::fs::canonicalize(root.join(requested)).ok()?;
    (target.is_dir() && target.starts_with(root.as_path())).then_some(target)
}

fn discover_node_dependency_context(
    workspace_root: &Path,
    target_dir: &Path,
) -> Option<NodeDependencyContext> {
    let root = std::fs::canonicalize(workspace_root).ok()?;
    let mut current = target_dir.to_path_buf();
    let mut package_manifest = None;
    let mut package_manager = None;
    let mut lockfile = None;

    loop {
        let manifest = current.join("package.json");
        if manifest.is_file() {
            if package_manifest.is_none() {
                package_manifest = Some(manifest.clone());
            }
            if package_manager.is_none() {
                package_manager = package_manager_from_manifest(manifest.as_path());
            }
        }
        if lockfile.is_none() {
            lockfile = lockfile_in(current.as_path(), package_manager);
        }
        if current == root {
            break;
        }
        let parent = current.parent()?;
        if !parent.starts_with(root.as_path()) {
            break;
        }
        current = parent.to_path_buf();
    }

    let package_manifest = package_manifest?;
    let package_manager = package_manager
        .or_else(|| lockfile.as_deref().map(package_manager_from_lockfile))
        .unwrap_or("npm");
    Some(NodeDependencyContext {
        package_manifest,
        lockfile,
        package_manager,
    })
}

fn dependency_installation_exists(
    workspace_root: &Path,
    target_dir: &Path,
    package_manager: &str,
) -> bool {
    let Ok(root) = std::fs::canonicalize(workspace_root) else {
        return false;
    };
    let mut current = target_dir.to_path_buf();
    loop {
        if NODE_DEPENDENCY_MARKERS
            .iter()
            .any(|marker| current.join(marker).exists())
            || (package_manager == "bun" && current.join("node_modules").is_dir())
        {
            return true;
        }
        if current == root {
            return false;
        }
        let Some(parent) = current.parent() else {
            return false;
        };
        if !parent.starts_with(root.as_path()) {
            return false;
        }
        current = parent.to_path_buf();
    }
}

fn lockfile_in(dir: &Path, preferred_manager: Option<&'static str>) -> Option<PathBuf> {
    let candidates = match preferred_manager {
        Some("pnpm") => [
            "pnpm-lock.yaml",
            "package-lock.json",
            "yarn.lock",
            "bun.lock",
        ],
        Some("yarn") => [
            "yarn.lock",
            "package-lock.json",
            "pnpm-lock.yaml",
            "bun.lock",
        ],
        Some("bun") => [
            "bun.lock",
            "bun.lockb",
            "package-lock.json",
            "pnpm-lock.yaml",
        ],
        _ => [
            "package-lock.json",
            "pnpm-lock.yaml",
            "yarn.lock",
            "bun.lock",
        ],
    };
    candidates
        .into_iter()
        .map(|name| dir.join(name))
        .find(|path| path.is_file())
        .or_else(|| {
            let legacy_bun_lock = dir.join("bun.lockb");
            legacy_bun_lock.is_file().then_some(legacy_bun_lock)
        })
}

fn package_manager_from_manifest(path: &Path) -> Option<&'static str> {
    let content = std::fs::read_to_string(path).ok()?;
    let manifest: Value = serde_json::from_str(content.as_str()).ok()?;
    let package_manager = manifest
        .get("packageManager")?
        .as_str()?
        .to_ascii_lowercase();
    if package_manager.starts_with("pnpm@") {
        Some("pnpm")
    } else if package_manager.starts_with("yarn@") {
        Some("yarn")
    } else if package_manager.starts_with("bun@") {
        Some("bun")
    } else if package_manager.starts_with("npm@") {
        Some("npm")
    } else {
        None
    }
}

fn package_manager_from_lockfile(path: &Path) -> &'static str {
    match path.file_name().and_then(|name| name.to_str()) {
        Some("pnpm-lock.yaml") => "pnpm",
        Some("yarn.lock") => "yarn",
        Some("bun.lock" | "bun.lockb") => "bun",
        _ => "npm",
    }
}

fn recommended_install_command(package_manager: &str, has_lockfile: bool) -> &'static str {
    match (package_manager, has_lockfile) {
        ("pnpm", true) => "pnpm install --frozen-lockfile --ignore-scripts",
        ("pnpm", false) => "pnpm install --ignore-scripts",
        ("yarn", true) => "yarn install --immutable --mode=skip-builds",
        ("yarn", false) => "yarn install --mode=skip-builds",
        ("bun", true) => "bun install --frozen-lockfile --ignore-scripts",
        ("bun", false) => "bun install --ignore-scripts",
        ("npm", true) => "npm ci --ignore-scripts",
        _ => "npm install --ignore-scripts",
    }
}

fn node_validation_runs_before_install(command: &str) -> bool {
    let mut install_seen = false;
    for segment in shell_segments(command) {
        if is_node_install_segment(segment) {
            install_seen = true;
            continue;
        }
        if is_node_validation_segment(segment) {
            return !install_seen;
        }
    }
    false
}

fn contains_node_validation(command: &str) -> bool {
    shell_segments(command)
        .into_iter()
        .any(is_node_validation_segment)
}

fn shell_segments(command: &str) -> Vec<&str> {
    command
        .split([';', '\n'])
        .flat_map(|part| part.split("&&"))
        .flat_map(|part| part.split("||"))
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect()
}

fn is_node_install_segment(segment: &str) -> bool {
    let segment = normalized_command_segment(segment);
    [
        "npm install",
        "npm i ",
        "npm ci",
        "pnpm install",
        "pnpm i ",
        "yarn install",
        "bun install",
    ]
    .iter()
    .any(|prefix| command_starts_with(segment.as_str(), prefix))
}

fn is_node_validation_segment(segment: &str) -> bool {
    let segment = normalized_command_segment(segment);
    [
        "npm test",
        "npm run ",
        "npm exec ",
        "npx ",
        "pnpm test",
        "pnpm run ",
        "pnpm exec ",
        "pnpx ",
        "yarn test",
        "yarn run ",
        "yarn exec ",
        "bun test",
        "bun run ",
        "bunx ",
        "tsc",
        "vitest",
        "vite ",
        "next ",
        "eslint ",
        "jest",
        "./node_modules/.bin/",
    ]
    .iter()
    .any(|prefix| command_starts_with(segment.as_str(), prefix))
}

fn normalized_command_segment(segment: &str) -> String {
    let mut tokens = segment.split_whitespace().peekable();
    if tokens.peek().copied() == Some("env") {
        tokens.next();
    }
    while tokens
        .peek()
        .copied()
        .is_some_and(is_environment_assignment)
    {
        tokens.next();
    }
    if tokens.peek().copied() == Some("corepack") {
        tokens.next();
    }
    tokens.collect::<Vec<_>>().join(" ").to_ascii_lowercase()
}

fn is_environment_assignment(token: &str) -> bool {
    let Some((name, _value)) = token.split_once('=') else {
        return false;
    };
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
        && name
            .chars()
            .next()
            .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
}

fn command_starts_with(command: &str, prefix: &str) -> bool {
    let exact = prefix.trim_end();
    if command == exact {
        return true;
    }
    if prefix.ends_with(' ') || prefix.ends_with('/') {
        return command.starts_with(prefix);
    }
    command
        .strip_prefix(exact)
        .is_some_and(|suffix| suffix.chars().next().is_some_and(char::is_whitespace))
}

fn command_masks_failure(command: &str) -> bool {
    let compact = command
        .to_ascii_lowercase()
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    compact.contains("||true") || compact.contains("||:")
}

fn workspace_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn structured_error(payload: Value) -> String {
    serde_json::to_string(&payload)
        .unwrap_or_else(|_| "terminal command preflight failed".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> PathBuf {
        std::env::temp_dir().join(format!("terminal-node-preflight-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn missing_node_dependencies_block_validation_before_execution() {
        let root = temp_root();
        std::fs::create_dir_all(root.join("apps/web")).expect("create workspace");
        std::fs::write(
            root.join("package.json"),
            r#"{"packageManager":"pnpm@10.0.0"}"#,
        )
        .expect("write manifest");
        std::fs::write(root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n")
            .expect("write lockfile");
        std::fs::write(root.join("apps/web/package.json"), "{}\n").expect("write child manifest");

        let error = validate_command_preflight(root.as_path(), "apps/web", "pnpm test")
            .expect_err("missing dependencies must block validation");
        let payload: Value = serde_json::from_str(error.as_str()).expect("structured error");

        assert_eq!(payload["category"], "dependency_precheck");
        assert_eq!(payload["command_executed"], false);
        assert_eq!(payload["package_manager"], "pnpm");
        assert_eq!(payload["lockfile"], "pnpm-lock.yaml");
        assert_eq!(
            payload["recovery"]["recommended_command"],
            "pnpm install --frozen-lockfile --ignore-scripts"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn installed_workspace_allows_node_validation() {
        let root = temp_root();
        std::fs::create_dir_all(root.join("node_modules")).expect("create dependencies");
        std::fs::write(root.join("node_modules/.package-lock.json"), "{}\n")
            .expect("write install state");
        std::fs::write(root.join("package.json"), "{}\n").expect("write manifest");
        std::fs::write(root.join("package-lock.json"), "{}\n").expect("write lockfile");

        assert!(validate_command_preflight(root.as_path(), ".", "npm test").is_ok());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn empty_node_modules_directory_does_not_bypass_preflight() {
        let root = temp_root();
        std::fs::create_dir_all(root.join("node_modules")).expect("create empty dependencies");
        std::fs::write(root.join("package.json"), "{}\n").expect("write manifest");
        std::fs::write(root.join("package-lock.json"), "{}\n").expect("write lockfile");

        let error = validate_command_preflight(root.as_path(), ".", "npm run build")
            .expect_err("empty dependency directory must not pass preflight");
        let payload: Value = serde_json::from_str(error.as_str()).expect("structured error");
        assert_eq!(payload["category"], "dependency_precheck");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn install_then_validation_is_allowed_in_one_command() {
        let root = temp_root();
        std::fs::create_dir_all(&root).expect("create workspace");
        std::fs::write(root.join("package.json"), "{}\n").expect("write manifest");
        std::fs::write(root.join("package-lock.json"), "{}\n").expect("write lockfile");

        assert!(validate_command_preflight(
            root.as_path(),
            ".",
            "npm ci --ignore-scripts && npm test"
        )
        .is_ok());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn validation_cannot_mask_failure_with_or_true() {
        let root = temp_root();
        std::fs::create_dir_all(&root).expect("create workspace");
        let error = validate_command_preflight(root.as_path(), ".", "npm test || true")
            .expect_err("masked validation must fail");
        let payload: Value = serde_json::from_str(error.as_str()).expect("structured error");

        assert_eq!(payload["category"], "validation");
        assert_eq!(payload["reason"], "unconditional_failure_masking");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn environment_and_corepack_prefixes_do_not_bypass_preflight() {
        let root = temp_root();
        std::fs::create_dir_all(&root).expect("create workspace");
        std::fs::write(
            root.join("package.json"),
            r#"{"packageManager":"pnpm@10.0.0"}"#,
        )
        .expect("write manifest");
        std::fs::write(root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n")
            .expect("write lockfile");

        for command in ["CI=1 pnpm test", "env CI=1 corepack pnpm run build"] {
            let error = validate_command_preflight(root.as_path(), ".", command)
                .expect_err("prefixed validation must still run dependency preflight");
            let payload: Value = serde_json::from_str(error.as_str()).expect("structured error");
            assert_eq!(payload["category"], "dependency_precheck");
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn unrelated_commands_are_not_treated_as_node_validation() {
        let root = temp_root();
        std::fs::create_dir_all(&root).expect("create workspace");
        std::fs::write(root.join("package.json"), "{}\n").expect("write manifest");

        assert!(
            validate_command_preflight(root.as_path(), ".", "rg 'npm run build' README.md").is_ok()
        );
        assert!(validate_command_preflight(root.as_path(), ".", "tsconfig-paths --help").is_ok());
        let _ = std::fs::remove_dir_all(root);
    }
}
