// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod approval;
mod auth;
mod helpers;
mod history;
mod mcp_configs;
mod model_configs;
mod runtime_settings;
mod sandbox;
mod status;
mod system_permissions;
mod terminal;
mod workspace;

pub(super) use approval::{
    local_approval_settings, local_approve_pending_approval, local_deny_pending_approval,
    local_pending_approvals, local_update_approval_settings,
};
pub(super) use auth::{local_login, local_logout, local_register};
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
    local_docker_status, local_initialize_sandbox_image, local_sandbox_image_jobs,
    local_sandbox_image_mcp, local_sandbox_images, local_sandbox_leases, local_toggle_sandbox,
};
pub(super) use status::local_status;
pub(super) use system_permissions::{local_request_system_permission, local_system_permissions};
pub(super) use terminal::local_terminal_exec;
pub(super) use workspace::{local_add_workspace, local_fs_list_handler, local_remove_workspace};
