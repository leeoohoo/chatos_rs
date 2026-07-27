// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) async fn list_plugin_audit(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<PluginAuditQuery>,
) -> Result<Json<ListResponse<PluginAuditLogRecord>>, ApiError> {
    ensure_super_admin(&user)?;
    state
        .store
        .list_plugin_audit(&query)
        .await
        .map(Json)
        .map_err(ApiError::internal)
}
