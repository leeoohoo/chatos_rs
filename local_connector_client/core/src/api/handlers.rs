// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod agent_prompts;
mod approval;
mod auth;
mod helpers;
mod history;
mod mcp_configs;
mod model_configs;
mod runtime_settings;
mod sandbox;
mod skills;
mod status;
mod system_permissions;
mod terminal;
mod workspace;

pub(super) use agent_prompts::{
    local_agent_prompt_status, local_check_agent_prompt_updates, local_update_agent_prompt_bundle,
};
pub(super) use approval::{
    local_approval_settings, local_approve_pending_approval, local_deny_pending_approval,
    local_pending_approvals, local_update_approval_settings,
};
pub(super) use auth::{
    local_desktop_ticket, local_login, local_logout, local_register, local_send_register_email_code,
};
pub(super) use history::{local_clear_command_history, local_command_history};
pub(super) use mcp_configs::{
    local_delete_mcp_config, local_disable_mcp_config, local_enable_mcp_config,
    local_get_mcp_config, local_mcp_configs, local_save_mcp_config, local_sync_mcp_config,
    local_test_mcp_config, local_update_mcp_config,
};
pub(super) use model_configs::{
    local_delete_model_config, local_model_configs, local_model_settings,
    local_preview_model_catalog, local_save_model_config, local_sync_model_config,
    local_update_model_config, local_update_model_settings,
};
pub(super) use runtime_settings::{local_runtime_settings, local_update_runtime_settings};
pub(super) use sandbox::{
    local_delete_sandbox_image, local_docker_status, local_initialize_sandbox_image,
    local_reinitialize_sandbox_image, local_sandbox_capabilities, local_sandbox_image_jobs,
    local_sandbox_image_mcp, local_sandbox_images, local_sandbox_leases, local_sandbox_settings,
    local_shutdown_sandboxes, local_toggle_sandbox, local_update_sandbox_settings,
};
pub(super) use skills::{local_skills, local_sync_skill_inventory, local_update_skill_preference};
pub(super) use status::local_status;
pub(super) use system_permissions::{local_request_system_permission, local_system_permissions};
pub(super) use terminal::local_terminal_exec;
pub(super) use workspace::{
    local_add_workspace, local_fs_list_handler, local_remove_workspace,
    local_update_workspace_project_config_trust,
};
