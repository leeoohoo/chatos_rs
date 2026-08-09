// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::runtime::RuntimeSessionSnapshot;

use super::ProviderDispatcher;

impl ProviderDispatcher {
    pub async fn close_session(&self, snapshot: &RuntimeSessionSnapshot) {
        if let Err(error) = self.chatos.close_session(snapshot).await {
            tracing::warn!(
                session_id = snapshot.session_id.as_str(),
                error_code = error.code,
                "failed to close ChatOS MCP Provider session state"
            );
        }
        self.cloud_stdio.close_session(snapshot).await;
        self.plugins.close_session(snapshot).await;
    }
}
