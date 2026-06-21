use crate::services::code_nav::languages::ts_service::{
    get_semantic_document_symbols, get_semantic_locations, semantic_capabilities,
    supports_javascript_file, TsServiceMode,
};
use crate::services::code_nav::types::{
    DocumentSymbolsRequest, DocumentSymbolsResponse, NavCapabilities, NavLocation,
    NavPositionRequest, ProjectContext,
};
use crate::services::code_nav::CodeNavProvider;
use std::path::Path;

#[derive(Default)]
pub struct JavaScriptCodeNavProvider;

#[axum::async_trait]
impl CodeNavProvider for JavaScriptCodeNavProvider {
    fn provider_id(&self) -> &'static str {
        "javascript"
    }

    fn language_id(&self) -> &'static str {
        "javascript"
    }

    fn supports_file(&self, file_path: &Path) -> bool {
        supports_javascript_file(file_path)
    }

    fn detect_project(&self, ctx: &ProjectContext) -> bool {
        ctx.root.join("package.json").exists()
            || ctx.root.join("jsconfig.json").exists()
            || ctx.root.join("tsconfig.json").exists()
    }

    fn capabilities(&self, _ctx: &ProjectContext) -> NavCapabilities {
        semantic_capabilities()
    }

    async fn definition(
        &self,
        ctx: &ProjectContext,
        req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String> {
        get_semantic_locations(TsServiceMode::Definition, ctx, req).await
    }

    async fn references(
        &self,
        ctx: &ProjectContext,
        req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String> {
        get_semantic_locations(TsServiceMode::References, ctx, req).await
    }

    async fn document_symbols(
        &self,
        ctx: &ProjectContext,
        _req: &DocumentSymbolsRequest,
    ) -> Result<DocumentSymbolsResponse, String> {
        get_semantic_document_symbols(ctx).await
    }
}
