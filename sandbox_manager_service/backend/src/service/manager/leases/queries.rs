// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl SandboxManager {
    pub async fn get(
        &self,
        auth: &SandboxAuthContext,
        sandbox_id: &str,
    ) -> Result<SandboxLeaseRecord, ApiError> {
        let record = self.require_sandbox(sandbox_id).await?;
        auth.ensure_lease_access(&record, SCOPE_LEASE_READ)?;
        Ok(record)
    }

    pub async fn list(
        &self,
        auth: &SandboxAuthContext,
        query: ListSandboxQuery,
    ) -> Result<Vec<SandboxLeaseRecord>, ApiError> {
        let query = auth.scoped_list_query(query)?;
        self.store
            .list_leases(query)
            .await
            .map_err(ApiError::internal)
    }

    pub async fn events(
        &self,
        auth: &SandboxAuthContext,
        sandbox_id: &str,
    ) -> Result<Vec<SandboxEventRecord>, ApiError> {
        let record = self.require_sandbox(sandbox_id).await?;
        auth.ensure_lease_access(&record, SCOPE_LEASE_READ)?;
        self.store
            .list_events(sandbox_id)
            .await
            .map_err(ApiError::internal)
    }
}
