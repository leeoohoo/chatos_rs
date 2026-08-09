// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod relay;
mod runtime;

pub(crate) use relay::{
    handle_remote_connection_command_request, handle_remote_connection_test_request,
};
