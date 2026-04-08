use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCapabilityDefinition {
    pub token: String,
    pub builtin_mcp_id: String,
    pub display_name: String,
    pub description: String,
    #[serde(default)]
    pub contact_authorizable: bool,
    #[serde(default)]
    pub planning_visible: bool,
    #[serde(default)]
    pub default_when_available: bool,
    #[serde(default)]
    pub runtime_requirements: Vec<String>,
    #[serde(default)]
    pub inference_aliases: Vec<String>,
}

static TASK_CAPABILITY_REGISTRY: Lazy<Vec<TaskCapabilityDefinition>> = Lazy::new(|| {
    serde_json::from_str(include_str!("../../config/task_capability_registry.json"))
        .expect("task capability registry config is invalid")
});

pub fn list_task_capabilities() -> &'static [TaskCapabilityDefinition] {
    TASK_CAPABILITY_REGISTRY.as_slice()
}

pub fn list_planning_task_capabilities() -> Vec<&'static TaskCapabilityDefinition> {
    list_task_capabilities()
        .iter()
        .filter(|item| item.planning_visible)
        .collect()
}

pub fn find_task_capability_by_token(token: &str) -> Option<&'static TaskCapabilityDefinition> {
    let normalized = token.trim();
    if normalized.is_empty() {
        return None;
    }
    list_task_capabilities()
        .iter()
        .find(|item| item.token.trim() == normalized)
}

pub fn find_task_capability_by_mcp_id(
    builtin_mcp_id: &str,
) -> Option<&'static TaskCapabilityDefinition> {
    let normalized = builtin_mcp_id.trim();
    if normalized.is_empty() {
        return None;
    }
    list_task_capabilities()
        .iter()
        .find(|item| item.builtin_mcp_id.trim() == normalized)
}

pub fn planning_task_capability_tokens() -> Vec<String> {
    list_planning_task_capabilities()
        .into_iter()
        .map(|item| item.token.clone())
        .collect()
}

pub fn runtime_requirement_matches(
    requirement: &str,
    project_root: bool,
    remote_connection: bool,
) -> bool {
    match requirement.trim() {
        "project_root" => project_root,
        "remote_connection_id" => remote_connection,
        _ => true,
    }
}

pub fn capability_runtime_requirements_satisfied(
    capability: &TaskCapabilityDefinition,
    has_project_root: bool,
    has_remote_connection: bool,
) -> bool {
    capability.runtime_requirements.iter().all(|item| {
        runtime_requirement_matches(item, has_project_root, has_remote_connection)
    })
}

pub fn infer_default_capability_mcp_ids(
    authorized_builtin_mcp_ids: &[String],
    has_project_root: bool,
    has_remote_connection: bool,
) -> Vec<String> {
    let mut out = Vec::new();
    for capability in list_planning_task_capabilities() {
        if !capability.default_when_available {
            continue;
        }
        if !authorized_builtin_mcp_ids
            .iter()
            .any(|item| item == &capability.builtin_mcp_id)
        {
            continue;
        }
        if !capability_runtime_requirements_satisfied(
            capability,
            has_project_root,
            has_remote_connection,
        ) {
            continue;
        }
        out.push(capability.builtin_mcp_id.clone());
    }
    out
}

pub fn infer_capability_mcp_ids_from_text(
    text: &str,
    authorized_builtin_mcp_ids: &[String],
    has_project_root: bool,
    has_remote_connection: bool,
) -> Vec<String> {
    let normalized = text.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    for capability in list_planning_task_capabilities() {
        if !authorized_builtin_mcp_ids
            .iter()
            .any(|item| item == &capability.builtin_mcp_id)
        {
            continue;
        }
        if !capability_runtime_requirements_satisfied(
            capability,
            has_project_root,
            has_remote_connection,
        ) {
            continue;
        }
        if capability.inference_aliases.iter().any(|alias| {
            let trimmed = alias.trim();
            !trimmed.is_empty() && normalized.contains(trimmed)
        }) {
            out.push(capability.builtin_mcp_id.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::infer_capability_mcp_ids_from_text;

    #[test]
    fn infers_write_capability_from_common_chinese_implementation_phrases() {
        let inferred = infer_capability_mcp_ids_from_text(
            "实现 Admin 报表页面，新增路由并接入接口",
            &vec!["builtin_code_maintainer_write".to_string()],
            true,
            false,
        );

        assert_eq!(inferred, vec!["builtin_code_maintainer_write".to_string()]);
    }
}
