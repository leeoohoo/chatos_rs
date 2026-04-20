pub mod fallback;
pub mod languages;
pub mod manager;
pub mod registry;
pub mod symbol_index;
pub mod types;
pub mod workspace;

use std::path::Path;

use self::types::{
    DocumentSymbolsRequest, DocumentSymbolsResponse, NavCapabilities, NavLocation,
    NavPositionRequest, ProjectContext,
};

#[axum::async_trait]
pub trait CodeNavProvider: Send + Sync {
    fn provider_id(&self) -> &'static str;

    fn language_id(&self) -> &'static str;

    fn definition_mode(&self) -> &'static str {
        "semantic"
    }

    fn references_mode(&self) -> &'static str {
        "semantic"
    }

    fn document_symbols_mode(&self) -> &'static str {
        "semantic"
    }

    fn supports_file(&self, file_path: &Path) -> bool;

    fn detect_project(&self, ctx: &ProjectContext) -> bool;

    fn capabilities(&self, ctx: &ProjectContext) -> NavCapabilities;

    async fn definition(
        &self,
        _ctx: &ProjectContext,
        _req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String> {
        Ok(Vec::new())
    }

    async fn references(
        &self,
        _ctx: &ProjectContext,
        _req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String> {
        Ok(Vec::new())
    }

    async fn document_symbols(
        &self,
        _ctx: &ProjectContext,
        _req: &DocumentSymbolsRequest,
    ) -> Result<DocumentSymbolsResponse, String> {
        Ok(DocumentSymbolsResponse {
            provider: self.provider_id().to_string(),
            language: self.language_id().to_string(),
            mode: "unsupported".to_string(),
            symbols: Vec::new(),
        })
    }
}
