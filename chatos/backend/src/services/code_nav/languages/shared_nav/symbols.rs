// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::services::code_nav::symbol_index::IndexedSymbol;
use crate::services::code_nav::types::{DocumentSymbolItem, DocumentSymbolsResponse};

pub(crate) trait NavSymbolLike {
    fn name(&self) -> &str;
    fn kind(&self) -> &str;
    fn line(&self) -> usize;
    fn column(&self) -> usize;
    fn end_line(&self) -> usize;
    fn end_column(&self) -> usize;
}

pub(crate) trait NavSearchMatchLike {
    fn path(&self) -> &str;
    fn relative_path(&self) -> &str;
    fn line(&self) -> usize;
    fn column(&self) -> usize;
    fn text(&self) -> &str;
}

macro_rules! impl_nav_symbol_like_for_field_struct {
    ($ty:ty) => {
        impl $crate::services::code_nav::languages::shared_nav::NavSymbolLike for $ty {
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
    };
}

macro_rules! impl_nav_search_match_like_for_field_struct {
    ($ty:ty) => {
        impl $crate::services::code_nav::languages::shared_nav::NavSearchMatchLike for $ty {
            fn path(&self) -> &str {
                &self.path
            }

            fn relative_path(&self) -> &str {
                &self.relative_path
            }

            fn line(&self) -> usize {
                self.line
            }

            fn column(&self) -> usize {
                self.column
            }

            fn text(&self) -> &str {
                &self.text
            }
        }
    };
}

pub(crate) use impl_nav_search_match_like_for_field_struct;
pub(crate) use impl_nav_symbol_like_for_field_struct;

pub(crate) fn indexed_symbols_from<S: NavSymbolLike>(symbols: &[S]) -> Vec<IndexedSymbol> {
    symbols
        .iter()
        .map(|symbol| IndexedSymbol {
            name: symbol.name().to_string(),
            kind: symbol.kind().to_string(),
            line: symbol.line(),
            column: symbol.column(),
            end_line: symbol.end_line(),
            end_column: symbol.end_column(),
        })
        .collect()
}

pub(crate) fn document_symbols_response<S: NavSymbolLike>(
    provider: &str,
    language: &str,
    mode: &str,
    symbols: &[S],
    max_symbols: usize,
) -> DocumentSymbolsResponse {
    let mut symbols: Vec<DocumentSymbolItem> = symbols
        .iter()
        .map(|item| DocumentSymbolItem {
            name: item.name().to_string(),
            kind: item.kind().to_string(),
            line: item.line(),
            column: item.column(),
            end_line: item.end_line(),
            end_column: item.end_column(),
        })
        .collect();
    if symbols.len() > max_symbols {
        symbols.truncate(max_symbols);
    }

    DocumentSymbolsResponse {
        provider: provider.to_string(),
        language: language.to_string(),
        mode: mode.to_string(),
        symbols,
    }
}
