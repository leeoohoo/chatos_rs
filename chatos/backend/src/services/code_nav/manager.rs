// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use super::fallback::{fallback_definition, fallback_document_symbols, fallback_references};
use super::local_connector;
use super::registry::default_providers;
use super::types::{
    DocumentSymbolsRequest, DocumentSymbolsResponse, NavCapabilitiesResponse, NavLocationsResponse,
    NavPositionRequest, ProjectContext,
};
use super::workspace::build_project_context;
use super::CodeNavProvider;

#[derive(Clone)]
pub struct CodeNavManager {
    providers: Vec<Arc<dyn CodeNavProvider>>,
}

impl Default for CodeNavManager {
    fn default() -> Self {
        Self::new(default_providers())
    }
}

impl CodeNavManager {
    pub fn new(providers: Vec<Arc<dyn CodeNavProvider>>) -> Self {
        Self { providers }
    }

    pub async fn capabilities(
        &self,
        project_root: &str,
        file_path: &str,
    ) -> Result<NavCapabilitiesResponse, String> {
        if local_connector::is_local_connector_request(project_root) {
            return local_connector::capabilities(project_root, file_path).await;
        }

        let ctx = build_project_context(project_root, file_path)?;
        let provider = self.resolve_provider(&ctx);
        let capabilities = provider
            .as_ref()
            .map(|provider| provider.capabilities(&ctx))
            .unwrap_or_else(|| super::types::NavCapabilities {
                supports_definition: false,
                supports_references: false,
                supports_document_symbols: false,
            });

        Ok(NavCapabilitiesResponse {
            language: ctx.language.clone(),
            provider: provider
                .as_ref()
                .map(|provider| provider.provider_id().to_string())
                .unwrap_or_else(|| "fallback".to_string()),
            supports_definition: capabilities.supports_definition,
            supports_references: capabilities.supports_references,
            supports_document_symbols: capabilities.supports_document_symbols,
            fallback_available: true,
        })
    }

    pub async fn definition(
        &self,
        request: &NavPositionRequest,
    ) -> Result<NavLocationsResponse, String> {
        if local_connector::is_local_connector_request(request.project_root.as_str()) {
            return local_connector::definition(request).await;
        }

        let ctx = build_project_context(&request.project_root, &request.file_path)?;
        let provider = self.resolve_provider(&ctx);

        if let Some(provider) = provider {
            let capabilities = provider.capabilities(&ctx);
            if capabilities.supports_definition {
                if let Ok(locations) = provider.definition(&ctx, request).await {
                    if !locations.is_empty() {
                        return Ok(NavLocationsResponse {
                            provider: provider.provider_id().to_string(),
                            language: ctx.language.clone(),
                            mode: provider.definition_mode().to_string(),
                            token: None,
                            locations,
                        });
                    }
                }
                return fallback_definition_blocking(ctx, request.clone(), provider.provider_id())
                    .await;
            }
            return fallback_definition_blocking(ctx, request.clone(), provider.provider_id())
                .await;
        }

        fallback_definition_blocking(ctx, request.clone(), "fallback").await
    }

    pub async fn references(
        &self,
        request: &NavPositionRequest,
    ) -> Result<NavLocationsResponse, String> {
        if local_connector::is_local_connector_request(request.project_root.as_str()) {
            return local_connector::references(request).await;
        }

        let ctx = build_project_context(&request.project_root, &request.file_path)?;
        let provider = self.resolve_provider(&ctx);

        if let Some(provider) = provider {
            let capabilities = provider.capabilities(&ctx);
            if capabilities.supports_references {
                if let Ok(locations) = provider.references(&ctx, request).await {
                    if !locations.is_empty() {
                        return Ok(NavLocationsResponse {
                            provider: provider.provider_id().to_string(),
                            language: ctx.language.clone(),
                            mode: provider.references_mode().to_string(),
                            token: None,
                            locations,
                        });
                    }
                }
                return fallback_references_blocking(ctx, request.clone(), provider.provider_id())
                    .await;
            }
            return fallback_references_blocking(ctx, request.clone(), provider.provider_id())
                .await;
        }

        fallback_references_blocking(ctx, request.clone(), "fallback").await
    }

    pub async fn document_symbols(
        &self,
        request: &DocumentSymbolsRequest,
    ) -> Result<DocumentSymbolsResponse, String> {
        if local_connector::is_local_connector_request(request.project_root.as_str()) {
            return local_connector::document_symbols(request).await;
        }

        let ctx = build_project_context(&request.project_root, &request.file_path)?;
        let provider = self.resolve_provider(&ctx);

        if let Some(provider) = provider {
            let capabilities = provider.capabilities(&ctx);
            if capabilities.supports_document_symbols {
                if let Ok(response) = provider.document_symbols(&ctx, request).await {
                    if !response.symbols.is_empty() {
                        return Ok(response);
                    }
                }
                return fallback_document_symbols_blocking(
                    ctx,
                    request.clone(),
                    provider.provider_id(),
                )
                .await;
            }
            return fallback_document_symbols_blocking(
                ctx,
                request.clone(),
                provider.provider_id(),
            )
            .await;
        }

        fallback_document_symbols_blocking(ctx, request.clone(), "fallback").await
    }

    fn resolve_provider(&self, ctx: &ProjectContext) -> Option<Arc<dyn CodeNavProvider>> {
        self.providers
            .iter()
            .find(|provider| provider.supports_file(&ctx.file_path) && provider.detect_project(ctx))
            .cloned()
            .or_else(|| {
                self.providers
                    .iter()
                    .find(|provider| provider.supports_file(&ctx.file_path))
                    .cloned()
            })
    }
}

async fn fallback_definition_blocking(
    ctx: ProjectContext,
    request: NavPositionRequest,
    provider_id: &'static str,
) -> Result<NavLocationsResponse, String> {
    tokio::task::spawn_blocking(move || fallback_definition(&ctx, &request, provider_id))
        .await
        .map_err(|err| format!("code-nav fallback task failed: {err}"))?
}

async fn fallback_references_blocking(
    ctx: ProjectContext,
    request: NavPositionRequest,
    provider_id: &'static str,
) -> Result<NavLocationsResponse, String> {
    tokio::task::spawn_blocking(move || fallback_references(&ctx, &request, provider_id))
        .await
        .map_err(|err| format!("code-nav fallback task failed: {err}"))?
}

async fn fallback_document_symbols_blocking(
    ctx: ProjectContext,
    request: DocumentSymbolsRequest,
    provider_id: &'static str,
) -> Result<DocumentSymbolsResponse, String> {
    tokio::task::spawn_blocking(move || fallback_document_symbols(&ctx, &request, provider_id))
        .await
        .map_err(|err| format!("code-nav fallback task failed: {err}"))?
}
