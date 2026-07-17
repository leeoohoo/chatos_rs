// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::Json;

use crate::local_runtime::storage::LocalRuntimeDatabaseHealth;
use crate::LocalRuntime;

use super::error::LocalRuntimeApiError;

pub(super) async fn health(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<LocalRuntimeDatabaseHealth>, LocalRuntimeApiError> {
    runtime
        .local_database()?
        .health()
        .await
        .map(Json)
        .map_err(LocalRuntimeApiError::from)
}
