// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::compose::*;
use super::super::*;

const CHINA_MIRROR_MARKER: &str = "# chatos: prefer china mirrors";
const DEFAULT_UBUNTU_MIRROR: &str = "https://mirrors.aliyun.com/ubuntu/";
const DEFAULT_UBUNTU_PORTS_MIRROR: &str = "https://mirrors.aliyun.com/ubuntu-ports/";
const DEFAULT_DEBIAN_MIRROR: &str = "https://mirrors.aliyun.com/debian";
const DEFAULT_DEBIAN_SECURITY_MIRROR: &str = "https://mirrors.aliyun.com/debian-security";

pub(in crate::services::environment_agent::tool_provider) fn normalize_generated_config_files(
    inputs: Vec<ProjectRuntimeEnvironmentConfigFileInput>,
) -> Result<Vec<ProjectRuntimeEnvironmentConfigFileRecord>, String> {
    const MAX_CONFIG_FILE_BYTES: usize = 1024 * 1024;
    let mut by_path = std::collections::BTreeMap::new();
    for input in inputs {
        let path = normalize_generated_config_path(input.path.as_str())?;
        if by_path.contains_key(path.as_str()) {
            return Err(format!("duplicate generated config file path: {path}"));
        }
        if input.content.len() > MAX_CONFIG_FILE_BYTES {
            return Err(format!(
                "generated config file {path} exceeds {MAX_CONFIG_FILE_BYTES} bytes"
            ));
        }
        let format = input
            .format
            .and_then(normalize_owned)
            .unwrap_or_else(|| infer_config_format(path.as_str()).to_string());
        let source_files = input
            .source_files
            .into_iter()
            .filter_map(normalize_owned)
            .collect();
        by_path.insert(
            path.clone(),
            ProjectRuntimeEnvironmentConfigFileRecord {
                path,
                format,
                content: input.content,
                description: input.description.and_then(normalize_owned),
                source_files,
            },
        );
    }
    Ok(by_path.into_values().collect())
}

pub(in crate::services::environment_agent::tool_provider) fn normalize_generated_config_path(
    value: &str,
) -> Result<String, String> {
    let value = value.trim().replace('\\', "/");
    if value.is_empty()
        || value.len() > 512
        || value.starts_with('/')
        || value
            .as_bytes()
            .get(1)
            .is_some_and(|separator| *separator == b':')
    {
        return Err(format!("invalid generated config file path: {value}"));
    }
    let segments = value
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .collect::<Vec<_>>();
    if segments.is_empty() || segments.contains(&"..") {
        return Err(format!("invalid generated config file path: {value}"));
    }
    Ok(segments.join("/"))
}

pub(in crate::services::environment_agent::tool_provider) fn infer_config_format(
    path: &str,
) -> &'static str {
    let file_name = path.rsplit('/').next().unwrap_or(path);
    if file_name == ".env" || file_name.starts_with(".env.") {
        return "dotenv";
    }
    match file_name.rsplit_once('.').map(|(_, extension)| extension) {
        Some("yml" | "yaml") => "yaml",
        Some("json") => "json",
        Some("toml") => "toml",
        Some("properties") => "properties",
        Some("xml") => "xml",
        Some("ini" | "conf") => "ini",
        _ => "text",
    }
}

pub(in crate::services::environment_agent::tool_provider) fn env_value_to_string(
    value: &Value,
) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

pub(in crate::services::environment_agent::tool_provider) fn image_input_to_record(
    project_id: &str,
    image: ProjectRuntimeEnvironmentImageInput,
    index: usize,
    default_provider: RuntimeEnvironmentProvider,
    prefer_china_mirrors: bool,
    image_catalog: Option<&Value>,
) -> Result<ProjectRuntimeEnvironmentImageRecord, String> {
    let now = now_rfc3339();
    let environment_type = image
        .environment_type
        .and_then(normalize_owned)
        .unwrap_or_else(|| "runtime".to_string());
    let environment_key = image
        .environment_key
        .and_then(normalize_owned)
        .unwrap_or_else(|| format!("{}_{}", environment_type, index + 1));
    let display_name = image
        .display_name
        .and_then(normalize_owned)
        .unwrap_or_else(|| environment_key.clone());
    let source_root = normalize_component_root(image.source_root.as_deref().unwrap_or("."))?;
    let component_kind = image
        .component_kind
        .and_then(normalize_owned)
        .unwrap_or_else(|| environment_type.clone());
    let startup_command = image.startup_command.and_then(normalize_owned);
    let test_command = image.test_command.and_then(normalize_owned);
    let mut depends_on = image
        .depends_on
        .into_iter()
        .filter_map(normalize_owned)
        .collect::<Vec<_>>();
    depends_on.sort();
    depends_on.dedup();
    let image_id = None;
    let image_ref = None;
    let dockerfile = image.dockerfile.and_then(normalize_multiline_owned);
    let custom_build_script = None;
    let status = "planned".to_string();
    let ports = image
        .ports
        .map(ensure_array)
        .filter(|ports| ports.as_array().is_some_and(|ports| !ports.is_empty()))
        .unwrap_or_else(|| {
            default_ports_for_environment(environment_key.as_str(), environment_type.as_str())
        });
    let mut record = ProjectRuntimeEnvironmentImageRecord {
        id: format!("project_env_image_{}", Uuid::new_v4()),
        project_id: project_id.to_string(),
        environment_key,
        environment_type,
        display_name,
        service_id: String::new(),
        service_role: RuntimeServiceRole::Unknown,
        source_root,
        component_kind,
        startup_command,
        test_command,
        depends_on,
        auto_start: image.auto_start,
        mcp_policy: ProgramManagedMcpPolicy::default(),
        image_id,
        image_ref,
        image_provider: default_provider,
        features: image.features.map(ensure_array).unwrap_or_else(empty_array),
        ports,
        env_vars: image
            .env_vars
            .map(ensure_object)
            .unwrap_or_else(empty_object),
        dockerfile,
        custom_build_script,
        status,
        error: None,
        created_at: now.clone(),
        updated_at: now,
    };
    if let Some(image_ref) = super::super::super::compose_dependency_image_ref(&record) {
        record.image_id = None;
        record.image_ref = Some(image_ref);
        record.status = "ready".to_string();
        record.error = None;
    } else if image_is_application_runtime(&record) {
        record.image_id = None;
        record.image_ref = None;
        record.status = "planned".to_string();
        record.error = None;
    }
    crate::services::runtime_environment::apply_program_managed_image_policy(&mut record);
    apply_preferred_mirror_policy(&mut record, prefer_china_mirrors);
    let _ = image_catalog;
    Ok(record)
}

pub(in crate::services::environment_agent::tool_provider) fn analysis_prefers_china_mirrors(
    detected_stack: &Value,
) -> bool {
    detected_stack
        .get("prefer_china_mirrors")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn apply_preferred_mirror_policy(
    record: &mut ProjectRuntimeEnvironmentImageRecord,
    prefer_china_mirrors: bool,
) {
    if !prefer_china_mirrors || record.service_role != RuntimeServiceRole::Application {
        return;
    }
    let Some(dockerfile) = record.dockerfile.as_deref() else {
        return;
    };
    let rewritten = inject_china_mirror_bootstrap(dockerfile);
    if rewritten != dockerfile {
        record.dockerfile = Some(rewritten);
    }
}

fn inject_china_mirror_bootstrap(dockerfile: &str) -> String {
    let lower = dockerfile.to_ascii_lowercase();
    if lower.contains(CHINA_MIRROR_MARKER) || !lower.contains("apt-get") {
        return dockerfile.to_string();
    }
    let uses_debian_family = dockerfile.lines().any(|line| {
        let line = line.trim().to_ascii_lowercase();
        line.starts_with("from ")
            && [
                "ubuntu", "debian", "bookworm", "bullseye", "buster", "trixie", "sid", "noble",
                "jammy", "focal",
            ]
            .iter()
            .any(|marker| line.contains(marker))
    });
    if !uses_debian_family {
        return dockerfile.to_string();
    }
    let ubuntu_mirror = normalized_env_value(
        "SANDBOX_MANAGER_IMAGE_APT_UBUNTU_MIRROR",
        DEFAULT_UBUNTU_MIRROR,
    );
    let ubuntu_ports_mirror = normalized_env_value(
        "SANDBOX_MANAGER_IMAGE_APT_UBUNTU_PORTS_MIRROR",
        DEFAULT_UBUNTU_PORTS_MIRROR,
    );
    let debian_mirror = normalized_env_value(
        "SANDBOX_MANAGER_IMAGE_APT_DEBIAN_MIRROR",
        DEFAULT_DEBIAN_MIRROR,
    );
    let debian_security_mirror = normalized_env_value(
        "SANDBOX_MANAGER_IMAGE_APT_DEBIAN_SECURITY_MIRROR",
        DEFAULT_DEBIAN_SECURITY_MIRROR,
    );
    let bootstrap = format!(
        "{CHINA_MIRROR_MARKER}\nRUN set -eux; \\\n    if [ -f /etc/apt/sources.list.d/ubuntu.sources ]; then \\\n      sed -i \\\n        -e 's|http://ports.ubuntu.com/ubuntu-ports/|{ubuntu_ports_mirror}|g' \\\n        -e 's|https://ports.ubuntu.com/ubuntu-ports/|{ubuntu_ports_mirror}|g' \\\n        -e 's|http://archive.ubuntu.com/ubuntu/|{ubuntu_mirror}|g' \\\n        -e 's|https://archive.ubuntu.com/ubuntu/|{ubuntu_mirror}|g' \\\n        -e 's|http://security.ubuntu.com/ubuntu/|{ubuntu_mirror}|g' \\\n        -e 's|https://security.ubuntu.com/ubuntu/|{ubuntu_mirror}|g' \\\n        /etc/apt/sources.list.d/ubuntu.sources; \\\n    fi; \\\n    if [ -f /etc/apt/sources.list ]; then \\\n      sed -i \\\n        -e 's|http://ports.ubuntu.com/ubuntu-ports/|{ubuntu_ports_mirror}|g' \\\n        -e 's|https://ports.ubuntu.com/ubuntu-ports/|{ubuntu_ports_mirror}|g' \\\n        -e 's|http://archive.ubuntu.com/ubuntu/|{ubuntu_mirror}|g' \\\n        -e 's|https://archive.ubuntu.com/ubuntu/|{ubuntu_mirror}|g' \\\n        -e 's|http://security.ubuntu.com/ubuntu/|{ubuntu_mirror}|g' \\\n        -e 's|https://security.ubuntu.com/ubuntu/|{ubuntu_mirror}|g' \\\n        /etc/apt/sources.list; \\\n    fi; \\\n    find /etc/apt -type f \\( -name '*.list' -o -name '*.sources' \\) -print0 \\\n      | xargs -0 -r sed -i \\\n        -e 's|http://deb.debian.org/debian|{debian_mirror}|g' \\\n        -e 's|https://deb.debian.org/debian|{debian_mirror}|g' \\\n        -e 's|http://security.debian.org/debian-security|{debian_security_mirror}|g' \\\n        -e 's|https://security.debian.org/debian-security|{debian_security_mirror}|g'\n"
    );
    let mut output = Vec::new();
    for line in dockerfile.lines() {
        output.push(line.to_string());
        if line.trim_start().to_ascii_uppercase().starts_with("FROM ") {
            output.push(bootstrap.clone());
        }
    }
    let rewritten = output.join("\n");
    if dockerfile.ends_with('\n') {
        format!("{rewritten}\n")
    } else {
        rewritten
    }
}

fn normalized_env_value(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .map(|value| format!("{value}/"))
        .unwrap_or_else(|| default.to_string())
}

fn normalize_component_root(value: &str) -> Result<String, String> {
    let value = value.trim().replace('\\', "/");
    if value.is_empty() || value == "." {
        return Ok(".".to_string());
    }
    if value.starts_with('/')
        || value
            .as_bytes()
            .get(1)
            .is_some_and(|separator| *separator == b':')
    {
        return Err(format!("invalid component source_root: {value}"));
    }
    let segments = value
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .collect::<Vec<_>>();
    if segments.is_empty() || segments.contains(&"..") {
        return Err(format!("invalid component source_root: {value}"));
    }
    Ok(segments.join("/"))
}

pub(in crate::services::environment_agent::tool_provider) fn ensure_array(value: Value) -> Value {
    if value.is_array() {
        value
    } else {
        empty_array()
    }
}

pub(in crate::services::environment_agent::tool_provider) fn ensure_object(value: Value) -> Value {
    if value.is_object() {
        value
    } else {
        empty_object()
    }
}

pub(in crate::services::environment_agent::tool_provider) fn mcp_tool_result(
    message: impl Into<String>,
    structured: Value,
) -> Value {
    let message = message.into();
    let text = serde_json::to_string_pretty(&structured).unwrap_or_else(|_| message.clone());
    json!({
        "content": [{
            "type": "text",
            "text": format!("{message}\n{text}")
        }],
        "_structured_result": structured
    })
}
