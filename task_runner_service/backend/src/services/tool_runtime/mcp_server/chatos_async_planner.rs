// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use crate::models::{
    now_rfc3339, CreateTaskRequest, ModelConfigRecord, TaskScheduleConfig, TaskScheduleMode,
    TaskStatus, UpdateTaskRequest,
};

use super::support::{remove_tool_schema_property, set_schema_required_fields};
use super::McpRequestContext;

mod access;
mod request_guards;
mod schema;

pub(in crate::mcp_server) use self::access::planner_agent_tool_allowed;
#[cfg(test)]
pub(in crate::mcp_server) use self::request_guards::ensure_planner_required_fields;
pub(in crate::mcp_server) use self::request_guards::{
    planner_prerequisite_create_request, planner_root_create_request, planner_update_task_request,
    require_chatos_async_source_context,
};
pub(in crate::mcp_server) use self::schema::enrich_tool_schemas_for_async_planner;
