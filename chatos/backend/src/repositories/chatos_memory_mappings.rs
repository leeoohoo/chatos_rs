// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod contacts;
mod project_links;
mod projects;
mod support;

pub use contacts::{
    create_contact_idempotent, delete_contact_by_id, get_contact_by_id,
    get_contact_by_user_and_agent, list_contacts, list_contacts_by_ids,
    update_contact_task_runner_config, UpdateContactTaskRunnerConfigInput,
};
pub use project_links::{
    delete_project_agent_link, list_project_agent_links_by_contact,
    list_project_agent_links_by_project, touch_project_agent_link_session,
    upsert_project_agent_link, TouchProjectAgentLinkSessionInput, UpsertProjectAgentLinkInput,
};
pub use projects::{
    get_project_by_user_and_project_id, list_memory_projects, list_projects_by_ids,
    upsert_memory_project, UpsertMemoryProjectInput,
};
pub use support::default_project_name;
