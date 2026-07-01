// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub(in crate::mcp_server) fn planner_agent_tool_allowed(name: &str) -> bool {
    matches!(
        name,
        "list_tasks"
            | "get_task"
            | "get_task_stats"
            | "create_task"
            | "list_mcp_builtin_catalog"
            | "list_external_mcp_configs"
            | "search_installed_skills"
            | "get_skill_detail"
            | "create_tasks_with_prerequisites"
            | "update_task"
            | "set_task_prerequisites"
            | "cancel_task"
            | "wait_for_task_completion"
            | "get_task_dependency_graph"
    )
}
