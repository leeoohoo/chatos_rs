// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::services::project_local_cache::cache_key;

use super::{IndexedSymbol, ProjectIndexedSymbol, ProjectSymbolIndex, ProjectSymbolIndexSnapshot};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedIndexedSymbol {
    name: String,
    kind: String,
    line: usize,
    column: usize,
    end_line: usize,
    end_column: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedProjectIndexedSymbol {
    path: String,
    relative_path: String,
    symbol: PersistedIndexedSymbol,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct PersistedProjectSymbolIndex {
    symbols_by_name: HashMap<String, Vec<PersistedProjectIndexedSymbol>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PersistedProjectSymbolIndexEntry {
    pub(super) snapshot: ProjectSymbolIndexSnapshot,
    pub(super) index: PersistedProjectSymbolIndex,
}

pub(super) fn symbol_index_cache_path(provider_id: &str) -> String {
    format!("code_nav/index-{}.json", cache_key(provider_id))
}

pub(super) fn persisted_project_symbol_index_entry(
    snapshot: ProjectSymbolIndexSnapshot,
    index: &ProjectSymbolIndex,
) -> PersistedProjectSymbolIndexEntry {
    PersistedProjectSymbolIndexEntry {
        snapshot,
        index: to_persisted_index(index),
    }
}

pub(super) fn from_persisted_index(index: PersistedProjectSymbolIndex) -> ProjectSymbolIndex {
    ProjectSymbolIndex {
        symbols_by_name: index
            .symbols_by_name
            .into_iter()
            .map(|(name, items)| {
                (
                    name,
                    items
                        .into_iter()
                        .map(|item| ProjectIndexedSymbol {
                            path: item.path,
                            relative_path: item.relative_path,
                            symbol: IndexedSymbol {
                                name: item.symbol.name,
                                kind: item.symbol.kind,
                                line: item.symbol.line,
                                column: item.symbol.column,
                                end_line: item.symbol.end_line,
                                end_column: item.symbol.end_column,
                            },
                        })
                        .collect(),
                )
            })
            .collect(),
    }
}

fn to_persisted_index(index: &ProjectSymbolIndex) -> PersistedProjectSymbolIndex {
    PersistedProjectSymbolIndex {
        symbols_by_name: index
            .symbols_by_name
            .iter()
            .map(|(name, items)| {
                (
                    name.clone(),
                    items
                        .iter()
                        .map(|item| PersistedProjectIndexedSymbol {
                            path: item.path.clone(),
                            relative_path: item.relative_path.clone(),
                            symbol: PersistedIndexedSymbol {
                                name: item.symbol.name.clone(),
                                kind: item.symbol.kind.clone(),
                                line: item.symbol.line,
                                column: item.symbol.column,
                                end_line: item.symbol.end_line,
                                end_column: item.symbol.end_column,
                            },
                        })
                        .collect(),
                )
            })
            .collect(),
    }
}
