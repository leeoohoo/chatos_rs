// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_mcp_management_sdk::ResolvedMcpRoute;
use chatos_plugin_management_sdk::SystemAgentKey;
use serde_json::Value;

use super::ChatosProvider;

impl ChatosProvider {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::providers) async fn prepare_routes(
        &self,
        _routes: &mut [ResolvedMcpRoute],
        _runtime_session_id: &str,
        _owner_user_id: &str,
        _agent_key: SystemAgentKey,
        _project_id: &str,
        _run_id: Option<&str>,
        _source_session_id: Option<&str>,
        _expires_at_unix: i64,
    ) -> HashMap<String, Vec<Value>> {
        HashMap::new()
    }
}
