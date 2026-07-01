// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod helpers;
mod resolution;
mod search;

use std::fs;
use std::path::Path;

use self::helpers::extension_matches;
use self::resolution::{find_definitions, find_references};

use crate::services::code_nav::languages::shared_nav::{
    document_symbols_response, heuristic_nav_capabilities, NavSymbolLike,
};
use crate::services::code_nav::types::{
    DocumentSymbolsResponse, NavCapabilities, NavLocation, NavPositionRequest, ProjectContext,
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

impl NavSymbolLike for BasicSymbol {
    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> &str {
        &self.kind
    }

    fn line(&self) -> usize {
        self.line
    }

    fn column(&self) -> usize {
        self.column
    }

    fn end_line(&self) -> usize {
        self.end_line
    }

    fn end_column(&self) -> usize {
        self.end_column
    }
}

#[derive(Debug, Clone)]
pub struct BasicFileAnalysis {
    pub symbols: Vec<BasicSymbol>,
}

#[derive(Clone, Copy)]
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
        heuristic_nav_capabilities()
    }

    pub fn document_symbols(
        &self,
        ctx: &ProjectContext,
    ) -> Result<DocumentSymbolsResponse, String> {
        let analysis = (self.analyze_file)(&ctx.file_path)?;
        Ok(document_symbols_response(
            self.provider_id,
            self.language_id,
            "provider-heuristic",
            &analysis.symbols,
            MAX_SYMBOL_RESULTS,
        ))
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

pub async fn definition_blocking(
    spec: BasicLanguageSpec,
    ctx: &ProjectContext,
    req: &NavPositionRequest,
) -> Result<Vec<NavLocation>, String> {
    let ctx = ctx.clone();
    let req = req.clone();
    tokio::task::spawn_blocking(move || spec.definition(&ctx, &req))
        .await
        .map_err(|err| format!("code-nav basic definition task failed: {err}"))?
}

pub async fn references_blocking(
    spec: BasicLanguageSpec,
    ctx: &ProjectContext,
    req: &NavPositionRequest,
) -> Result<Vec<NavLocation>, String> {
    let ctx = ctx.clone();
    let req = req.clone();
    tokio::task::spawn_blocking(move || spec.references(&ctx, &req))
        .await
        .map_err(|err| format!("code-nav basic references task failed: {err}"))?
}

pub async fn document_symbols_blocking(
    spec: BasicLanguageSpec,
    ctx: &ProjectContext,
) -> Result<DocumentSymbolsResponse, String> {
    let ctx = ctx.clone();
    tokio::task::spawn_blocking(move || spec.document_symbols(&ctx))
        .await
        .map_err(|err| format!("code-nav basic document symbols task failed: {err}"))?
}

pub use self::helpers::{
    count_char, find_balanced_end, find_column, last_identifier, make_symbol,
    strip_c_style_comments, strip_leading_attributes,
};
