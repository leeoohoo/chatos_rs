// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod context;
mod errors;
mod plan;
mod status;
mod sync;
mod tasks;
mod types;
mod values;

pub(super) use context::{
    create_execution_message, resolve_or_create_execution_session, select_contact_runtime,
};
pub(super) use errors::HandlerError;
pub(super) use plan::{
    add_requirement_work_item_dependencies, collect_requirement_execution_scope,
    parse_requirements, parse_work_items, project_plan_array, project_plan_value,
    requirement_dependency_map, topological_work_item_order, validate_requirement_prerequisites,
    work_item_dependency_map,
};
pub(super) use status::{
    is_done_status, task_runner_callback_event_for_status, task_runner_status_is_active,
    task_runner_status_is_success,
};
pub(super) use sync::{
    load_execution_links_for_work_items, mark_execution_messages_for_stop,
    sync_execution_link_status, sync_requirement_execution_state,
};
pub(super) use tasks::ensure_requirement_execution_not_active;
pub(super) use types::{RequirementPlanItem, WorkItemPlanItem};
pub(super) use values::value_string;
