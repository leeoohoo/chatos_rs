// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub mod args;
pub mod mcp;
pub mod schemas;
pub mod tools;

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::{json, Value};

    use super::{schemas, tools};

    #[test]
    fn task_runner_tools_are_subset_of_server_tools() {
        let server = tools::PROJECT_MANAGEMENT_SERVER_TOOL_NAMES
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        for name in tools::TASK_RUNNER_BUILTIN_TOOL_NAMES {
            assert!(server.contains(name));
        }
    }

    #[test]
    fn tool_names_are_unique() {
        let names = tools::PROJECT_MANAGEMENT_SERVER_TOOL_NAMES;
        let unique = names.iter().copied().collect::<BTreeSet<_>>();
        assert_eq!(names.len(), unique.len());
    }

    #[test]
    fn server_schema_names_match_contract_order() {
        let names = schema_tool_names(schemas::project_management_server_tool_definitions());
        assert_eq!(names, tools::PROJECT_MANAGEMENT_SERVER_TOOL_NAMES);
    }

    #[test]
    fn task_runner_schema_names_match_contract_order() {
        let names = schema_tool_names(schemas::task_runner_builtin_tool_definitions());
        assert_eq!(names, tools::TASK_RUNNER_BUILTIN_TOOL_NAMES);
    }

    #[test]
    fn status_schema_values_do_not_advertise_archived() {
        assert!(!schemas::requirement_status_values().contains(&"archived"));
        assert!(!schemas::project_task_status_values().contains(&"archived"));
    }

    #[test]
    fn create_project_task_schema_excludes_task_runner_execution_options() {
        let definitions = schemas::task_runner_builtin_tool_definitions();
        let create_task = definitions
            .iter()
            .find(|tool| {
                tool.get("name").and_then(Value::as_str) == Some(tools::CREATE_PROJECT_TASK)
            })
            .expect("create_project_task");
        let properties = create_task
            .pointer("/inputSchema/properties")
            .and_then(Value::as_object)
            .expect("properties");
        assert!(!properties.contains_key("task_runner_default_model_config_id"));
        assert!(!properties.contains_key("task_runner_enabled_tool_ids"));
        assert!(!properties.contains_key("task_runner_skill_ids"));
        assert!(properties.contains_key("is_planning_task"));
        let list_tasks = definitions
            .iter()
            .find(|tool| {
                tool.get("name").and_then(Value::as_str) == Some(tools::LIST_PROJECT_TASKS)
            })
            .expect("list_project_tasks");
        assert!(list_tasks
            .pointer("/inputSchema/properties/is_planning_task")
            .is_some());
        let update_task = definitions
            .iter()
            .find(|tool| {
                tool.get("name").and_then(Value::as_str) == Some(tools::UPDATE_PROJECT_TASK)
            })
            .expect("update_project_task");
        assert!(update_task
            .pointer("/inputSchema/properties/patch/properties/is_planning_task")
            .is_some());
    }

    #[test]
    fn tool_call_params_default_arguments_to_null() {
        let params = serde_json::from_value::<super::args::ToolCallParams>(json!({
            "name": tools::GET_PROJECT_OVERVIEW
        }))
        .expect("tool call params");
        assert_eq!(params.name, tools::GET_PROJECT_OVERVIEW);
        assert!(params.arguments.is_null());
    }

    #[test]
    fn wire_args_decode_statuses_and_patches() {
        let create = serde_json::from_value::<super::args::CreateRequirementArgs>(json!({
            "title": "Add import",
            "requirement_type": "change",
            "status": "approved"
        }))
        .expect("create requirement args");
        assert_eq!(
            create.requirement_type,
            Some(super::args::RequirementType::Change)
        );
        assert_eq!(
            create.status,
            Some(super::args::RequirementStatus::Approved)
        );

        let failed_update = serde_json::from_value::<super::args::UpdateRequirementArgs>(json!({
            "requirement_id": "req-1",
            "patch": {
                "status": "failed"
            }
        }))
        .expect("failed requirement update args");
        assert_eq!(
            failed_update.patch.status,
            Some(super::args::RequirementStatus::Failed)
        );

        let update = serde_json::from_value::<super::args::UpdateProjectTaskArgs>(json!({
            "project_task_id": "task-1",
            "patch": {
                "requirement_id": "req-1",
                "status": "failed",
                "tags": ["backend", "mcp"]
            },
            "prerequisite_project_task_ids": ["task-0"]
        }))
        .expect("update project task args");
        assert_eq!(
            update.patch.status,
            Some(super::args::ProjectTaskStatus::Failed)
        );
        assert_eq!(
            update.patch.tags,
            Some(vec!["backend".to_string(), "mcp".to_string()])
        );
        assert_eq!(
            update.prerequisite_project_task_ids,
            Some(vec!["task-0".to_string()])
        );
    }

    #[test]
    fn project_management_server_schema_snapshot_hash() {
        assert_schema_snapshot_hash(
            "project_management_server_tools",
            schemas::project_management_server_tool_definitions(),
            0x87a90c868d590da8,
        );
    }

    #[test]
    fn task_runner_builtin_schema_snapshot_hash() {
        assert_schema_snapshot_hash(
            "task_runner_builtin_tools",
            schemas::task_runner_builtin_tool_definitions(),
            0x87a90c868d590da8,
        );
    }

    fn schema_tool_names(definitions: Vec<Value>) -> Vec<String> {
        definitions
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .map(ToOwned::to_owned)
            .collect()
    }

    fn assert_schema_snapshot_hash(label: &str, definitions: Vec<Value>, expected: u64) {
        let snapshot = serde_json::to_string_pretty(&definitions).expect("serialize schema");
        let actual = fnv1a64(snapshot.as_bytes());
        assert_eq!(
            actual, expected,
            "{label} schema snapshot changed; new fnv1a64 hash is {actual:#018x}\n{snapshot}"
        );
    }

    fn fnv1a64(bytes: &[u8]) -> u64 {
        let mut hash = 0xcbf29ce484222325;
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }
}
