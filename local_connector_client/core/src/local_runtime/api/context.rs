// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::LocalRuntime;

use super::error::LocalRuntimeApiError;

pub(super) struct LocalRuntimeOwnerContext {
    pub(super) owner_user_id: String,
    pub(super) device_id: String,
}

pub(super) async fn owner_context(
    runtime: &LocalRuntime,
) -> Result<LocalRuntimeOwnerContext, LocalRuntimeApiError> {
    let state = runtime.state.read().await;
    let owner_user_id = state
        .auth
        .as_ref()
        .and_then(|auth| auth.user.as_ref())
        .map(|user| user.id.trim())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            LocalRuntimeApiError::conflict(
                "local_runtime_not_authenticated",
                "Local Connector must be logged in before using the local runtime",
            )
        })?
        .to_string();
    let device_id = state
        .device_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            LocalRuntimeApiError::conflict(
                "local_runtime_device_unavailable",
                "Local Connector device is not registered",
            )
        })?
        .to_string();

    Ok(LocalRuntimeOwnerContext {
        owner_user_id,
        device_id,
    })
}
