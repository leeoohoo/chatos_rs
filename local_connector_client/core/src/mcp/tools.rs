// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod browser;
mod code;
mod project;
mod terminal_controller;

pub(crate) use browser::{local_browser_conversation_id, local_browser_tools_service_for_root};
pub(crate) use code::{code_maintainer_service_for_root, normalize_code_maintainer_arguments};
pub(crate) use project::{normalize_request_project_relative_path, request_project_root};
pub(crate) use terminal_controller::call_local_terminal_controller_tool;

#[cfg(test)]
pub(crate) use code::code_maintainer_structured_result;
