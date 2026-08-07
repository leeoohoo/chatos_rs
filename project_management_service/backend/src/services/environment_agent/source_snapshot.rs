// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::models::{ProjectRecord, ProjectRuntimeEnvironmentImageRecord, RuntimeServiceRole};
use crate::services::cloud_import::git::{authenticated_git_url, run_git_output};
use crate::services::harness_repo::fetch_harness_api_access;
use crate::state::AppState;

const MAX_RECORDED_PATHS_PER_CATEGORY: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ProjectSourceSnapshot {
    pub(crate) project_id: String,
    pub(crate) provider: String,
    pub(crate) default_branch: String,
    pub(crate) source_commit: String,
    pub(crate) snapshot_id: String,
    pub(crate) scanned_file_count: usize,
    pub(crate) manifests: Vec<String>,
    pub(crate) dockerfiles: Vec<String>,
    pub(crate) compose_files: Vec<String>,
    pub(crate) migration_paths: Vec<String>,
    pub(crate) seed_paths: Vec<String>,
    pub(crate) environment_files: Vec<String>,
    pub(crate) environment_variable_candidates: Vec<String>,
    pub(crate) port_candidates: Vec<u16>,
    pub(crate) application_candidates: Vec<ProjectApplicationCandidate>,
    pub(crate) requires_application_runtime: bool,
    pub(crate) evidence_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ProjectApplicationCandidate {
    pub(crate) source_root: String,
    pub(crate) component_kind: String,
    pub(crate) manifest: String,
    pub(crate) package_name: Option<String>,
    pub(crate) startup_command: Option<String>,
    pub(crate) test_command: Option<String>,
    pub(crate) evidence: Vec<String>,
}

pub(crate) async fn capture_harness_source_snapshot(
    state: &AppState,
    project: &ProjectRecord,
    owner_user_id: &str,
) -> Result<ProjectSourceSnapshot, String> {
    let default_branch = required_project_value(
        project.harness_default_branch.as_deref(),
        "harness_default_branch",
    )?;
    let git_url = required_project_value(project.harness_git_url.as_deref(), "harness_git_url")?;
    let project_space = required_project_value(
        project.harness_space_identifier.as_deref(),
        "harness_space_identifier",
    )?;
    let access = fetch_harness_api_access(state, owner_user_id).await?;
    if access.space_identifier.trim() != project_space {
        return Err("Harness access token owner does not match project Harness space".to_string());
    }
    let authenticated_url = authenticated_git_url(
        git_url.as_str(),
        access.harness_uid.as_str(),
        access.access_token.as_str(),
    )?;
    let branch_ref = format!("refs/heads/{default_branch}");
    let remote = run_git_output(
        vec![
            "ls-remote".to_string(),
            authenticated_url.clone(),
            branch_ref.clone(),
        ],
        None,
        &state.config,
        &[access.access_token.as_str(), authenticated_url.as_str()],
    )
    .await?;
    let source_commit = parse_remote_commit(remote.as_str(), branch_ref.as_str())?;

    let snapshot_dir = create_snapshot_dir(project.id.as_str())?;
    let result = capture_snapshot_inventory(
        state,
        project,
        snapshot_dir.as_path(),
        authenticated_url.as_str(),
        access.access_token.as_str(),
        default_branch.as_str(),
        source_commit.as_str(),
    )
    .await;
    let _ = fs::remove_dir_all(snapshot_dir);
    result
}

async fn capture_snapshot_inventory(
    state: &AppState,
    project: &ProjectRecord,
    snapshot_dir: &Path,
    authenticated_url: &str,
    access_token: &str,
    default_branch: &str,
    source_commit: &str,
) -> Result<ProjectSourceSnapshot, String> {
    run_git_output(
        vec![
            "init".to_string(),
            "--bare".to_string(),
            snapshot_dir.to_string_lossy().to_string(),
        ],
        None,
        &state.config,
        &[access_token, authenticated_url],
    )
    .await?;
    run_git_output(
        vec![
            "remote".to_string(),
            "add".to_string(),
            "origin".to_string(),
            authenticated_url.to_string(),
        ],
        Some(snapshot_dir),
        &state.config,
        &[access_token, authenticated_url],
    )
    .await?;
    run_git_output(
        vec![
            "fetch".to_string(),
            "--depth".to_string(),
            "1".to_string(),
            "origin".to_string(),
            format!("refs/heads/{default_branch}"),
        ],
        Some(snapshot_dir),
        &state.config,
        &[access_token, authenticated_url],
    )
    .await?;
    let fetched_commit = run_git_output(
        vec!["rev-parse".to_string(), "FETCH_HEAD".to_string()],
        Some(snapshot_dir),
        &state.config,
        &[access_token, authenticated_url],
    )
    .await?;
    if fetched_commit.trim() != source_commit {
        return Err(format!(
            "Harness default branch changed while creating the analysis snapshot: expected {source_commit}, fetched {}",
            fetched_commit.trim()
        ));
    }
    let tree = run_git_output(
        vec![
            "ls-tree".to_string(),
            "-r".to_string(),
            "--name-only".to_string(),
            source_commit.to_string(),
        ],
        Some(snapshot_dir),
        &state.config,
        &[access_token, authenticated_url],
    )
    .await?;
    let mut paths = tree
        .lines()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .filter(|path| !is_ignored_repository_path(path))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    if paths.is_empty() {
        return Err("Harness source snapshot contains no readable project files".to_string());
    }
    if paths.len() > state.config.cloud_project_max_files {
        return Err(format!(
            "Harness source snapshot contains {} files, exceeding the configured analysis limit {}",
            paths.len(),
            state.config.cloud_project_max_files
        ));
    }
    build_inventory(
        state,
        project,
        snapshot_dir,
        access_token,
        authenticated_url,
        default_branch,
        source_commit,
        paths,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn build_inventory(
    state: &AppState,
    project: &ProjectRecord,
    snapshot_dir: &Path,
    access_token: &str,
    authenticated_url: &str,
    default_branch: &str,
    source_commit: &str,
    paths: Vec<String>,
) -> Result<ProjectSourceSnapshot, String> {
    let manifests = limited_sorted_paths(paths.iter().filter(|path| is_manifest_path(path)));
    let dockerfiles = limited_sorted_paths(paths.iter().filter(|path| is_dockerfile_path(path)));
    let compose_files = limited_sorted_paths(paths.iter().filter(|path| is_compose_path(path)));
    let migration_paths = limited_sorted_paths(paths.iter().filter(|path| {
        path_has_segment(path, "migration") || path_has_segment(path, "migrations")
    }));
    let seed_paths = limited_sorted_paths(paths.iter().filter(|path| {
        path_has_segment(path, "seed")
            || path_has_segment(path, "seeds")
            || file_name(path).to_ascii_lowercase().contains("seed")
    }));
    let environment_files = limited_sorted_paths(paths.iter().filter(|path| {
        let name = file_name(path).to_ascii_lowercase();
        name == ".env.example"
            || name == ".env.sample"
            || name == ".env.template"
            || name.ends_with(".env.example")
            || name.ends_with(".env.sample")
    }));

    let mut inspected = BTreeMap::new();
    for path in manifests
        .iter()
        .chain(compose_files.iter())
        .chain(environment_files.iter())
    {
        let content = read_snapshot_file(
            state,
            snapshot_dir,
            access_token,
            authenticated_url,
            source_commit,
            path,
        )
        .await?;
        inspected.insert(path.clone(), content);
    }

    let package_manager = infer_package_manager(paths.as_slice(), inspected.get("package.json"));
    let mut applications = Vec::new();
    let mut ports = BTreeSet::new();
    for manifest in &manifests {
        let content = inspected
            .get(manifest)
            .map(String::as_str)
            .unwrap_or_default();
        if let Some(candidate) = application_candidate_from_manifest(
            manifest,
            content,
            package_manager.as_deref(),
            &mut ports,
        )? {
            merge_application_candidate(&mut applications, candidate);
        }
    }
    for dockerfile in &dockerfiles {
        let source_root = parent_path(dockerfile);
        merge_application_candidate(
            &mut applications,
            ProjectApplicationCandidate {
                component_kind: component_kind_from_path(source_root.as_str()).to_string(),
                source_root,
                manifest: dockerfile.clone(),
                package_name: None,
                startup_command: None,
                test_command: None,
                evidence: vec![dockerfile.clone()],
            },
        );
    }

    let mut environment_variables = BTreeSet::new();
    for path in &environment_files {
        if let Some(content) = inspected.get(path) {
            collect_dotenv_variable_names(content, &mut environment_variables);
        }
    }
    for path in &compose_files {
        if let Some(content) = inspected.get(path) {
            collect_compose_ports(content, &mut ports);
            collect_template_variable_names(content, &mut environment_variables);
        }
    }
    applications.sort_by(|left, right| left.source_root.cmp(&right.source_root));

    let evidence_files = manifests
        .iter()
        .chain(dockerfiles.iter())
        .chain(compose_files.iter())
        .chain(environment_files.iter())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .take(MAX_RECORDED_PATHS_PER_CATEGORY)
        .collect::<Vec<_>>();
    let requires_application_runtime = !applications.is_empty()
        || manifests
            .iter()
            .any(|path| is_executable_manifest_path(path))
        || !dockerfiles.is_empty()
        || !compose_files.is_empty();

    Ok(ProjectSourceSnapshot {
        project_id: project.id.clone(),
        provider: "harness".to_string(),
        default_branch: default_branch.to_string(),
        source_commit: source_commit.to_string(),
        snapshot_id: snapshot_id(project.id.as_str(), default_branch, source_commit),
        scanned_file_count: paths.len(),
        manifests,
        dockerfiles,
        compose_files,
        migration_paths,
        seed_paths,
        environment_files,
        environment_variable_candidates: environment_variables.into_iter().collect(),
        port_candidates: ports.into_iter().collect(),
        application_candidates: applications,
        requires_application_runtime,
        evidence_files,
    })
}

pub(crate) fn bind_source_snapshot(detected_stack: &mut Value, snapshot: &ProjectSourceSnapshot) {
    let object = ensure_object(detected_stack);
    object.insert("source_snapshot".to_string(), json!(snapshot));
}

pub(crate) fn preserve_analysis_evidence(existing: &Value, proposed: &mut Value) {
    let proposed = ensure_object(proposed);
    for key in ["source_snapshot", "analysis_progress"] {
        if let Some(value) = existing.get(key).cloned() {
            proposed.insert(key.to_string(), value);
        }
    }
}

pub(crate) fn set_analysis_progress(
    detected_stack: &mut Value,
    run_id: &str,
    stage: &str,
    started_at: &str,
    updated_at: &str,
    finished_at: Option<&str>,
    error: Option<&str>,
) {
    ensure_object(detected_stack).insert(
        "analysis_progress".to_string(),
        json!({
            "run_id": run_id,
            "current_stage": stage,
            "started_at": started_at,
            "updated_at": updated_at,
            "finished_at": finished_at,
            "last_error": error,
        }),
    );
}

pub(crate) fn validate_source_snapshot_coverage(
    detected_stack: &Value,
    images: &[ProjectRuntimeEnvironmentImageRecord],
) -> Result<(), String> {
    let Some(snapshot) = detected_stack.get("source_snapshot") else {
        return Ok(());
    };
    for field in [
        "project_id",
        "default_branch",
        "source_commit",
        "snapshot_id",
    ] {
        if snapshot
            .get(field)
            .and_then(Value::as_str)
            .map(str::trim)
            .is_none_or(str::is_empty)
        {
            return Err(format!(
                "runtime environment source snapshot is missing required field: {field}"
            ));
        }
    }
    if snapshot
        .get("scanned_file_count")
        .and_then(Value::as_u64)
        .unwrap_or_default()
        == 0
    {
        return Err("runtime environment source snapshot contains no scanned files".to_string());
    }

    let application_images = images
        .iter()
        .filter(|image| image.service_role == RuntimeServiceRole::Application)
        .collect::<Vec<_>>();
    let expected_roots = snapshot
        .get("application_candidates")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|candidate| candidate.get("source_root").and_then(Value::as_str))
        .map(normalize_source_root)
        .collect::<BTreeSet<_>>();
    let actual_roots = application_images
        .iter()
        .map(|image| normalize_source_root(image.source_root.as_str()))
        .collect::<BTreeSet<_>>();
    let missing = expected_roots
        .difference(&actual_roots)
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!(
            "runtime environment analysis omitted application candidates confirmed by the source snapshot: {}",
            missing.join(", ")
        ));
    }
    if snapshot
        .get("requires_application_runtime")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && application_images.is_empty()
    {
        return Err(
            "runtime environment analysis returned zero applications even though the source snapshot contains executable project evidence"
                .to_string(),
        );
    }
    Ok(())
}

fn application_candidate_from_manifest(
    manifest: &str,
    content: &str,
    package_manager: Option<&str>,
    ports: &mut BTreeSet<u16>,
) -> Result<Option<ProjectApplicationCandidate>, String> {
    let name = file_name(manifest).to_ascii_lowercase();
    let source_root = parent_path(manifest);
    if name == "package.json" {
        let package: Value = serde_json::from_str(content)
            .map_err(|err| format!("parse Harness snapshot manifest {manifest} failed: {err}"))?;
        let is_workspace_root = source_root == "." && package.get("workspaces").is_some();
        let scripts = package
            .get("scripts")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let dependency_names = package_dependency_names(&package);
        let component_kind = component_kind_for_package(source_root.as_str(), &dependency_names);
        let has_runtime_marker = component_kind != "node_application";
        let has_startup_script = ["start", "dev", "serve"]
            .iter()
            .any(|script| scripts.get(*script).and_then(Value::as_str).is_some());
        if is_workspace_root || (!has_runtime_marker && !has_startup_script) {
            return Ok(None);
        }
        for command in scripts.values().filter_map(Value::as_str) {
            collect_command_ports(command, ports);
        }
        let startup_script = ["start", "dev", "serve"]
            .iter()
            .find(|script| scripts.get(**script).and_then(Value::as_str).is_some())
            .copied();
        let startup_command = startup_script.and_then(|script| {
            package_manager.map(|manager| package_script_command(manager, script))
        });
        let test_command = scripts
            .get("test")
            .and_then(Value::as_str)
            .and_then(|_| package_manager.map(|manager| package_script_command(manager, "test")));
        return Ok(Some(ProjectApplicationCandidate {
            source_root,
            component_kind: component_kind.to_string(),
            manifest: manifest.to_string(),
            package_name: package
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned),
            startup_command,
            test_command,
            evidence: vec![manifest.to_string()],
        }));
    }
    if name == "cargo.toml" && content.contains("[workspace]") && !content.contains("[package]") {
        return Ok(None);
    }
    Ok(Some(ProjectApplicationCandidate {
        component_kind: component_kind_from_path(source_root.as_str()).to_string(),
        source_root,
        manifest: manifest.to_string(),
        package_name: None,
        startup_command: None,
        test_command: None,
        evidence: vec![manifest.to_string()],
    }))
}

fn merge_application_candidate(
    applications: &mut Vec<ProjectApplicationCandidate>,
    candidate: ProjectApplicationCandidate,
) {
    if let Some(existing) = applications
        .iter_mut()
        .find(|existing| existing.source_root == candidate.source_root)
    {
        existing.evidence.extend(candidate.evidence);
        existing.evidence.sort();
        existing.evidence.dedup();
        if existing.package_name.is_none() {
            existing.package_name = candidate.package_name;
        }
        if existing.startup_command.is_none() {
            existing.startup_command = candidate.startup_command;
        }
        if existing.test_command.is_none() {
            existing.test_command = candidate.test_command;
        }
        if existing.component_kind == "application" || existing.component_kind == "node_application"
        {
            existing.component_kind = candidate.component_kind;
        }
        return;
    }
    applications.push(candidate);
}

async fn read_snapshot_file(
    state: &AppState,
    snapshot_dir: &Path,
    access_token: &str,
    authenticated_url: &str,
    source_commit: &str,
    path: &str,
) -> Result<String, String> {
    run_git_output(
        vec!["show".to_string(), format!("{source_commit}:{path}")],
        Some(snapshot_dir),
        &state.config,
        &[access_token, authenticated_url],
    )
    .await
    .map_err(|err| format!("read Harness snapshot file {path} failed: {err}"))
}

fn parse_remote_commit(output: &str, branch_ref: &str) -> Result<String, String> {
    let commit = output.lines().find_map(|line| {
        let (sha, reference) = line.split_once(char::is_whitespace)?;
        (reference.trim() == branch_ref).then(|| sha.trim().to_string())
    });
    let Some(commit) = commit else {
        return Err(format!(
            "Harness default branch does not exist or has no commit: {branch_ref}"
        ));
    };
    if commit.len() < 40
        || !commit
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(format!(
            "Harness default branch returned an invalid commit id: {commit}"
        ));
    }
    Ok(commit)
}

fn create_snapshot_dir(project_id: &str) -> Result<PathBuf, String> {
    let safe_project_id = project_id
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || *character == '-')
        .take(64)
        .collect::<String>();
    let path = std::env::temp_dir().join(format!(
        "chatos-project-analysis-{}-{}",
        safe_project_id,
        Uuid::new_v4()
    ));
    fs::create_dir_all(path.as_path())
        .map_err(|err| format!("create Harness analysis snapshot directory failed: {err}"))?;
    Ok(path)
}

fn required_project_value(value: Option<&str>, field: &str) -> Result<String, String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("project {field} is required for Harness source analysis"))
}

fn snapshot_id(project_id: &str, default_branch: &str, source_commit: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(project_id.as_bytes());
    digest.update([0]);
    digest.update(default_branch.as_bytes());
    digest.update([0]);
    digest.update(source_commit.as_bytes());
    format!("harness-{}", hex::encode(digest.finalize()))
}

fn ensure_object(value: &mut Value) -> &mut serde_json::Map<String, Value> {
    if !value.is_object() {
        *value = json!({});
    }
    value
        .as_object_mut()
        .expect("runtime analysis value is normalized to an object")
}

fn limited_sorted_paths<'a>(paths: impl Iterator<Item = &'a String>) -> Vec<String> {
    paths
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .take(MAX_RECORDED_PATHS_PER_CATEGORY)
        .collect()
}

fn is_ignored_repository_path(path: &str) -> bool {
    path.split('/').any(|segment| {
        matches!(
            segment.to_ascii_lowercase().as_str(),
            ".git"
                | "node_modules"
                | "target"
                | "dist"
                | "build"
                | ".next"
                | ".cache"
                | "coverage"
                | "vendor"
        )
    })
}

fn is_manifest_path(path: &str) -> bool {
    matches!(
        file_name(path).to_ascii_lowercase().as_str(),
        "package.json"
            | "cargo.toml"
            | "pyproject.toml"
            | "requirements.txt"
            | "go.mod"
            | "pom.xml"
            | "build.gradle"
            | "build.gradle.kts"
    )
}

fn is_executable_manifest_path(path: &str) -> bool {
    is_manifest_path(path)
}

fn is_dockerfile_path(path: &str) -> bool {
    let name = file_name(path).to_ascii_lowercase();
    name == "dockerfile" || name.starts_with("dockerfile.") || name.ends_with(".dockerfile")
}

fn is_compose_path(path: &str) -> bool {
    matches!(
        file_name(path).to_ascii_lowercase().as_str(),
        "docker-compose.yml" | "docker-compose.yaml" | "compose.yml" | "compose.yaml"
    )
}

fn path_has_segment(path: &str, expected: &str) -> bool {
    path.split('/')
        .any(|segment| segment.eq_ignore_ascii_case(expected))
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn parent_path(path: &str) -> String {
    path.rsplit_once('/')
        .map(|(parent, _)| normalize_source_root(parent))
        .unwrap_or_else(|| ".".to_string())
}

fn normalize_source_root(path: &str) -> String {
    let path = path.trim().trim_matches('/');
    if path.is_empty() || path == "." {
        ".".to_string()
    } else {
        path.to_string()
    }
}

fn infer_package_manager(paths: &[String], root_package: Option<&String>) -> Option<String> {
    if let Some(manager) = root_package
        .and_then(|content| serde_json::from_str::<Value>(content).ok())
        .and_then(|package| {
            package
                .get("packageManager")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .and_then(|manager| manager.split('@').next().map(ToOwned::to_owned))
        .filter(|manager| matches!(manager.as_str(), "npm" | "pnpm" | "yarn" | "bun"))
    {
        return Some(manager);
    }
    for (lockfile, manager) in [
        ("pnpm-lock.yaml", "pnpm"),
        ("yarn.lock", "yarn"),
        ("bun.lock", "bun"),
        ("bun.lockb", "bun"),
        ("package-lock.json", "npm"),
    ] {
        if paths.iter().any(|path| path == lockfile) {
            return Some(manager.to_string());
        }
    }
    None
}

fn package_dependency_names(package: &Value) -> BTreeSet<String> {
    ["dependencies", "devDependencies", "peerDependencies"]
        .iter()
        .filter_map(|key| package.get(*key).and_then(Value::as_object))
        .flat_map(|dependencies| dependencies.keys())
        .map(|dependency| dependency.to_ascii_lowercase())
        .collect()
}

fn component_kind_for_package(path: &str, dependencies: &BTreeSet<String>) -> &'static str {
    let path_kind = component_kind_from_path(path);
    if path_kind != "application" {
        return path_kind;
    }
    if dependencies.iter().any(|dependency| {
        [
            "react",
            "react-dom",
            "next",
            "vue",
            "nuxt",
            "svelte",
            "@sveltejs/kit",
            "vite",
        ]
        .contains(&dependency.as_str())
    }) {
        return "web";
    }
    if dependencies.iter().any(|dependency| {
        [
            "express",
            "fastify",
            "@nestjs/core",
            "koa",
            "hono",
            "@hapi/hapi",
        ]
        .contains(&dependency.as_str())
    }) {
        return "api";
    }
    "node_application"
}

fn component_kind_from_path(path: &str) -> &'static str {
    let segments = path
        .split('/')
        .map(|segment| segment.to_ascii_lowercase())
        .collect::<Vec<_>>();
    if segments
        .iter()
        .any(|segment| matches!(segment.as_str(), "api" | "server" | "backend"))
    {
        "api"
    } else if segments
        .iter()
        .any(|segment| matches!(segment.as_str(), "web" | "frontend" | "client"))
    {
        "web"
    } else {
        "application"
    }
}

fn package_script_command(manager: &str, script: &str) -> String {
    match manager {
        "yarn" | "bun" => format!("{manager} {script}"),
        _ => format!("{manager} run {script}"),
    }
}

fn collect_command_ports(command: &str, ports: &mut BTreeSet<u16>) {
    let normalized = command.to_ascii_lowercase();
    if !normalized.contains("port") && !normalized.contains("listen") {
        return;
    }
    collect_numeric_ports(command, ports);
}

fn collect_compose_ports(content: &str, ports: &mut BTreeSet<u16>) {
    let mut in_ports = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.ends_with("ports:") {
            in_ports = true;
            continue;
        }
        if in_ports && !trimmed.starts_with('-') && !trimmed.is_empty() {
            in_ports = false;
        }
        if in_ports {
            collect_numeric_ports(trimmed, ports);
        }
    }
}

fn collect_numeric_ports(value: &str, ports: &mut BTreeSet<u16>) {
    for token in value.split(|character: char| !character.is_ascii_digit()) {
        if let Ok(port) = token.parse::<u16>() {
            if port >= 1024 {
                ports.insert(port);
            }
        }
    }
}

fn collect_dotenv_variable_names(content: &str, variables: &mut BTreeSet<String>) {
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let name = line
            .strip_prefix("export ")
            .unwrap_or(line)
            .split_once('=')
            .map(|(name, _)| name.trim())
            .unwrap_or_default();
        if is_environment_variable_name(name) {
            variables.insert(name.to_string());
        }
    }
}

fn collect_template_variable_names(content: &str, variables: &mut BTreeSet<String>) {
    let mut remaining = content;
    while let Some(start) = remaining.find("${") {
        remaining = &remaining[start + 2..];
        let Some(end) = remaining.find('}') else {
            break;
        };
        let expression = &remaining[..end];
        let name = expression
            .split([':', '-', '?'])
            .next()
            .map(str::trim)
            .unwrap_or_default();
        if is_environment_variable_name(name) {
            variables.insert(name.to_string());
        }
        remaining = &remaining[end + 1..];
    }
}

fn is_environment_variable_name(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
        })
        && value
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_uppercase() || character == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        empty_array, empty_object, ProgramManagedMcpPolicy, RuntimeEnvironmentProvider,
    };

    #[test]
    fn servicepulse_manifest_inventory_identifies_api_web_and_runtime_evidence() {
        let paths = vec![
            "package.json".to_string(),
            "pnpm-lock.yaml".to_string(),
            "apps/api/package.json".to_string(),
            "apps/api/src/db/migrations/001_init.sql".to_string(),
            "apps/api/src/db/seed.ts".to_string(),
            "apps/web/package.json".to_string(),
            ".env.example".to_string(),
            "docker-compose.yml".to_string(),
        ];
        assert_eq!(
            infer_package_manager(paths.as_slice(), None).as_deref(),
            Some("pnpm")
        );

        let mut ports = BTreeSet::new();
        let api = application_candidate_from_manifest(
            "apps/api/package.json",
            r#"{"name":"api","scripts":{"dev":"tsx watch src/index.ts --port 4000","test":"vitest"},"dependencies":{"fastify":"1"}}"#,
            Some("pnpm"),
            &mut ports,
        )
        .expect("valid api package")
        .expect("api application");
        let web = application_candidate_from_manifest(
            "apps/web/package.json",
            r#"{"name":"web","scripts":{"dev":"vite --port 5173"},"dependencies":{"react":"1","vite":"1"}}"#,
            Some("pnpm"),
            &mut ports,
        )
        .expect("valid web package")
        .expect("web application");

        assert_eq!(api.component_kind, "api");
        assert_eq!(api.startup_command.as_deref(), Some("pnpm run dev"));
        assert_eq!(web.component_kind, "web");
        assert_eq!(ports, BTreeSet::from([4000, 5173]));
    }

    #[test]
    fn workspace_root_package_is_not_misclassified_as_an_application() {
        let mut ports = BTreeSet::new();
        assert!(application_candidate_from_manifest(
            "package.json",
            r#"{"private":true,"workspaces":["apps/*"],"scripts":{"dev":"turbo run dev"}}"#,
            Some("pnpm"),
            &mut ports,
        )
        .expect("valid workspace package")
        .is_none());
    }

    #[test]
    fn source_snapshot_requires_every_confirmed_application_root() {
        let stack = json!({
            "source_snapshot": {
                "project_id": "project-1",
                "default_branch": "main",
                "source_commit": "1111111111111111111111111111111111111111",
                "snapshot_id": "snapshot-1",
                "scanned_file_count": 8,
                "requires_application_runtime": true,
                "application_candidates": [
                    {"source_root": "apps/api"},
                    {"source_root": "apps/web"}
                ]
            }
        });
        let images = vec![application_image("apps/api")];
        let error = validate_source_snapshot_coverage(&stack, images.as_slice())
            .expect_err("missing web plan must fail closed");
        assert!(error.contains("apps/web"));
    }

    fn application_image(source_root: &str) -> ProjectRuntimeEnvironmentImageRecord {
        ProjectRuntimeEnvironmentImageRecord {
            id: format!("image-{source_root}"),
            project_id: "project-1".to_string(),
            environment_key: source_root.replace('/', "-"),
            environment_type: "application".to_string(),
            display_name: source_root.to_string(),
            service_id: source_root.replace('/', "-"),
            service_role: RuntimeServiceRole::Application,
            source_root: source_root.to_string(),
            component_kind: "application".to_string(),
            startup_command: Some("pnpm run dev".to_string()),
            test_command: None,
            depends_on: Vec::new(),
            auto_start: true,
            mcp_policy: ProgramManagedMcpPolicy::default(),
            image_id: None,
            image_ref: None,
            image_provider: RuntimeEnvironmentProvider::CloudSandboxManager,
            features: empty_array(),
            ports: empty_array(),
            env_vars: empty_object(),
            dockerfile: Some("FROM node:22".to_string()),
            custom_build_script: None,
            status: "planned".to_string(),
            error: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }
}
