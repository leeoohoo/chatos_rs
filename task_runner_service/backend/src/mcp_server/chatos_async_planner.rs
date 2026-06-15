use serde_json::Value;

use crate::models::{
    mcp_builtin_kind_guide, mcp_builtin_kind_values, now_rfc3339, CreateTaskRequest,
    ModelConfigRecord, TaskScheduleConfig, TaskScheduleMode, TaskStatus, UpdateTaskRequest,
};
use chatos_mcp_runtime::builtin_kind_by_any;

use super::support::{
    remove_tool_schema_property, set_schema_required_fields, set_tool_property_description,
};
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
