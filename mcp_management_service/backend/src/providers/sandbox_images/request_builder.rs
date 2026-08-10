// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::sandbox_images::{SANDBOX_IMAGE_PROJECT_ID_HEADER, SANDBOX_IMAGE_RUN_ID_HEADER};
use chatos_mcp_management_sdk::{ResolvedMcpRoute, SandboxProviderKind};

use crate::runtime::RuntimeSessionSnapshot;
use crate::trace_context::InternalTraceContextExt;

use super::support::{cloud_provider_ref, local_provider_ref};
use super::{
    ProviderCallError, SandboxImagesProvider, CALLER_SERVICE, LOCAL_CONNECTOR_AUDIENCE,
    SANDBOX_MANAGER_AUDIENCE, SANDBOX_SERVICE_SCOPE,
};

impl SandboxImagesProvider {
    pub(in crate::providers) fn cloud_request(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
    ) -> Result<reqwest::RequestBuilder, ProviderCallError> {
        if snapshot.project_context.sandbox_provider != SandboxProviderKind::Cloud
            || route.provider_ref.as_deref() != Some(cloud_provider_ref())
        {
            return Err(ProviderCallError::provider_unavailable(
                "cloud Sandbox Images route does not match the immutable project context",
            ));
        }
        let secret = self.cloud_internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Sandbox Manager image internal secret is not configured",
            )
        })?;
        let token = chatos_service_runtime::issue_internal_service_token(
            secret,
            CALLER_SERVICE,
            SANDBOX_MANAGER_AUDIENCE,
            SANDBOX_SERVICE_SCOPE,
            60,
        )
        .map_err(ProviderCallError::provider_unavailable)?;
        Ok(with_runtime_headers(
            self.cloud_http
                .post(format!(
                    "{}/api/internal/sandbox-images/mcp",
                    self.cloud_base_url
                ))
                .header("x-sandbox-caller", CALLER_SERVICE)
                .header("x-sandbox-internal-token", token),
            snapshot,
        ))
    }

    pub(in crate::providers) fn local_request(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
    ) -> Result<reqwest::RequestBuilder, ProviderCallError> {
        if snapshot.project_context.sandbox_provider != SandboxProviderKind::LocalConnector {
            return Err(ProviderCallError::provider_unavailable(
                "local Sandbox Images route does not match the immutable project context",
            ));
        }
        let pairing_id = snapshot
            .project_context
            .sandbox_pairing_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "local Sandbox Images route has no sandbox pairing",
                )
            })?;
        let expected_provider_ref = local_provider_ref(pairing_id);
        if route.provider_ref.as_deref() != Some(expected_provider_ref.as_str()) {
            return Err(ProviderCallError::provider_unavailable(
                "local Sandbox Images route does not match the immutable sandbox pairing",
            ));
        }
        let secret = self.local_internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Local Connector image internal secret is not configured",
            )
        })?;
        let token = chatos_service_runtime::issue_internal_service_token_for_owner(
            secret,
            CALLER_SERVICE,
            LOCAL_CONNECTOR_AUDIENCE,
            SANDBOX_SERVICE_SCOPE,
            60,
            snapshot.owner_user_id.as_str(),
        )
        .map_err(ProviderCallError::provider_unavailable)?;
        let pairing_id = urlencoding::encode(pairing_id);
        Ok(with_runtime_headers(
            self.local_http
                .post(format!(
                    "{}/api/local-connectors/sandbox-facade/{pairing_id}/api/local/sandbox/images/mcp",
                    self.local_base_url
                ))
                .header("x-local-connector-caller", CALLER_SERVICE)
                .header("x-local-connector-internal-token", token)
                .header(
                    "x-local-connector-owner-user-id",
                    snapshot.owner_user_id.as_str(),
                ),
            snapshot,
        ))
    }
}

fn with_runtime_headers(
    mut request: reqwest::RequestBuilder,
    snapshot: &RuntimeSessionSnapshot,
) -> reqwest::RequestBuilder {
    request = request.header(
        SANDBOX_IMAGE_PROJECT_ID_HEADER,
        snapshot.project_id.as_str(),
    );
    if let Some(run_id) = snapshot
        .run_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        request = request.header(SANDBOX_IMAGE_RUN_ID_HEADER, run_id);
    }
    request.with_internal_trace_context()
}
