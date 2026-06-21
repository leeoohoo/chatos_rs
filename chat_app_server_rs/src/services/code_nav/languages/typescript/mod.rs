use crate::services::code_nav::languages::ts_service::{
    get_semantic_document_symbols, get_semantic_locations, semantic_capabilities,
    supports_typescript_file, TsServiceMode,
};
use crate::services::code_nav::types::{
    DocumentSymbolsRequest, DocumentSymbolsResponse, NavCapabilities, NavLocation,
    NavPositionRequest, ProjectContext,
};
use crate::services::code_nav::CodeNavProvider;
use std::path::Path;

#[derive(Default)]
pub struct TypeScriptCodeNavProvider;

#[axum::async_trait]
impl CodeNavProvider for TypeScriptCodeNavProvider {
    fn provider_id(&self) -> &'static str {
        "typescript"
    }

    fn language_id(&self) -> &'static str {
        "typescript"
    }

    fn supports_file(&self, file_path: &Path) -> bool {
        supports_typescript_file(file_path)
    }

    fn detect_project(&self, ctx: &ProjectContext) -> bool {
        ctx.root.join("tsconfig.json").exists()
            || ctx.root.join("jsconfig.json").exists()
            || ctx.root.join("package.json").exists()
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
