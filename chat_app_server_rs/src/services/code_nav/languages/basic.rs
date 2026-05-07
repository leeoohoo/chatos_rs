mod helpers;
mod resolution;
mod search;

use std::fs;
use std::path::Path;

use self::helpers::extension_matches;
use self::resolution::{find_definitions, find_references};

use crate::services::code_nav::types::{
    DocumentSymbolItem, DocumentSymbolsResponse, NavCapabilities, NavLocation, NavPositionRequest,
    ProjectContext,
};

const MAX_SYMBOL_RESULTS: usize = 200;

#[derive(Debug, Clone)]
pub struct BasicSymbol {
    pub name: String,
    pub kind: String,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

#[derive(Debug, Clone)]
pub struct BasicFileAnalysis {
    pub symbols: Vec<BasicSymbol>,
}

pub struct BasicLanguageSpec {
    pub provider_id: &'static str,
    pub language_id: &'static str,
    pub extensions: &'static [&'static str],
    pub ignored_dirs: &'static [&'static str],
    pub project_files: &'static [&'static str],
    pub project_extensions: &'static [&'static str],
    pub analyze_file: fn(&Path) -> Result<BasicFileAnalysis, String>,
    pub classify_declaration: fn(&str, &str) -> Option<&'static str>,
}

impl BasicLanguageSpec {
    pub fn supports_file(&self, file_path: &Path) -> bool {
        extension_matches(file_path, self.extensions)
    }

    pub fn detect_project(&self, ctx: &ProjectContext) -> bool {
        self.project_files
            .iter()
            .any(|marker| ctx.root.join(marker).exists())
            || fs::read_dir(&ctx.root)
                .ok()
                .into_iter()
                .flat_map(|entries| entries.filter_map(Result::ok))
                .any(|entry| extension_matches(&entry.path(), self.project_extensions))
    }

    pub fn capabilities(&self) -> NavCapabilities {
        NavCapabilities {
            supports_definition: true,
            supports_references: true,
            supports_document_symbols: true,
        }
    }

    pub fn document_symbols(
        &self,
        ctx: &ProjectContext,
    ) -> Result<DocumentSymbolsResponse, String> {
        let analysis = (self.analyze_file)(&ctx.file_path)?;
        let mut symbols: Vec<DocumentSymbolItem> = analysis
            .symbols
            .into_iter()
            .map(|item| DocumentSymbolItem {
                name: item.name,
                kind: item.kind,
                line: item.line,
                column: item.column,
                end_line: item.end_line,
                end_column: item.end_column,
            })
            .collect();
        if symbols.len() > MAX_SYMBOL_RESULTS {
            symbols.truncate(MAX_SYMBOL_RESULTS);
        }

        Ok(DocumentSymbolsResponse {
            provider: self.provider_id.to_string(),
            language: self.language_id.to_string(),
            mode: "provider-heuristic".to_string(),
            symbols,
        })
    }

    pub fn definition(
        &self,
        ctx: &ProjectContext,
        req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String> {
        find_definitions(self, ctx, req)
    }

    pub fn references(
        &self,
        ctx: &ProjectContext,
        req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String> {
        find_references(self, ctx, req)
    }
}

pub use self::helpers::{
    count_char, find_balanced_end, find_column, last_identifier, make_symbol,
    strip_c_style_comments, strip_leading_attributes,
};
