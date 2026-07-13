// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) fn task_tool_definitions() -> Vec<Value> {
    vec![
        tool_definition(
            "list_tasks",
            "List historical Task Runner tasks created for the current owner and current task profile. Default profile returns ordinary tasks; Chatos Plan profile returns planning tasks. Use keyword for fuzzy search and limit/offset to page older history.",
            json!({
                "type": "object",
                "properties": {
                    "status": {
                        "type": "string",
                        "enum": task_status_values(),
                        "description": "Optional status filter."
                    },
                    "keyword": {
                        "type": "string",
                        "description": "Fuzzy search across task id, title, objective, description, result summary, and tags. Use this first when the user refers to earlier work."
                    },
                    "tag": { "type": "string", "description": "Exact tag filter." },
                    "scheduled_only": { "type": "boolean", "description": "Only return scheduled or async tasks." },
                    "parent_task_id": { "type": "string", "description": "Only return direct subtasks of this task." },
                    "source_run_id": { "type": "string", "description": "Only return tasks created from a specific source run." },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 500,
                        "description": "Maximum result count. Results are sorted by most recently updated first."
                    },
                    "offset": {
                        "type": "integer",
                        "minimum": 0,
                        "maximum": 100000,
                        "description": "Number of matching tasks to skip for paging older history."
                    }
                },
                "additionalProperties": false
            }),
        ),
        tool_definition(
            "get_task",
            "Get one Task Runner task by id.",
            required_object_schema(
                json!({
                    "task_id": { "type": "string", "minLength": 1 }
                }),
                &["task_id"],
            ),
        ),
        tool_definition(
            "get_task_stats",
            "Get aggregate task counts for the Task Runner workspace.",
            empty_object_schema(),
        ),
        tool_definition(
            "create_task",
            "Create a new Task Runner task for the current authenticated agent. Ownership and memory scope are assigned automatically by Task Runner.",
            create_task_schema(),
        ),
        tool_definition(
            "list_mcp_builtin_catalog",
            "List builtin MCP capabilities that can be enabled for newly created Task Runner tasks, including use cases, capabilities, and current tool names.",
            empty_object_schema(),
        ),
        tool_definition(
            "list_external_mcp_configs",
            "List enabled external MCP configs visible to the current authenticated user. Use the returned id values as external_mcp_config_ids when a new task needs those external tools.",
            empty_object_schema(),
        ),
        tool_definition(
            "list_available_skills",
            "List Local Connector Skills currently enabled by the user and available for Task Runner. Use returned id values as selected_skill_ids when creating tasks.",
            empty_object_schema(),
        ),
        tool_definition(
            "create_tasks_with_prerequisites",
            "Create multiple Task Runner tasks in one call and connect prerequisite edges using temporary client_ref values plus existing prerequisite_task_ids. Use this when new prerequisite tasks do not have real task ids yet.",
            create_tasks_with_prerequisites_schema(),
        ),
        tool_definition(
            "create_project_execution_tasks",
            "Create concrete Task Runner execution tasks for Chatos project requirement execution and bind every created task to its project-management task/work item. Use this instead of create_tasks_with_prerequisites for project requirement execution.",
            create_project_execution_tasks_schema(),
        ),
        tool_definition(
            "update_task",
            "Update metadata for an existing Task Runner task. Do not use this to change execution status; create a new task for new work or use cancel_task for obsolete work.",
            required_object_schema(
                json!({
                    "task_id": { "type": "string", "minLength": 1 },
                    "patch": update_task_schema()
                }),
                &["task_id", "patch"],
            ),
        ),
        tool_definition(
            "set_task_prerequisites",
            "Replace the direct prerequisite task ids for one existing Task Runner task.",
            required_object_schema(
                json!({
                    "task_id": { "type": "string", "minLength": 1 },
                    "prerequisite_task_ids": prerequisite_task_ids_schema()
                }),
                &["task_id", "prerequisite_task_ids"],
            ),
        ),
        tool_definition(
            "cancel_task",
            "Cancel a pending or running Task Runner task because it conflicts with the user's latest intent. A human-readable reason is required. Dependent pending/running tasks are cancelled automatically by Task Runner.",
            required_object_schema(
                json!({
                    "task_id": { "type": "string", "minLength": 1 },
                    "reason": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": 1000,
                        "description": "Why this task no longer matches the user's current intent. This reason is sent back to Chatos in the task.cancelled callback."
                    },
                    "replacement_task_ids": {
                        "type": "array",
                        "items": { "type": "string", "minLength": 1 },
                        "uniqueItems": true
                    }
                }),
                &["task_id", "reason"],
            ),
        ),
        tool_definition(
            "wait_for_task_completion",
            "Use after the requested Task Runner tasks have been created or adjusted. It confirms that the arranged tasks should continue through Task Runner's normal background execution flow.",
            empty_object_schema(),
        ),
        tool_definition(
            "get_task_dependency_graph",
            "Get direct and transitive prerequisite tasks for one Task Runner task.",
            required_object_schema(
                json!({
                    "task_id": { "type": "string", "minLength": 1 }
                }),
                &["task_id"],
            ),
        ),
        tool_definition(
            "delete_task",
            "Delete a Task Runner task by id.",
            required_object_schema(
                json!({
                    "task_id": { "type": "string", "minLength": 1 }
                }),
                &["task_id"],
            ),
        ),
        tool_definition(
            "batch_update_task_status",
            "Admin-only status maintenance tool. Chatos agents should not call this; create new tasks for new work or use cancel_task for obsolete work.",
            required_object_schema(
                json!({
                    "task_ids": {
                        "type": "array",
                        "items": { "type": "string", "minLength": 1 },
                        "minItems": 1
                    },
                    "status": { "type": "string", "enum": task_status_values() }
                }),
                &["task_ids", "status"],
            ),
        ),
        tool_definition(
            "batch_delete_tasks",
            "Delete multiple Task Runner tasks by id.",
            required_object_schema(
                json!({
                    "task_ids": {
                        "type": "array",
                        "items": { "type": "string", "minLength": 1 },
                        "minItems": 1
                    }
                }),
                &["task_ids"],
            ),
        ),
    ]
}
