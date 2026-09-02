// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod harness;
mod http;

pub use harness::{
    create_harness_project_repo, ensure_harness_user_public_register_on_login,
    get_harness_api_access_for_user, provision_harness_user_public_register,
    provision_harness_user_public_register_result, HarnessApiAccessResponse,
    HarnessProjectRepoCreateRequest, HarnessProjectRepoResponse,
};
