// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::*;
use crate::store::AppStore;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

mod boundary;

pub use boundary::enforce_project_runtime_boundary;

pub const WORKSPACE_EXECUTION_SERVICE_ID: &str = "workspace";

pub async fn ensure_runtime_environment_for_project(
    store: &AppStore,
    project: &ProjectRecord,
    sandbox_enabled: Option<bool>,
) -> Result<ProjectRuntimeEnvironmentRecord, String> {
    if let Some(mut existing) = store
        .get_project_runtime_environment(project.id.as_str())
        .await?
    {
        if let Some(sandbox_enabled) = sandbox_enabled {
            existing.sandbox_enabled = sandbox_enabled;
            existing.status = if sandbox_enabled {
                if existing.status == ProjectRuntimeEnvironmentStatus::Disabled {
                    ProjectRuntimeEnvironmentStatus::Pending
                } else {
                    existing.status
                }
            } else {
                ProjectRuntimeEnvironmentStatus::Disabled
            };
            if !sandbox_enabled {
                existing.sandbox_provider = RuntimeEnvironmentProvider::None;
                existing.file_provider = project_file_provider(project);
                existing.last_error = None;
            }
            existing.updated_at = now_rfc3339();
            let saved = store.upsert_project_runtime_environment(&existing).await?;
            if !sandbox_enabled {
                store
                    .replace_project_runtime_environment_images(project.id.as_str(), &[])
                    .await?;
            }
            return Ok(saved);
        }
        return Ok(existing);
    }
    let environment = default_runtime_environment_for_project(project, sandbox_enabled);
    store.upsert_project_runtime_environment(&environment).await
}

pub fn default_runtime_environment_for_project(
    project: &ProjectRecord,
    sandbox_enabled: Option<bool>,
) -> ProjectRuntimeEnvironmentRecord {
    let sandbox_enabled = sandbox_enabled.unwrap_or(true);
    let now = now_rfc3339();
    ProjectRuntimeEnvironmentRecord {
        project_id: project.id.clone(),
        status: if sandbox_enabled {
            ProjectRuntimeEnvironmentStatus::Pending
        } else {
            ProjectRuntimeEnvironmentStatus::Disabled
        },
        sandbox_enabled,
        sandbox_provider: RuntimeEnvironmentProvider::None,
        file_provider: project_file_provider(project),
        analysis_summary: None,
        not_runnable_reason: None,
        execution_service_id: None,
        detected_stack: empty_object(),
        required_services: empty_array(),
        env_vars: empty_object(),
        environment_variables: Vec::new(),
        generated_config_files: Vec::new(),
        last_agent_run_id: None,
        last_error: None,
        created_at: now.clone(),
        updated_at: now,
    }
}

fn project_file_provider(project: &ProjectRecord) -> RuntimeEnvironmentProvider {
    if project
        .harness_repo_identifier
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
        || project
            .harness_git_url
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty())
    {
        return RuntimeEnvironmentProvider::Harness;
    }
    match project.source_type {
        ProjectSourceType::Cloud
            if project
                .harness_repo_identifier
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty()) =>
        {
            RuntimeEnvironmentProvider::Harness
        }
        ProjectSourceType::Local | ProjectSourceType::LocalConnector
            if chatos_project_execution::parse_local_connector_workspace_root(
                project.root_path.as_deref().unwrap_or_default(),
            )
            .is_some() =>
        {
            RuntimeEnvironmentProvider::LocalConnector
        }
        _ => RuntimeEnvironmentProvider::None,
    }
}

pub fn refresh_environment_variable_values(environment: &mut ProjectRuntimeEnvironmentRecord) {
    environment.environment_variables = normalize_environment_variable_records(
        std::mem::take(&mut environment.environment_variables),
        &environment.env_vars,
    );
    environment.env_vars = effective_environment_variables(&environment.environment_variables);
}

pub fn program_generated_runtime_analysis_summary(
    environment: &ProjectRuntimeEnvironmentRecord,
    images: &[ProjectRuntimeEnvironmentImageRecord],
) -> String {
    match environment.status {
        ProjectRuntimeEnvironmentStatus::Analyzing => {
            return "正在分析项目技术栈和运行环境需求。".to_string();
        }
        ProjectRuntimeEnvironmentStatus::NotRunnable => {
            return "未识别到可自动初始化的应用或基础设施入口。".to_string();
        }
        ProjectRuntimeEnvironmentStatus::Failed => {
            return "项目技术分析未能完成。".to_string();
        }
        _ => {}
    }

    let application_count = images
        .iter()
        .filter(|image| image.service_role == RuntimeServiceRole::Application)
        .count();
    let artifact_count = images
        .iter()
        .filter(|image| image.service_role == RuntimeServiceRole::Artifact)
        .count();
    let dependency_count = images
        .iter()
        .filter(|image| image.service_role == RuntimeServiceRole::Dependency)
        .count();
    let config_file_count = environment.generated_config_files.len();
    let missing_variables = environment
        .environment_variables
        .iter()
        .filter(|record| {
            record.required
                && record
                    .effective_value
                    .as_deref()
                    .is_none_or(|value| value.trim().is_empty())
        })
        .map(|record| record.name.as_str())
        .collect::<Vec<_>>();
    let base = if runtime_environment_requires_managed_images(environment) {
        format!(
            "已识别 {application_count} 个平等应用组件、{dependency_count} 个依赖服务和 {artifact_count} 个非运行组件，生成唯一工作区执行镜像计划及 {config_file_count} 个环境配置文件"
        )
    } else {
        format!(
            "已识别 {application_count} 个应用组件、{dependency_count} 个依赖服务和 {artifact_count} 个非运行组件，记录本地启动条件及 {config_file_count} 个环境配置文件"
        )
    };
    match environment.status {
        ProjectRuntimeEnvironmentStatus::PendingImageBuild => {
            format!("{base}，等待生成工作区执行镜像。")
        }
        ProjectRuntimeEnvironmentStatus::PendingConfiguration if missing_variables.is_empty() => {
            format!("{base}，仍需补充必填运行参数。")
        }
        ProjectRuntimeEnvironmentStatus::PendingConfiguration => format!(
            "{base}，仍需补充 {} 个必填运行参数：{}。",
            missing_variables.len(),
            missing_variables.join(", ")
        ),
        ProjectRuntimeEnvironmentStatus::Ready
            if runtime_environment_requires_managed_images(environment) =>
        {
            format!("{base}，运行环境已就绪。")
        }
        ProjectRuntimeEnvironmentStatus::Ready => {
            format!("{base}，本地隔离与执行由 Local Connector 客户端负责。")
        }
        _ => format!("{base}。"),
    }
}

pub fn runtime_environment_requires_managed_images(
    environment: &ProjectRuntimeEnvironmentRecord,
) -> bool {
    environment.sandbox_enabled
        && environment.sandbox_provider == RuntimeEnvironmentProvider::CloudSandboxManager
}

pub fn replace_legacy_internal_routing_summary(
    environment: &mut ProjectRuntimeEnvironmentRecord,
    images: &[ProjectRuntimeEnvironmentImageRecord],
) -> bool {
    let is_legacy_internal_summary = environment.analysis_summary.as_deref().is_some_and(|summary| {
        matches!(
            summary,
            "云端项目只通过 Harness MCP 读取文件，并只使用云端 Sandbox Manager。"
                | "本地项目将通过 Local Connector 文件 MCP 读取文件，并按本地沙箱可用性选择沙箱镜像 MCP。"
        ) || summary.contains("等待生成应用镜像")
            || summary.contains("等待生成主执行镜像")
            || summary.contains("主应用")
            || summary.contains("伴随微服务")
    });
    if !is_legacy_internal_summary {
        return false;
    }
    environment.analysis_summary = Some(program_generated_runtime_analysis_summary(
        environment,
        images,
    ));
    environment.updated_at = now_rfc3339();
    true
}

pub fn apply_program_managed_image_policy(
    image: &mut ProjectRuntimeEnvironmentImageRecord,
) -> bool {
    let service_role = program_managed_service_role(image);
    let service_id = program_managed_service_id_for_role(image, service_role);
    let mcp_policy = match service_role {
        RuntimeServiceRole::Workspace => ProgramManagedMcpPolicy::workspace_target(),
        RuntimeServiceRole::Application
        | RuntimeServiceRole::Dependency
        | RuntimeServiceRole::Artifact
        | RuntimeServiceRole::Unknown => ProgramManagedMcpPolicy::default(),
    };
    let mut changed = image.service_id != service_id
        || image.service_role != service_role
        || image.mcp_policy != mcp_policy;
    image.service_id = service_id;
    image.service_role = service_role;
    image.mcp_policy = mcp_policy;
    if matches!(
        service_role,
        RuntimeServiceRole::Application | RuntimeServiceRole::Artifact
    ) && (image.image_id.is_some() || image.image_ref.is_some())
    {
        image.image_id = None;
        image.image_ref = None;
        image.error = None;
        image.status = if service_role == RuntimeServiceRole::Artifact {
            "excluded".to_string()
        } else {
            "planned".to_string()
        };
        changed = true;
    }
    if service_role == RuntimeServiceRole::Artifact && image.status != "excluded" {
        image.status = "excluded".to_string();
        image.error = None;
        changed = true;
    }
    changed
}

fn ensure_workspace_execution_record(
    environment: &ProjectRuntimeEnvironmentRecord,
    images: &mut Vec<ProjectRuntimeEnvironmentImageRecord>,
    legacy_target: Option<&ProjectRuntimeEnvironmentImageRecord>,
) -> bool {
    let features = workspace_runtime_features(images.as_slice(), &environment.detected_stack);
    if let Some(workspace) = images
        .iter_mut()
        .find(|image| program_managed_service_role(image) == RuntimeServiceRole::Workspace)
    {
        let desired_features = Value::Array(features.into_iter().map(Value::String).collect());
        let changed = workspace.environment_key != WORKSPACE_EXECUTION_SERVICE_ID
            || workspace.environment_type != "workspace"
            || workspace.display_name != "Project Workspace"
            || workspace.source_root != "."
            || workspace.component_kind != "workspace"
            || workspace.features != desired_features;
        workspace.environment_key = WORKSPACE_EXECUTION_SERVICE_ID.to_string();
        workspace.environment_type = "workspace".to_string();
        workspace.display_name = "Project Workspace".to_string();
        workspace.source_root = ".".to_string();
        workspace.component_kind = "workspace".to_string();
        workspace.features = desired_features;
        return changed;
    }

    let now = now_rfc3339();
    let reusable_legacy = legacy_target.filter(|legacy| {
        let legacy_features = legacy
            .features
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect::<BTreeSet<_>>();
        !features.is_empty()
            && features
                .iter()
                .all(|feature| legacy_features.contains(feature.as_str()))
    });
    images.push(ProjectRuntimeEnvironmentImageRecord {
        id: format!("project_env_image_{}", uuid::Uuid::new_v4()),
        project_id: environment.project_id.clone(),
        environment_key: WORKSPACE_EXECUTION_SERVICE_ID.to_string(),
        environment_type: "workspace".to_string(),
        display_name: "Project Workspace".to_string(),
        service_id: WORKSPACE_EXECUTION_SERVICE_ID.to_string(),
        service_role: RuntimeServiceRole::Workspace,
        source_root: ".".to_string(),
        component_kind: "workspace".to_string(),
        startup_command: None,
        test_command: None,
        depends_on: Vec::new(),
        auto_start: true,
        mcp_policy: ProgramManagedMcpPolicy::workspace_target(),
        image_id: reusable_legacy.and_then(|image| image.image_id.clone()),
        image_ref: reusable_legacy.and_then(|image| image.image_ref.clone()),
        image_provider: environment.sandbox_provider,
        features: Value::Array(features.into_iter().map(Value::String).collect()),
        ports: empty_array(),
        env_vars: empty_object(),
        dockerfile: None,
        custom_build_script: None,
        status: if reusable_legacy.is_some() {
            "ready".to_string()
        } else {
            "planned".to_string()
        },
        error: None,
        created_at: now.clone(),
        updated_at: now,
    });
    true
}

pub fn workspace_runtime_features(
    images: &[ProjectRuntimeEnvironmentImageRecord],
    detected_stack: &Value,
) -> Vec<String> {
    const EMPTY_PROJECT_BOOTSTRAP_FEATURES: [&str; 2] = ["node@24", "python@3.11"];
    const ORDERED_RUNTIMES: [&str; 10] = [
        "java", "node", "python", "rust", "go", "dotnet", "php", "ruby", "gcc", "clang",
    ];
    let mut selected = BTreeMap::<&'static str, String>::new();
    let mut evidence = detected_stack_runtime_evidence(detected_stack);
    for image in images
        .iter()
        .filter(|image| image.service_role == RuntimeServiceRole::Application)
    {
        evidence.push(' ');
        evidence.push_str(image.dockerfile.as_deref().unwrap_or_default());
        evidence.push(' ');
        for raw in image
            .features
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            evidence.push_str(raw);
            evidence.push(' ');
            if let Some((runtime, feature)) = canonical_workspace_runtime(raw) {
                let entry = selected.entry(runtime).or_insert_with(|| feature.clone());
                if !entry.contains('@') && feature.contains('@') {
                    *entry = feature;
                }
            }
        }
    }
    let evidence = evidence.to_ascii_lowercase();
    for (feature, markers) in [
        (
            "java",
            &["java", "maven", "gradle", "spring", "openjdk"][..],
        ),
        (
            "node",
            &["node", "npm", "pnpm", "yarn", "typescript", "package.json"][..],
        ),
        (
            "python",
            &["python", "pip", "pyproject.toml", "requirements.txt"][..],
        ),
        ("rust", &["rust", "cargo.toml", "cargo build"][..]),
        ("go", &["golang", "go.mod"][..]),
        ("dotnet", &["dotnet", ".csproj", "msbuild"][..]),
        ("php", &["php", "composer.json"][..]),
        ("ruby", &["ruby", "gemfile", "bundle install"][..]),
        ("gcc", &["gcc", "g++", "cmakelists.txt"][..]),
        ("clang", &["clang", "llvm"][..]),
    ] {
        let has_runtime_evidence = markers.iter().any(|marker| evidence.contains(marker))
            || (feature == "go" && contains_standalone_marker(&evidence, "go build"));
        if has_runtime_evidence {
            selected
                .entry(feature)
                .or_insert_with(|| feature.to_string());
        }
    }
    let selected = ORDERED_RUNTIMES
        .into_iter()
        .filter_map(|runtime| selected.get(runtime).cloned())
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return EMPTY_PROJECT_BOOTSTRAP_FEATURES
            .into_iter()
            .map(ToOwned::to_owned)
            .collect();
    }
    selected
}

fn contains_standalone_marker(value: &str, marker: &str) -> bool {
    value.match_indices(marker).any(|(start, matched)| {
        let before_is_boundary = value[..start]
            .chars()
            .next_back()
            .is_none_or(|character| !character.is_ascii_alphanumeric() && character != '_');
        let end = start + matched.len();
        let after_is_boundary = value[end..]
            .chars()
            .next()
            .is_none_or(|character| !character.is_ascii_alphanumeric() && character != '_');
        before_is_boundary && after_is_boundary
    })
}

fn detected_stack_runtime_evidence(detected_stack: &Value) -> String {
    const RUNTIME_EVIDENCE_KEYS: [&str; 12] = [
        "analysis_requirement",
        "application_entrypoints",
        "build_manifests",
        "build_tools",
        "frameworks",
        "languages",
        "package_managers",
        "project_type",
        "runtime",
        "runtimes",
        "stack",
        "technology_stack",
    ];
    let Some(object) = detected_stack.as_object() else {
        return String::new();
    };
    RUNTIME_EVIDENCE_KEYS
        .into_iter()
        .filter_map(|key| object.get(key))
        .filter_map(|value| serde_json::to_string(value).ok())
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn canonical_workspace_runtime(value: &str) -> Option<(&'static str, String)> {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty() {
        return None;
    }
    if let Some((name, version)) = value.split_once('@').or_else(|| value.split_once(':')) {
        let runtime = workspace_runtime_name(name.trim())?;
        let version = version.trim().trim_start_matches('v');
        return Some((
            runtime,
            if version.is_empty() {
                runtime.to_string()
            } else {
                format!("{runtime}@{version}")
            },
        ));
    }
    if let Some(runtime) = workspace_runtime_name(value.as_str()) {
        return Some((runtime, runtime.to_string()));
    }
    for name in [
        "javascript",
        "typescript",
        "openjdk",
        "nodejs",
        "python",
        "dotnet",
        "golang",
        "clang",
        "java",
        "rust",
        "ruby",
        "node",
        "gcc",
        "jdk",
        "php",
        "go",
    ] {
        let Some(version) = value.strip_prefix(name) else {
            continue;
        };
        let version = version
            .trim_matches(['-', '_', '@', ':'])
            .trim_start_matches('v');
        if version.is_empty()
            || !(version.chars().any(|character| character.is_ascii_digit())
                || matches!(version, "stable" | "beta" | "nightly"))
        {
            continue;
        }
        let runtime = workspace_runtime_name(name)?;
        return Some((runtime, format!("{runtime}@{version}")));
    }
    None
}

fn workspace_runtime_name(value: &str) -> Option<&'static str> {
    match value {
        "java" | "jdk" | "openjdk" | "maven" | "mvn" | "gradle" | "spring" | "springboot"
        | "spring-boot" => Some("java"),
        "node" | "nodejs" | "js" | "javascript" | "typescript" | "npm" | "pnpm" | "yarn"
        | "bun" => Some("node"),
        "python" | "python3" | "py" | "pip" | "pip3" | "poetry" | "uv" => Some("python"),
        "rust" | "cargo" => Some("rust"),
        "go" | "golang" | "gomod" => Some("go"),
        "dotnet" | "csharp" | "cs" | "fsharp" | "msbuild" => Some("dotnet"),
        "php" | "composer" => Some("php"),
        "ruby" | "rails" | "gem" | "bundler" => Some("ruby"),
        "gcc" | "c" | "cpp" | "c++" | "cplusplus" | "g++" => Some("gcc"),
        "clang" | "llvm" => Some("clang"),
        _ => None,
    }
}

pub fn program_managed_service_id(image: &ProjectRuntimeEnvironmentImageRecord) -> String {
    program_managed_service_id_for_role(image, program_managed_service_role(image))
}

pub fn runtime_image_is_execution_required(image: &ProjectRuntimeEnvironmentImageRecord) -> bool {
    matches!(
        image.service_role,
        RuntimeServiceRole::Workspace | RuntimeServiceRole::Dependency
    )
}

fn program_managed_service_id_for_role(
    image: &ProjectRuntimeEnvironmentImageRecord,
    service_role: RuntimeServiceRole,
) -> String {
    const MAX_SERVICE_ID_LENGTH: usize = 63;

    let mut normalized = String::new();
    let mut previous_separator = false;
    for character in image.environment_key.trim().chars() {
        if character.is_ascii_alphanumeric() {
            normalized.push(character.to_ascii_lowercase());
            previous_separator = false;
        } else if !previous_separator && !normalized.is_empty() {
            normalized.push('-');
            previous_separator = true;
        }
    }
    while normalized.ends_with('-') {
        normalized.pop();
    }
    if normalized.is_empty() {
        normalized = match service_role {
            RuntimeServiceRole::Workspace => WORKSPACE_EXECUTION_SERVICE_ID,
            RuntimeServiceRole::Application => "application",
            RuntimeServiceRole::Dependency => "dependency",
            RuntimeServiceRole::Artifact => "artifact",
            RuntimeServiceRole::Unknown => "service",
        }
        .to_string();
    } else if normalized
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_digit())
    {
        let prefix = match service_role {
            RuntimeServiceRole::Workspace => "workspace",
            RuntimeServiceRole::Application => "app",
            RuntimeServiceRole::Dependency => "dependency",
            RuntimeServiceRole::Artifact => "artifact",
            RuntimeServiceRole::Unknown => "service",
        };
        normalized = format!("{prefix}-{normalized}");
    }
    normalized.truncate(MAX_SERVICE_ID_LENGTH);
    while normalized.ends_with('-') {
        normalized.pop();
    }
    normalized
}

fn program_managed_service_role(
    image: &ProjectRuntimeEnvironmentImageRecord,
) -> RuntimeServiceRole {
    if image.environment_type.eq_ignore_ascii_case("workspace")
        || image.component_kind.eq_ignore_ascii_case("workspace")
        || image.environment_key == WORKSPACE_EXECUTION_SERVICE_ID
            && image.service_role == RuntimeServiceRole::Workspace
    {
        return RuntimeServiceRole::Workspace;
    }
    if runtime_image_is_known_dependency(image) {
        return RuntimeServiceRole::Dependency;
    }
    if runtime_image_declares_artifact(image) {
        return RuntimeServiceRole::Artifact;
    }
    if runtime_image_declares_application(image)
        && image
            .dockerfile
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
    {
        return RuntimeServiceRole::Application;
    }
    RuntimeServiceRole::Unknown
}

fn runtime_image_declares_artifact(image: &ProjectRuntimeEnvironmentImageRecord) -> bool {
    if image.environment_type.eq_ignore_ascii_case("artifact")
        || image.component_kind.eq_ignore_ascii_case("artifact")
    {
        return true;
    }
    let identity = format!(
        "{} {} {}",
        image.environment_key, image.display_name, image.component_kind
    )
    .to_ascii_lowercase();
    let artifact_named = [
        "prototype",
        "storybook",
        "docs",
        "documentation",
        "example",
        "fixture",
    ]
    .iter()
    .any(|marker| identity.contains(marker));
    let static_nginx_plan = image.dockerfile.as_deref().is_some_and(|dockerfile| {
        let dockerfile = dockerfile.to_ascii_lowercase();
        dockerfile.contains("from nginx") && dockerfile.contains("/usr/share/nginx/html")
    });
    artifact_named && static_nginx_plan
}

fn runtime_image_declares_application(image: &ProjectRuntimeEnvironmentImageRecord) -> bool {
    let identity =
        format!("{} {}", image.environment_key, image.environment_type).to_ascii_lowercase();
    identity.contains("application")
        || identity.contains("runtime")
        || matches!(image.environment_key.as_str(), "app" | "application")
}

fn runtime_image_is_known_dependency(image: &ProjectRuntimeEnvironmentImageRecord) -> bool {
    let identity = format!(
        "{} {} {} {}",
        image.environment_key,
        image.environment_type,
        image.display_name,
        image.image_ref.as_deref().unwrap_or_default(),
    )
    .to_ascii_lowercase();
    [
        "mysql",
        "mariadb",
        "mongodb",
        "mongo:",
        "postgres",
        "redis",
        "nacos",
        "rabbitmq",
        "kafka",
        "elasticsearch",
        "opensearch",
        "minio",
    ]
    .iter()
    .any(|marker| identity.contains(marker))
}

pub fn normalize_environment_variable_records(
    records: Vec<ProjectRuntimeEnvironmentVariableRecord>,
    legacy_env_vars: &Value,
) -> Vec<ProjectRuntimeEnvironmentVariableRecord> {
    let mut by_name = BTreeMap::<String, ProjectRuntimeEnvironmentVariableRecord>::new();
    for mut record in records {
        let Some(name) = normalize_environment_variable_name(record.name.as_str()) else {
            continue;
        };
        record.name = name.clone();
        record.description = normalize_optional_text(record.description);
        record.recommendation_reason = normalize_optional_text(record.recommendation_reason);
        record.project_value = normalize_optional_value(record.project_value);
        record.recommended_value = normalize_optional_value(record.recommended_value);
        record.user_value = record.user_value.map(|value| value.trim().to_string());
        refresh_environment_variable_record(&mut record);
        by_name.insert(name, record);
    }
    if let Some(legacy) = legacy_env_vars.as_object() {
        for (name, value) in legacy {
            let Some(name) = normalize_environment_variable_name(name) else {
                continue;
            };
            let value = scalar_to_string(value);
            by_name.entry(name.clone()).or_insert_with(|| {
                let mut record = ProjectRuntimeEnvironmentVariableRecord {
                    name,
                    project_value: None,
                    project_value_suitable: false,
                    recommended_value: value,
                    user_value: None,
                    effective_value: None,
                    effective_source: RuntimeEnvironmentVariableSource::None,
                    description: Some("由历史运行环境配置迁移".to_string()),
                    recommendation_reason: Some(
                        "历史记录未保存来源，作为 AI 推荐值保留".to_string(),
                    ),
                    required: false,
                    secret: false,
                };
                record.secret = environment_variable_name_is_secret(record.name.as_str());
                refresh_environment_variable_record(&mut record);
                record
            });
        }
    }
    by_name
        .into_values()
        .filter(|record| {
            record.project_value.is_some()
                || record.recommended_value.is_some()
                || record.user_value.is_some()
        })
        .collect()
}

pub fn effective_environment_variables(
    records: &[ProjectRuntimeEnvironmentVariableRecord],
) -> Value {
    let mut values = Map::new();
    for record in records {
        if let Some(value) = record.effective_value.as_deref() {
            values.insert(record.name.clone(), Value::String(value.to_string()));
        }
    }
    Value::Object(values)
}

pub fn apply_environment_variable_overrides(
    environment: &mut ProjectRuntimeEnvironmentRecord,
    overrides: Vec<ProjectRuntimeEnvironmentVariableOverride>,
) -> Result<(), String> {
    let mut records = normalize_environment_variable_records(
        std::mem::take(&mut environment.environment_variables),
        &environment.env_vars,
    );
    for record in &mut records {
        record.user_value = None;
    }
    let mut seen = BTreeSet::new();
    for item in overrides {
        let name = normalize_environment_variable_name(item.name.as_str())
            .ok_or_else(|| format!("invalid environment variable name: {}", item.name))?;
        if !seen.insert(name.clone()) {
            return Err(format!("duplicate environment variable name: {name}"));
        }
        let value = item.value.trim().to_string();
        if let Some(record) = records.iter_mut().find(|record| record.name == name) {
            record.user_value = Some(value);
        } else {
            records.push(ProjectRuntimeEnvironmentVariableRecord {
                name: name.clone(),
                project_value: None,
                project_value_suitable: false,
                recommended_value: None,
                user_value: Some(value),
                effective_value: None,
                effective_source: RuntimeEnvironmentVariableSource::None,
                description: Some("用户自定义环境变量".to_string()),
                recommendation_reason: None,
                required: false,
                secret: environment_variable_name_is_secret(name.as_str()),
            });
        }
    }
    for record in &mut records {
        refresh_environment_variable_record(record);
    }
    environment.environment_variables = records
        .into_iter()
        .filter(|record| {
            record.project_value.is_some()
                || record.recommended_value.is_some()
                || record.user_value.is_some()
        })
        .collect();
    environment.env_vars = effective_environment_variables(&environment.environment_variables);
    Ok(())
}

pub fn refresh_environment_variable_record(record: &mut ProjectRuntimeEnvironmentVariableRecord) {
    let (value, source) = if let Some(value) = record.user_value.clone() {
        (Some(value), RuntimeEnvironmentVariableSource::User)
    } else if record.project_value_suitable && record.project_value.is_some() {
        (
            record.project_value.clone(),
            RuntimeEnvironmentVariableSource::Project,
        )
    } else if record.recommended_value.is_some() {
        (
            record.recommended_value.clone(),
            RuntimeEnvironmentVariableSource::AiRecommended,
        )
    } else {
        (None, RuntimeEnvironmentVariableSource::None)
    };
    record.effective_value = value;
    record.effective_source = source;
    record.secret = record.secret || environment_variable_name_is_secret(record.name.as_str());
}

pub fn required_environment_variables_are_complete(
    records: &[ProjectRuntimeEnvironmentVariableRecord],
) -> bool {
    records.iter().all(|record| {
        !record.required
            || record
                .effective_value
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
    })
}

pub fn normalize_environment_variable_name(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 256
        || !value.chars().enumerate().all(|(index, character)| {
            character == '_'
                || character.is_ascii_alphanumeric() && (index > 0 || !character.is_ascii_digit())
        })
    {
        return None;
    }
    Some(value.to_ascii_uppercase())
}

pub fn environment_variable_name_is_secret(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    [
        "password",
        "passwd",
        "secret",
        "token",
        "credential",
        "private",
        "access_key",
        "api_key",
        "apikey",
    ]
    .iter()
    .any(|marker| name.contains(marker))
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_string();
        (!value.is_empty()).then_some(value)
    })
}

fn normalize_optional_value(value: Option<String>) -> Option<String> {
    value.map(|value| value.trim().to_string())
}

fn scalar_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

#[cfg(test)]
include!("runtime_environment.test.rs");
